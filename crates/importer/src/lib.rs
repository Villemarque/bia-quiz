//! `bia-import` — one-shot importer for `annales-bia.tsv`.
//!
//! Contract (plan §8): a quoted TSV is the single source of truth. Any row that cannot
//! be used is **warned and skipped** — there is no quarantine table. Re-running is safe
//! (upsert on `qid`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use bia_core::models::{Chapter, NewChapter, NewFigure, NewQuestion, NewQuestionChapter};
use bia_core::schema::{chapters, figures, question_chapters, questions};
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use flate2::read::GzDecoder;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("../server/migrations");
pub const DEFAULT_UPSTREAM: &str =
    "https://github.com/Villemarque/bia-annales-database-csv/archive/refs/heads/master.tar.gz";
/// Baked-in chapter titles; the importer has no chapters configuration.
const CHAPTERS: [(i32, &str); 15] = [
    (0, "1.1 Les aéronefs"),
    (1, "1.2 Instrumentation"),
    (2, "1.3 Moteurs"),
    (3, "2.1 La sustentation de l'aile"),
    (4, "2.2 Le vol stabilisé"),
    (5, "2.3 L'aérostation et le vol spatial"),
    (6, "3.1 L'atmosphère"),
    (7, "3.2 Les masses d'air et les fronts"),
    (8, "3.3 Les nuages"),
    (9, "3.4 Les vents"),
    (10, "3.5 Les phénomènes dangereux"),
    (11, "3.6 L'information météo"),
    (12, "4.1 Réglementation"),
    (13, "4.2 Sécurité des vols (SV) et facteurs humains (FH)"),
    (14, "4.3 Navigation"),
];

const HEADER: [&str; 15] = [
    "qid",
    "year",
    "subject",
    "no_subject",
    "no",
    "content_verbatim",
    "content_fixed",
    "choice_a",
    "choice_b",
    "choice_c",
    "choice_d",
    "answer",
    "chapter",
    "attachment_link",
    "mixed_choices",
];

#[derive(clap::Parser, Debug)]
#[command(name = "bia-import", about = "Import the BIA annales into SQLite")]
pub struct Args {
    #[arg(long, default_value = "annales-bia.tsv")]
    pub tsv: PathBuf,
    #[arg(long, default_value = "bia.sqlite3")]
    pub db: PathBuf,
    #[arg(long, default_value = "data/figures")]
    pub images: PathBuf,
    #[arg(long, default_value = "import-report.txt")]
    pub report: PathBuf,
    /// Download the upstream TSV and figures before importing.
    #[arg(long)]
    pub fetch: bool,
    /// Upstream source archive (tar.gz), e.g. a tag or commit tarball.
    #[arg(long, default_value = DEFAULT_UPSTREAM)]
    pub upstream: String,
}

/// Download the upstream archive, then extract only what we own:
/// `site/static/annales-bia.tsv` and `site/static/img-sujets/*.jpeg`.
fn fetch_upstream(url: &str, tsv: &Path, images: &Path) -> anyhow::Result<()> {
    println!("fetch: downloading {url}");
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(120))
        .build();
    let response = agent
        .get(url)
        .call()
        .map_err(|err| anyhow::anyhow!("download failed: {err}"))?;
    let status = response.status();
    if !(200..300).contains(&status) {
        anyhow::bail!("download failed: HTTP {status}");
    }
    extract_archive(response.into_reader(), tsv, images)
}

