use crate::rnnt_decoder::engine::WordResult;
use crate::rover_engine::normalize::normalize_word;
use similar::{capture_diff_slices, Algorithm, DiffTag};

/// A `B`-only word is only accepted into the merge if its own confidence clears
/// this bar — matches the reference app's `_word_confidence(wb) > 0.20` check.
const INSERT_CONFIDENCE_THRESHOLD: f32 = 0.20;
/// Two words within this many seconds of each other, with the same normalized
/// text, are treated as the same word for dedup purposes.
const DEDUP_TIME_WINDOW_SECONDS: f32 = 0.15;

pub struct MergedWord {
    pub word: WordResult,
    /// True if this word came from B overriding A in a Replace block, or from a
    /// B-only Insert. False for anything both models agreed on, or anything kept
    /// from A by default (Equal, Delete, or a Replace A won).
    pub disagree: bool,
}

struct Candidate {
    word: WordResult,
    disagree: bool,
    is_supplement: bool,
}

fn block_confidence(words: &[WordResult]) -> f32 {
    if words.is_empty() {
        return 0.0;
    }
    let sum: f32 = words.iter().map(|w| w.confidence).sum();
    sum / words.len() as f32
}

/// Aligns two independently-decoded word sequences and merges them by confidence.
/// Mirrors `rover_merge_words` in the reference app, minus the hotword bonus (this
/// project has no hotword feature).
pub fn rover_merge_words(words_a: &[WordResult], words_b: &[WordResult]) -> Vec<MergedWord> {
    let norm_a: Vec<String> = words_a.iter().map(|w| normalize_word(&w.text)).collect();
    let norm_b: Vec<String> = words_b.iter().map(|w| normalize_word(&w.text)).collect();

    let ops = capture_diff_slices(Algorithm::Myers, &norm_a, &norm_b);

    let mut candidates: Vec<Candidate> = Vec::new();

    for op in &ops {
        let (tag, old_range, new_range) = op.as_tag_tuple();
        match tag {
            DiffTag::Equal | DiffTag::Delete => {
                for w in &words_a[old_range] {
                    candidates.push(Candidate {
                        word: w.clone(),
                        disagree: false,
                        is_supplement: false,
                    });
                }
            }
            DiffTag::Replace => {
                let block_a = &words_a[old_range];
                let block_b = &words_b[new_range];
                let conf_a = block_confidence(block_a);
                let conf_b = block_confidence(block_b);
                let (chosen, disagree) = if conf_b > conf_a {
                    (block_b, true)
                } else {
                    (block_a, false)
                };
                for w in chosen {
                    candidates.push(Candidate {
                        word: w.clone(),
                        disagree,
                        is_supplement: false,
                    });
                }
            }
            DiffTag::Insert => {
                for w in &words_b[new_range] {
                    if w.confidence > INSERT_CONFIDENCE_THRESHOLD {
                        candidates.push(Candidate {
                            word: w.clone(),
                            disagree: true,
                            is_supplement: true,
                        });
                    }
                }
            }
        }
    }

    candidates.sort_by(|a, b| a.word.start.partial_cmp(&b.word.start).unwrap());

    let mut result: Vec<MergedWord> = Vec::with_capacity(candidates.len());
    for cand in candidates {
        if cand.is_supplement {
            let is_duplicate = result.iter().any(|kept: &MergedWord| {
                (kept.word.start - cand.word.start).abs() < DEDUP_TIME_WINDOW_SECONDS
                    && normalize_word(&kept.word.text) == normalize_word(&cand.word.text)
            });
            if is_duplicate {
                continue;
            }
        }
        result.push(MergedWord {
            word: cand.word,
            disagree: cand.disagree,
        });
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, start: f32, confidence: f32) -> WordResult {
        WordResult {
            text: text.to_string(),
            start,
            end: start,
            margin_min: confidence,
            tsallis_max: 0.0,
            confidence,
        }
    }

    #[test]
    fn full_agreement_keeps_a_with_no_disagreement() {
        let a = vec![word("một", 0.0, 0.9), word("hai", 0.5, 0.9), word("ba", 1.0, 0.9)];
        let b = vec![word("một", 0.0, 0.9), word("hai", 0.5, 0.9), word("ba", 1.0, 0.9)];

        let merged = rover_merge_words(&a, &b);

        assert_eq!(merged.len(), 3);
        let texts: Vec<&str> = merged.iter().map(|m| m.word.text.as_str()).collect();
        assert_eq!(texts, vec!["một", "hai", "ba"]);
        assert!(merged.iter().all(|m| !m.disagree));
    }

    #[test]
    fn replace_keeps_a_when_a_more_confident() {
        let a = vec![word("một", 0.0, 0.95)];
        let b = vec![word("mốt", 0.0, 0.40)];

        let merged = rover_merge_words(&a, &b);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].word.text, "một");
        assert!(!merged[0].disagree);
    }

    #[test]
    fn replace_picks_b_when_b_more_confident_and_marks_disagreement() {
        let a = vec![word("một", 0.0, 0.30)];
        let b = vec![word("mốt", 0.0, 0.92)];

        let merged = rover_merge_words(&a, &b);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].word.text, "mốt");
        assert!(merged[0].disagree);
    }

    #[test]
    fn insert_above_threshold_is_included() {
        let a = vec![word("một", 0.0, 0.9), word("ba", 1.0, 0.9)];
        let b = vec![word("một", 0.0, 0.9), word("hai", 0.5, 0.50), word("ba", 1.0, 0.9)];

        let merged = rover_merge_words(&a, &b);

        let texts: Vec<&str> = merged.iter().map(|m| m.word.text.as_str()).collect();
        assert_eq!(texts, vec!["một", "hai", "ba"]);
        assert!(merged.iter().find(|m| m.word.text == "hai").unwrap().disagree);
    }

    #[test]
    fn insert_below_threshold_is_dropped() {
        let a = vec![word("một", 0.0, 0.9), word("ba", 1.0, 0.9)];
        let b = vec![word("một", 0.0, 0.9), word("hai", 0.5, 0.05), word("ba", 1.0, 0.9)];

        let merged = rover_merge_words(&a, &b);

        let texts: Vec<&str> = merged.iter().map(|m| m.word.text.as_str()).collect();
        assert_eq!(texts, vec!["một", "ba"]);
    }

    #[test]
    fn near_duplicate_insert_supplement_is_deduped() {
        // "hai" already present via A/Equal at t=0.50; B supplies the same
        // normalized word 0.05s away via a spurious Insert — must not double up.
        let a = vec![word("một", 0.0, 0.9), word("hai", 0.50, 0.9), word("ba", 1.0, 0.9)];
        let b = vec![
            word("một", 0.0, 0.9),
            word("hai", 0.50, 0.9),
            word("hai", 0.55, 0.60), // spurious near-duplicate
            word("ba", 1.0, 0.9),
        ];

        let merged = rover_merge_words(&a, &b);

        let hai_count = merged.iter().filter(|m| m.word.text == "hai").count();
        assert_eq!(hai_count, 1, "duplicate 'hai' supplement should have been deduped");
    }

    #[test]
    fn empty_a_takes_all_of_b_above_threshold() {
        let a: Vec<WordResult> = vec![];
        let b = vec![word("một", 0.0, 0.9), word("hai", 0.5, 0.05)];

        let merged = rover_merge_words(&a, &b);

        let texts: Vec<&str> = merged.iter().map(|m| m.word.text.as_str()).collect();
        assert_eq!(texts, vec!["một"]);
    }

    #[test]
    fn empty_b_keeps_all_of_a() {
        let a = vec![word("một", 0.0, 0.9), word("hai", 0.5, 0.9)];
        let b: Vec<WordResult> = vec![];

        let merged = rover_merge_words(&a, &b);

        assert_eq!(merged.len(), 2);
        assert!(merged.iter().all(|m| !m.disagree));
    }

    #[test]
    fn both_empty_returns_empty() {
        let merged = rover_merge_words(&[], &[]);
        assert!(merged.is_empty());
    }
}
