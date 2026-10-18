//! Random selection and answer shuffling (plan §4).

use std::collections::{HashMap, HashSet};

use rand::seq::SliceRandom;
use rand::Rng;

/// Pick at most `count` questions out of the candidates, without repeats.
/// Returns fewer than `count` when the chapters do not hold enough questions.
pub fn draw_question_ids<R: Rng + ?Sized>(
    mut candidates: Vec<String>,
    count: usize,
    rng: &mut R,
) -> Vec<String> {
    candidates.shuffle(rng);
    candidates.truncate(count);
    candidates
}

/// Draw up to `count` questions from each chapter, without ever drawing the same
/// question twice. A chapter that belongs to several picks (or that shares a
/// question with another chapter) contributes at most its quota; the caller gets
/// fewer questions when a pool is short.
pub fn draw_per_chapter<R: Rng + ?Sized>(
    candidates: Vec<(i32, String)>,
    picks: &[(i32, usize)],
    rng: &mut R,
) -> Vec<String> {
    let mut by_chapter: HashMap<i32, Vec<String>> = HashMap::new();
    for (chapter, qid) in candidates {
        by_chapter.entry(chapter).or_default().push(qid);
    }
    for pool in by_chapter.values_mut() {
        pool.shuffle(rng);
    }

    let mut seen = HashSet::new();
    let mut chosen = Vec::new();
    for (chapter, count) in picks {
        let Some(pool) = by_chapter.get(chapter) else {
            continue;
        };
        let mut taken = 0;
        for qid in pool {
            if taken >= *count {
                break;
            }
            if seen.insert(qid.clone()) {
                chosen.push(qid.clone());
                taken += 1;
            }
        }
    }
    chosen.shuffle(rng);
    chosen
}

/// A permutation of `[0, 1, 2, 3]`: `order[displayed_position] = original_index`.
pub fn shuffle_order<R: Rng + ?Sized>(rng: &mut R) -> [usize; 4] {
    let mut order = [0usize, 1, 2, 3];
    order.shuffle(rng);
    order
}

pub fn identity_order() -> [usize; 4] {
    [0, 1, 2, 3]
}

pub fn order_to_string(order: &[usize; 4]) -> String {
    format!("{},{},{},{}", order[0], order[1], order[2], order[3])
}

pub fn order_from_string(raw: &str) -> Option<[usize; 4]> {
    let mut out = [0usize; 4];
    let mut seen = [false; 4];
    let parts: Vec<&str> = raw.split(',').collect();
    if parts.len() != 4 {
        return None;
    }
    for (i, part) in parts.iter().enumerate() {
        let value: usize = part.trim().parse().ok()?;
        if value > 3 || seen[value] {
            return None;
        }
        seen[value] = true;
        out[i] = value;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn seq(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("q{i}")).collect()
    }

    #[test]
    fn draw_takes_exactly_count_without_repeats() {
        let mut rng = StdRng::seed_from_u64(7);
        let picked = draw_question_ids(seq(50), 10, &mut rng);
        assert_eq!(picked.len(), 10);
        let mut unique = picked.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 10);
    }

    #[test]
    fn draw_returns_what_exists_when_short() {
        let mut rng = StdRng::seed_from_u64(7);
        let picked = draw_question_ids(seq(6), 10, &mut rng);
        assert_eq!(picked.len(), 6);
    }

    #[test]
    fn per_chapter_respects_each_quota() {
        let mut rng = StdRng::seed_from_u64(11);
        let candidates = vec![
            (0, "a0".to_string()),
            (0, "a1".to_string()),
            (0, "a2".to_string()),
            (1, "b0".to_string()),
            (1, "b1".to_string()),
        ];
        let picked = draw_per_chapter(candidates, &[(0, 2), (1, 2)], &mut rng);
        assert_eq!(picked.len(), 4);
        assert_eq!(picked.iter().filter(|q| q.starts_with('a')).count(), 2);
        assert_eq!(picked.iter().filter(|q| q.starts_with('b')).count(), 2);
        let unique: HashSet<&String> = picked.iter().collect();
        assert_eq!(unique.len(), 4);
    }

    #[test]
    fn per_chapter_returns_short_when_a_pool_is_short() {
        let mut rng = StdRng::seed_from_u64(12);
        let candidates = vec![
            (0, "a0".to_string()),
            (1, "b0".to_string()),
            (1, "b1".to_string()),
        ];
        let picked = draw_per_chapter(candidates, &[(0, 3), (1, 5)], &mut rng);
        assert_eq!(picked.len(), 3);
        assert_eq!(picked.iter().filter(|q| q.starts_with('a')).count(), 1);
    }

    #[test]
    fn per_chapter_does_not_repeat_shared_questions() {
        let mut rng = StdRng::seed_from_u64(13);
        let candidates = vec![
            (0, "shared".to_string()),
            (0, "a0".to_string()),
            (1, "shared".to_string()),
            (1, "b0".to_string()),
        ];
        let picked = draw_per_chapter(candidates, &[(0, 2), (1, 2)], &mut rng);
        assert_eq!(picked.len(), 3);
        let unique: HashSet<&String> = picked.iter().collect();
        assert_eq!(unique.len(), 3);
    }

    #[test]
    fn order_is_a_permutation() {
        let mut rng = StdRng::seed_from_u64(7);
        for _ in 0..200 {
            let order = shuffle_order(&mut rng);
            let mut sorted = order;
            sorted.sort();
            assert_eq!(sorted, [0, 1, 2, 3]);
        }
    }

    #[test]
    fn order_round_trips() {
        let mut rng = StdRng::seed_from_u64(3);
        let order = shuffle_order(&mut rng);
        assert_eq!(order_from_string(&order_to_string(&order)), Some(order));
        assert_eq!(order_from_string("0,0,1,2"), None);
        assert_eq!(order_from_string("0,1,2"), None);
        assert_eq!(order_from_string("0,1,2,4"), None);
    }
}
