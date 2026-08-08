// Word-level overlap stitching for file-import ROVER chunks (mirrors test ASR).

use crate::rover_engine::merge::MergedWord;
use crate::rover_engine::normalize::normalize_word;

const MAX_OVERLAP_WORDS: usize = 100;
const MIN_MATCH_RATIO: f32 = 0.5;
/// Below this, a "backward" start is treated as normal ASR word-boundary imprecision
/// (adjacent words routinely land a few tens of ms into each other) rather than a real
/// chunk-stitch problem — roughly half a typical short word's duration. The confirmed
/// real-run bug this guards against was a 0.28s jump, well above this tolerance.
const BACKWARD_JUMP_TOLERANCE_SEC: f64 = 0.15;

#[derive(Debug, Clone)]
pub struct TimedWord {
    pub text: String,
    pub start_sec: f64,
    pub end_sec: f64,
    pub confidence: f32,
}

pub fn offset_rover_words(words: &[MergedWord], base_sec: f64) -> Vec<TimedWord> {
    words
        .iter()
        .map(|m| TimedWord {
            text: m.word.text.clone(),
            start_sec: base_sec + m.word.start as f64,
            end_sec: base_sec + m.word.end as f64,
            confidence: m.word.confidence,
        })
        .collect()
}

fn words_match(w1: &str, w2: &str) -> bool {
    let a = normalize_word(w1);
    let b = normalize_word(w2);
    if a == b {
        return true;
    }
    if a.len() > 2 && b.len() > 2 {
        return a.contains(&b) || b.contains(&a);
    }
    false
}

/// Index in `head` where non-overlapping content begins (0 = keep all).
pub fn find_overlap_cut_index(tail: &[TimedWord], head: &[TimedWord]) -> usize {
    if tail.is_empty() || head.is_empty() {
        return 0;
    }

    let tail_trunc = &tail[tail.len().saturating_sub(MAX_OVERLAP_WORDS)..];
    let head_trunc = &head[..head.len().min(MAX_OVERLAP_WORDS)];

    let tail_norm: Vec<String> = tail_trunc.iter().map(|w| normalize_word(&w.text)).collect();
    let head_norm: Vec<String> = head_trunc.iter().map(|w| normalize_word(&w.text)).collect();

    let mut best_score = 0usize;
    let mut best_cut = 0usize;

    let min_offset = -(tail_norm.len() as i32) + 1;
    let max_offset = head_norm.len() as i32;

    for offset in min_offset..max_offset {
        let mut score = 0usize;
        let mut matched_head_indices = Vec::new();

        for (i, tail_w) in tail_norm.iter().enumerate() {
            let head_idx = i as i32 + offset;
            if head_idx >= 0 && (head_idx as usize) < head_norm.len() {
                if words_match(tail_w, &head_norm[head_idx as usize]) {
                    score += 1;
                    matched_head_indices.push(head_idx as usize);
                }
            }
        }

        let overlap_window = (head_norm.len() as i32)
            .min(tail_norm.len() as i32 + offset)
            - 0.max(offset);
        let overlap_window = overlap_window.max(1) as usize;
        let match_ratio = score as f32 / overlap_window as f32;

        if score > best_score && match_ratio >= MIN_MATCH_RATIO {
            if let Some(&last_head) = matched_head_indices.last() {
                best_score = score;
                best_cut = last_head + 1;
            }
        }
    }

    best_cut.min(head.len())
}

/// Index in `words` of the first word starting at or after `min_start_sec` (or
/// `words.len()` if none). A pure timestamp cut — unlike `find_overlap_cut_index`, it
/// can't be fooled by a missing or partial text match, at the cost of occasionally
/// dropping one legitimate word whose ASR-decoded start time lands slightly early. That
/// trade favors a correct, monotonic timeline over completeness, matching test_asr's
/// timestamp-anchored fallback.
fn timestamp_cut_index(words: &[TimedWord], min_start_sec: f64) -> usize {
    words
        .iter()
        .position(|w| w.start_sec >= min_start_sec)
        .unwrap_or(words.len())
}

/// Merge per-chunk word lists, dropping duplicated overlap regions.
pub fn stitch_word_chunks(
    mut chunks: Vec<(usize, Vec<TimedWord>)>,
    leading_context_samples: &[usize],
) -> Vec<TimedWord> {
    chunks.sort_by_key(|(i, _)| *i);
    let mut merged: Vec<TimedWord> = Vec::new();

    for (idx, words) in chunks {
        if words.is_empty() {
            continue;
        }
        let has_overlap = leading_context_samples.get(idx).copied().unwrap_or(0) > 0;
        if merged.is_empty() || !has_overlap {
            merged.extend(words);
            continue;
        }

        let tail_take = merged.len().min(MAX_OVERLAP_WORDS);
        let tail = &merged[merged.len() - tail_take..];
        let head_take = words.len().min(MAX_OVERLAP_WORDS);
        let mut cut = find_overlap_cut_index(tail, &words[..head_take]);

        // Safety net: text-matching found no reliable alignment (cut stayed 0 despite
        // real overlap) or the cut it did find still leaves a word starting before the
        // previous chunk's kept content ends — a backward timestamp jump. Fall back to a
        // pure timestamp cut, correct by construction regardless of what text-matching
        // found. Mirrors test_asr's `find_overlap_alignment` divergence guard, whose
        // fallback is likewise anchored to the known overlap time window rather than to a
        // possibly-wrong text-match position.
        if let Some(prev_end) = merged.last().map(|w| w.end_sec) {
            let backward = words
                .get(cut)
                .map(|w| w.start_sec < prev_end - BACKWARD_JUMP_TOLERANCE_SEC)
                .unwrap_or(false);
            if backward {
                cut = timestamp_cut_index(&words, prev_end);
            }
        }

        merged.extend(words.into_iter().skip(cut));
    }

    merged
}

