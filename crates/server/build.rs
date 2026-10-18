use std::process::Command;

fn main() {
    // rust-embed requires the folder to exist at compile time. `make build` builds the
    // SPA first; this keeps a bare `cargo build` from failing on a fresh checkout.
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../frontend/dist");
    if !dist.exists() {
        std::fs::create_dir_all(&dist).expect("create frontend/dist");
    }
    println!("cargo:rerun-if-changed=../../frontend/dist");

    // The deployed commit, shown in the footer (AGPL source offer).
    // CI/tarball builds can pin it with BIA_QUIZ_COMMIT; otherwise read git.
    let commit = std::env::var("BIA_QUIZ_COMMIT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(git_commit)
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=BIA_QUIZ_COMMIT={commit}");
    println!("cargo:rerun-if-env-changed=BIA_QUIZ_COMMIT");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}

fn git_commit() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if commit.is_empty() {
        None
    } else {
        Some(commit)
    }
}
