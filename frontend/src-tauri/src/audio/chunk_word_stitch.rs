// Word-level overlap stitching for file-import ROVER chunks (mirrors test ASR).

use crate::rover_engine::merge::MergedWord;
use crate::rover_engine::normalize::normalize_word;

const MAX_OVERLAP_WORDS: usize = 100;
const MIN_MATCH_RATIO: f32 = 0.5;

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
        let cut = find_overlap_cut_index(tail, &words[..head_take]);
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
}