pub fn extract_archive<R: Read>(reader: R, tsv: &Path, images: &Path) -> anyhow::Result<()> {
    let mut archive = tar::Archive::new(GzDecoder::new(reader));
    let mut found_tsv = false;
    let mut figures = 0usize;

    if let Some(parent) = tsv.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::create_dir_all(images)?;

    for entry in archive
        .entries()
        .map_err(|e| anyhow::anyhow!("invalid archive: {e}"))?
    {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let text = path.to_string_lossy();

        if text.ends_with("site/static/annales-bia.tsv") {
            let partial = tsv.with_extension("tsv.part");
            entry.unpack(&partial)?;
            std::fs::rename(&partial, tsv)?;
            found_tsv = true;
        } else if text.contains("/site/static/img-sujets/")
            || text.starts_with("site/static/img-sujets/")
        {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".jpeg") {
                    entry.unpack(images.join(name))?;
                    figures += 1;
                }
            }
        }
    }

    if !found_tsv {
        anyhow::bail!("archive has no site/static/annales-bia.tsv");
    }
    println!(
        "fetch: updated {} and {} ({} figures)",
        tsv.display(),
        images.display(),
        figures
    );
    Ok(())
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Report {
    pub read: usize,
    pub imported: usize,
    pub figures: usize,
    pub skipped: Vec<Skipped>,
    pub reasons: BTreeMap<String, usize>,
    pub per_chapter: BTreeMap<i32, usize>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Skipped {
    pub record: usize,
    pub line: u64,
    pub qid: String,
    pub reason: String,
}

impl Report {
    fn skip(&mut self, record: usize, line: u64, qid: &str, reason: impl Into<String>) {
        let reason = reason.into();
        *self.reasons.entry(reason.clone()).or_default() += 1;
        self.skipped.push(Skipped {
            record,
            line,
            qid: qid.to_string(),
            reason,
        });
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str("bia-import report\n");
        out.push_str("=================\n");
        out.push_str(&format!("records read : {}\n", self.read));
        out.push_str(&format!("imported     : {}\n", self.imported));
        out.push_str(&format!("figures       : {}\n", self.figures));
        out.push_str(&format!("skipped      : {}\n", self.skipped.len()));
        out.push_str("\nskipped by reason:\n");
        for (reason, count) in &self.reasons {
            out.push_str(&format!("  {reason:<24} {count}\n"));
        }
        out.push_str("\nimported per chapter:\n");
        for (chapter, count) in &self.per_chapter {
            out.push_str(&format!("  {chapter:>2}  {count}\n"));
        }
        if !self.skipped.is_empty() {
            out.push_str("\nskipped rows (record index, physical line, qid, reason):\n");
            for s in &self.skipped {
                out.push_str(&format!(
                    "  #{:<5} line {:<6} {:<12} {}\n",
                    s.record, s.line, s.qid, s.reason
                ));
            }
        }
        out
    }
}

pub fn setup_connection(path: &Path) -> Result<SqliteConnection> {
    let mut conn = SqliteConnection::establish(&path.to_string_lossy())
        .with_context(|| format!("cannot open SQLite database {}", path.display()))?;
    conn.batch_execute(
        "PRAGMA foreign_keys = ON;\n\
         PRAGMA journal_mode = WAL;\n\
         PRAGMA busy_timeout = 5000;\n\
         PRAGMA synchronous = NORMAL;",
    )
    .context("cannot set SQLite pragmas")?;
    conn.run_pending_migrations(MIGRATIONS)
        .map_err(|e| anyhow::anyhow!("migration failed: {e}"))?;
    Ok(conn)
}

fn seed_chapters(conn: &mut SqliteConnection) -> Result<()> {
    for (number, title) in CHAPTERS {
        diesel::insert_into(chapters::table)
            .values(NewChapter {
                number,
                title: title.to_string(),
            })
            .on_conflict(chapters::number)
            .do_update()
            .set(chapters::title.eq(title))
            .execute(conn)?;
    }
    Ok(())
}

#[derive(Debug)]
struct Prepared {
    qid: String,
    year: i32,
    subject: i32,
    text: String,
    used_fixed: bool,
    choices: [String; 4],
    answer_idx: i32,
    mixed_choices: bool,
    attachment_link: Option<String>,
    chapters: Vec<i32>,
    seconds: Option<i32>,
}

fn field(record: &csv::StringRecord, index: usize) -> &str {
    record.get(index).unwrap_or("")
}

fn parse_row(
    record: &csv::StringRecord,
    images: &Path,
    known_chapters: &HashMap<i32, i32>,
    seconds_idx: Option<usize>,
) -> std::result::Result<Prepared, &'static str> {
    let qid = field(record, 0).trim();
    if qid.is_empty() {
        return Err("missing_qid");
    }

    let year: i32 = field(record, 1).trim().parse().map_err(|_| "bad_year")?;
    if year <= 0 {
        return Err("bad_year");
    }
    let subject: i32 = field(record, 2).trim().parse().map_err(|_| "bad_subject")?;
    if !(0..=5).contains(&subject) {
        return Err("bad_subject");
    }

    let fixed = field(record, 6).trim();
    let verbatim = field(record, 5).trim();
    let used_fixed = !fixed.is_empty();
    let text = if used_fixed { fixed } else { verbatim };
    if text.is_empty() {
        return Err("empty_text");
    }

    let mut choices: [String; 4] = std::array::from_fn(|_| String::new());
    for (i, slot) in choices.iter_mut().enumerate() {
        let value = field(record, 7 + i).trim();
        if value.is_empty() {
            return Err("empty_choice");
        }
        *slot = value.to_string();
    }

    let answer_idx: i32 = field(record, 11).trim().parse().map_err(|_| "bad_answer")?;
    if !(0..=3).contains(&answer_idx) {
        return Err("bad_answer");
    }

    let chapter_field = field(record, 12).trim();
    let mut chapter_numbers = Vec::new();
    if !chapter_field.is_empty() {
        for part in chapter_field.split(',') {
            let number: i32 = part.trim().parse().map_err(|_| "bad_chapter")?;
            if !(0..=14).contains(&number) || !known_chapters.contains_key(&number) {
                return Err("bad_chapter");
            }
            if !chapter_numbers.contains(&number) {
                chapter_numbers.push(number);
            }
        }
    }

    let attachment = field(record, 13).trim();
    let attachment_link = if attachment.is_empty() {
        None
    } else {
        if !attachment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err("bad_attachment");
        }
        let path = images.join(format!("{attachment}.jpeg"));
        if !path.is_file() {
            return Err("missing_figure");
        }
        Some(attachment.to_string())
    };

    let mixed_choices = match field(record, 14).trim().to_ascii_uppercase().as_str() {
        "" | "FALSE" => false,
        "TRUE" => true,
        _ => return Err("bad_mixed_choices"),
    };

    let seconds = match seconds_idx {
        Some(index) => {
            let raw = field(record, index).trim();
            if raw.is_empty() {
                None
            } else {
                let parsed: i32 = raw.parse().map_err(|_| "bad_seconds")?;
                if !(5..=600).contains(&parsed) {
                    return Err("bad_seconds");
                }
                Some(parsed)
            }
        }
        None => None,
    };

    Ok(Prepared {
        qid: qid.to_string(),
        year,
        subject,
        text: text.to_string(),
        used_fixed,
        choices,
        answer_idx,
        mixed_choices,
        attachment_link,
        chapters: chapter_numbers,
        seconds,
    })
}