/// Group continuous words into transcript segments at natural pauses.
pub fn group_words_into_segments(words: &[TimedWord], gap_sec: f64) -> Vec<(String, f64, f64)> {
    if words.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut start_sec = words[0].start_sec;
    let mut end_sec = words[0].end_sec;
    let mut texts = vec![words[0].text.clone()];

    for w in words.iter().skip(1) {
        if w.start_sec - end_sec > gap_sec {
            out.push((texts.join(" "), start_sec * 1000.0, end_sec * 1000.0));
            texts = vec![w.text.clone()];
            start_sec = w.start_sec;
        } else {
            texts.push(w.text.clone());
        }
        end_sec = w.end_sec;
    }

    out.push((texts.join(" "), start_sec * 1000.0, end_sec * 1000.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tw(text: &str, start: f64) -> TimedWord {
        TimedWord {
            text: text.to_string(),
            start_sec: start,
            end_sec: start + 0.3,
            confidence: 0.9,
        }
    }

    #[test]
    fn find_overlap_cut_index_finds_shared_prefix() {
        let tail = vec![tw("xin", 0.0), tw("chào", 0.5), tw("các", 1.0), tw("bạn", 1.5)];
        let head = vec![
            tw("các", 1.4),
            tw("bạn", 1.7),
            tw("hôm", 2.0),
            tw("nay", 2.3),
        ];
        let cut = find_overlap_cut_index(&tail, &head);
        assert_eq!(cut, 2, "should skip 'các bạn' overlap");
    }

    #[test]
    fn stitch_word_chunks_produces_continuous_timeline() {
        let chunks = vec![
            (
                0,
                vec![tw("một", 0.0), tw("hai", 0.5), tw("ba", 1.0)],
            ),
            (
                1,
                vec![tw("ba", 0.9), tw("bốn", 1.2), tw("năm", 1.5)],
            ),
        ];
        let merged = stitch_word_chunks(chunks, &[0, 16000]);
        let texts: Vec<&str> = merged.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, vec!["một", "hai", "ba", "bốn", "năm"]);
    }

    fn assert_no_backward_jump(merged: &[TimedWord]) {
        for i in 1..merged.len() {
            assert!(
                merged[i].start_sec >= merged[i - 1].end_sec - 1e-6,
                "backward jump: {:?} (end={}) then {:?} (start={})",
                merged[i - 1].text,
                merged[i - 1].end_sec,
                merged[i].text,
                merged[i].start_sec
            );
        }
    }

    // Reproduces the confirmed real-run bug (timing_debug.log chunk_idx=4): the next
    // chunk's head text has nothing in common with the previous chunk's tail, so
    // `find_overlap_cut_index` finds no match at all and returns 0 — keeping every head
    // word, including ones that start before the previous chunk's kept content already
    // ended. Mirrors test_asr's `find_overlap_alignment` divergence guard
    // (`best_score == 0`), which falls back to a timestamp-anchored cut instead.
    #[test]
    fn stitch_word_chunks_avoids_backward_jump_when_no_text_match_found() {
        let chunks = vec![
            (0, vec![tw("một", 0.0), tw("hai", 0.5), tw("ba", 1.0)]), // tail ends at 1.3
            (
                1,
                vec![tw("xyz", 0.8), tw("bốn", 1.2), tw("năm", 1.5)], // "xyz" matches nothing
            ),
        ];
        let merged = stitch_word_chunks(chunks, &[0, 16000]);
        assert_no_backward_jump(&merged);
    }

    // Reproduces the confirmed real-run bug (timing_debug.log chunk_idx=7,
    // BACKWARD_JUMP=true): text-matching finds *a* cut, but an extra unmatched word
    // sitting right after the matched span still starts before the previous chunk's kept
    // content ends. Mirrors test_asr's `is_diverged` guard (a match was found but doesn't
    // fully explain the overlap window) — must still fall back to a timestamp cut rather
    // than trusting the text-match position outright.
    #[test]
    fn stitch_word_chunks_avoids_backward_jump_when_text_match_is_incomplete() {
        let chunks = vec![
            (0, vec![tw("một", 0.0), tw("hai", 0.5), tw("ba", 1.0)]), // tail ends at 1.3
            (
                1,
                vec![
                    tw("hai", 0.6),
                    tw("ba", 0.9),
                    tw("khác", 1.1),  // unmatched, starts before prev's 1.3 end
                    tw("bốn", 1.4),
                ],
            ),
        ];
        let merged = stitch_word_chunks(chunks, &[0, 16000]);
        assert_no_backward_jump(&merged);
    }
}