pub fn run(
    conn: &mut SqliteConnection,
    args: &Args,
    progress: &mut dyn FnMut(usize),
) -> Result<Report> {
    if args.fetch {
        match fetch_upstream(&args.upstream, &args.tsv, &args.images) {
            Ok(()) => {}
            Err(err) => eprintln!("fetch: {err}; keeping the local copies"),
        }
    }

    seed_chapters(conn)?;

    let chapter_ids: HashMap<i32, i32> = chapters::table
        .load::<Chapter>(conn)?
        .into_iter()
        .map(|c| (c.number, c.id))
        .collect();

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .flexible(false)
        .from_path(&args.tsv)
        .with_context(|| format!("cannot read {}", args.tsv.display()))?;

    let headers = reader.headers().context("missing TSV header")?.clone();
    // 15 standard columns, or 16 with an optional trailing `seconds` column.
    let has_seconds = headers.len() == 16 && headers.get(15) == Some("seconds");
    if headers.len() != HEADER.len() && !has_seconds {
        bail!(
            "{} has {} columns, expected 15 or 16 (last column: seconds)",
            args.tsv.display(),
            headers.len()
        );
    }
    let seconds_idx = has_seconds.then_some(15);

    let report = conn.transaction::<Report, anyhow::Error, _>(|conn| {
        diesel::update(questions::table)
            .set(questions::active.eq(false))
            .execute(conn)?;

        let mut report = Report::default();
        let mut seen: HashSet<String> = HashSet::new();
        let mut stored_figures: HashSet<String> = HashSet::new();

        for (index, record) in reader.records().enumerate() {
            let record = match record {
                Ok(r) => r,
                Err(err) => {
                    report.skip(index, 0, "", format!("bad_record: {err}"));
                    continue;
                }
            };
            report.read += 1;
            if report.read % 50 == 0 {
                progress(report.read);
            }
            let line = record.position().map(|p| p.line()).unwrap_or(0);
            let qid = field(&record, 0).trim().to_string();

            if !qid.is_empty() && !seen.insert(qid.clone()) {
                // Never delete the row imported from the first occurrence.
                report.skip(index, line, &qid, "duplicate_qid");
                continue;
            }

            let prepared = match parse_row(&record, &args.images, &chapter_ids, seconds_idx) {
                Ok(p) => p,
                Err(reason) => {
                    report.skip(index, line, &qid, reason);
                    continue;
                }
            };

            let new = NewQuestion {
                qid: &prepared.qid,
                year: prepared.year,
                subject: prepared.subject,
                text: &prepared.text,
                used_fixed: prepared.used_fixed,
                choice_a: &prepared.choices[0],
                choice_b: &prepared.choices[1],
                choice_c: &prepared.choices[2],
                choice_d: &prepared.choices[3],
                answer_idx: prepared.answer_idx,
                mixed_choices: prepared.mixed_choices,
                attachment_link: prepared.attachment_link.as_deref(),
                active: true,
                seconds: prepared.seconds,
            };

            diesel::insert_into(questions::table)
                .values(&new)
                .on_conflict(questions::qid)
                .do_update()
                .set(&new)
                .execute(conn)?;

            diesel::delete(
                question_chapters::table.filter(question_chapters::question_qid.eq(&prepared.qid)),
            )
            .execute(conn)?;

            for number in &prepared.chapters {
                let chapter_id = chapter_ids[number];
                diesel::insert_into(question_chapters::table)
                    .values(NewQuestionChapter {
                        question_qid: prepared.qid.clone(),
                        chapter_id,
                    })
                    .on_conflict((
                        question_chapters::question_qid,
                        question_chapters::chapter_id,
                    ))
                    .do_nothing()
                    .execute(conn)?;
                *report.per_chapter.entry(*number).or_default() += 1;
            }

            // Store each distinct figure once, straight into SQLite.
            if let Some(id) = &prepared.attachment_link {
                if stored_figures.insert(id.clone()) {
                    let path = args.images.join(format!("{id}.jpeg"));
                    let bytes = std::fs::read(&path)
                        .with_context(|| format!("cannot read {}", path.display()))?;
                    diesel::insert_into(figures::table)
                        .values(NewFigure {
                            id,
                            mime: "image/jpeg",
                            data: &bytes,
                        })
                        .on_conflict(figures::id)
                        .do_update()
                        .set((
                            figures::mime.eq("image/jpeg"),
                            figures::data.eq(bytes.as_slice()),
                        ))
                        .execute(conn)?;
                    report.figures += 1;
                }
            }

            report.imported += 1;
        }

        // Mirror every vendored figure into SQLite, including the few that only belong to
        // questions skipped as unclassified, so the database owns the complete image set.
        if args.images.is_dir() {
            for entry in std::fs::read_dir(&args.images)? {
                let path = entry?.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("jpeg") {
                    continue;
                }
                let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                    continue;
                };
                if !stored_figures.insert(id.to_string()) {
                    continue;
                }
                let bytes = std::fs::read(&path)?;
                diesel::insert_into(figures::table)
                    .values(NewFigure {
                        id,
                        mime: "image/jpeg",
                        data: &bytes,
                    })
                    .on_conflict(figures::id)
                    .do_update()
                    .set((
                        figures::mime.eq("image/jpeg"),
                        figures::data.eq(bytes.as_slice()),
                    ))
                    .execute(conn)?;
                report.figures += 1;
            }
        }

        progress(report.read);
        Ok(report)
    })?;

    let text = report.to_text();
    std::fs::write(&args.report, &text)
        .with_context(|| format!("cannot write {}", args.report.display()))?;
    print!("{text}");
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn imports_optional_seconds_column() {
        let dir = tempfile::tempdir().unwrap();
        let images = dir.path().join("figures");
        fs::create_dir_all(&images).unwrap();
        let tsv = dir.path().join("seconds.tsv");
        let mut writer = csv::WriterBuilder::new()
            .delimiter(b'\t')
            .from_path(&tsv)
            .unwrap();
        let mut header: Vec<&str> = HEADER.to_vec();
        header.push("seconds");
        writer.write_record(&header).unwrap();
        writer
            .write_record([
                "S1",
                "2024",
                "2",
                "0",
                "0",
                "Question ?",
                "",
                "a",
                "b",
                "c",
                "d",
                "0",
                "0",
                "",
                "FALSE",
                "25",
            ])
            .unwrap();
        writer.flush().unwrap();

        let args = Args {
            tsv,
            db: dir.path().join("seconds.sqlite3"),
            images,
            report: dir.path().join("report.txt"),
            fetch: false,
            upstream: DEFAULT_UPSTREAM.to_string(),
        };
        let report = run_test(&args).unwrap();
        assert_eq!(report.imported, 1);

        let mut conn = setup_connection(&args.db).unwrap();
        let question = questions::table
            .first::<bia_core::models::Question>(&mut conn)
            .unwrap();
        assert_eq!(question.seconds, Some(25));
    }

    fn run_test(args: &Args) -> anyhow::Result<Report> {
        let mut conn = setup_connection(&args.db)?;
        run(&mut conn, args, &mut |_| {})
    }

    fn fixture(dir: &Path) -> (PathBuf, PathBuf) {
        let images = dir.join("figures");
        fs::create_dir_all(&images).unwrap();
        fs::write(images.join("2015-000.jpeg"), b"jpeg").unwrap();
        fs::write(images.join("2020-001.jpeg"), b"jpeg").unwrap();

        let tsv = dir.join("annales.tsv");
        let mut w = csv::WriterBuilder::new()
            .delimiter(b'\t')
            .from_path(&tsv)
            .unwrap();
        w.write_record(HEADER).unwrap();

        // 1. valid, chapter 0, shuffled
        w.write_record([
            "A1",
            "2024",
            "2",
            "0",
            "0",
            "Un aéronef est :",
            "",
            "a",
            "b",
            "c",
            "d",
            "1",
            "0",
            "",
            "TRUE",
        ])
        .unwrap();

        // 2. multi-chapter, content_fixed wins, figure present
        w.write_record([
            "A2",
            "2024",
            "2",
            "1",
            "1",
            "Énoncé original",
            "Énoncé corrigé",
            "a",
            "b",
            "c",
            "d",
            "0",
            "3,4",
            "2015-000",
            "FALSE",
        ])
        .unwrap();

        // 3. no chapter -> imported with no question_chapters row
        w.write_record([
            "A3",
            "2024",
            "2",
            "2",
            "2",
            "Question sans chapitre",
            "",
            "a",
            "b",
            "c",
            "d",
            "0",
            "",
            "",
            "TRUE",
        ])
        .unwrap();

        // 4. missing figure -> skipped
        w.write_record([
            "A4",
            "2024",
            "2",
            "3",
            "3",
            "Avec figure absente",
            "",
            "a",
            "b",
            "c",
            "d",
            "0",
            "5",
            "2099-999",
            "TRUE",
        ])
        .unwrap();

        // 5. bad answer -> skipped
        w.write_record([
            "A5",
            "2024",
            "2",
            "4",
            "4",
            "Réponse invalide",
            "",
            "a",
            "b",
            "c",
            "d",
            "9",
            "5",
            "",
            "TRUE",
        ])
        .unwrap();

        // 6. duplicate qid -> skipped
        w.write_record([
            "A1", "2024", "2", "5", "5", "Doublon", "", "a", "b", "c", "d", "0", "5", "", "TRUE",
        ])
        .unwrap();

        // 7. embedded newline in a quoted field
        w.write_record([
            "A6",
            "2024",
            "2",
            "6",
            "6",
            "Ligne 1\nLigne 2",
            "",
            "a",
            "b",
            "c",
            "d",
            "3",
            "14",
            "2020-001",
            "TRUE",
        ])
        .unwrap();
        w.flush().unwrap();
        (tsv, images)
    }

    #[test]
    fn imports_valid_rows_and_skips_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let (tsv, images) = fixture(dir.path());
        let args = Args {
            tsv,
            db: dir.path().join("test.sqlite3"),
            images,
            report: dir.path().join("report.txt"),
            fetch: false,
            upstream: DEFAULT_UPSTREAM.to_string(),
        };
        let report = run_test(&args).unwrap();

        assert_eq!(report.read, 7);
        assert_eq!(report.imported, 4, "A1, A2, A3, A6");
        assert!(!report.reasons.contains_key("no_chapter"));
        assert_eq!(report.reasons.get("missing_figure"), Some(&1));
        assert_eq!(report.reasons.get("bad_answer"), Some(&1));
        assert_eq!(report.reasons.get("duplicate_qid"), Some(&1));
        assert_eq!(report.per_chapter.get(&0), Some(&1));
        assert_eq!(report.per_chapter.get(&3), Some(&1));
        assert_eq!(report.per_chapter.get(&4), Some(&1));

        let mut conn = setup_connection(&args.db).unwrap();
        let loaded = questions::table
            .load::<bia_core::models::Question>(&mut conn)
            .unwrap();
        assert_eq!(loaded.len(), 4);
        let a2 = loaded.iter().find(|q| q.qid == "A2").unwrap();
        assert_eq!(a2.text, "Énoncé corrigé");
        assert!(a2.used_fixed);
        assert!(!a2.mixed_choices);
        assert_eq!(a2.attachment_link.as_deref(), Some("2015-000"));
        let a6 = loaded.iter().find(|q| q.qid == "A6").unwrap();
        assert_eq!(a6.text, "Ligne 1\nLigne 2");
        let a3_links: i64 = question_chapters::table
            .filter(question_chapters::question_qid.eq("A3"))
            .count()
            .get_result(&mut conn)
            .unwrap();
        assert_eq!(a3_links, 0, "A3 has no chapter membership");
    }

    #[test]
    fn extracts_the_upstream_layout() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Cursor;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        {
            let mut builder = tar::Builder::new(&mut encoder);

            let tsv = b"qid\tyear\nA1\t2024\n";
            let mut header = tar::Header::new_gnu();
            header.set_size(tsv.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(
                    &mut header,
                    "bia-annales-database-csv-master/site/static/annales-bia.tsv",
                    &tsv[..],
                )
                .unwrap();

            let jpeg = b"\xff\xd8\xff";
            let mut header = tar::Header::new_gnu();
            header.set_size(jpeg.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(
                    &mut header,
                    "bia-annales-database-csv-master/site/static/img-sujets/2015-000.jpeg",
                    &jpeg[..],
                )
                .unwrap();
            builder.finish().unwrap();
        }
        let archive = encoder.finish().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let tsv_path = dir.path().join("annales-bia.tsv");
        let images = dir.path().join("figures");
        extract_archive(Cursor::new(archive), &tsv_path, &images).unwrap();

        assert!(tsv_path.is_file());
        assert!(images.join("2015-000.jpeg").is_file());
    }

    #[test]
    fn reimport_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let (tsv, images) = fixture(dir.path());
        let args = Args {
            tsv,
            db: dir.path().join("test.sqlite3"),
            images,
            report: dir.path().join("report.txt"),
            fetch: false,
            upstream: DEFAULT_UPSTREAM.to_string(),
        };
        run_test(&args).unwrap();
        let second = run_test(&args).unwrap();
        assert_eq!(second.read, 7);
        assert_eq!(second.imported, 4);
        let mut conn = setup_connection(&args.db).unwrap();
        assert_eq!(
            questions::table
                .count()
                .get_result::<i64>(&mut conn)
                .unwrap(),
            4
        );
        assert_eq!(
            question_chapters::table
                .count()
                .get_result::<i64>(&mut conn)
                .unwrap(),
            4,
            "A1/ch0, A2/ch3, A2/ch4, A6/ch14"
        );
        assert_eq!(
            figures::table.count().get_result::<i64>(&mut conn).unwrap(),
            2
        );
    }
}
