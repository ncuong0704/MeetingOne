// Sentence-level segmentation after ROVER word-stitch (mirrors test ASR punctuation + align).

use crate::audio::chunk_word_stitch::TimedWord;
use crate::capu_engine::CapuEngine;
use crate::config::CAPU_BATCH_WORD_BUDGET;
use crate::rover_engine::normalize::normalize_word;
use log::warn;

const MAX_SEGMENT_DURATION_SEC: f64 = 12.0;
const PAUSE_GAP_SEC: f64 = 0.8;
const MAX_WORDS_PER_PAUSE_SEGMENT: usize = 15;

/// Split punctuated text into sentences (test ASR: `re.split(r'(?<=[.?!])\s+', full_text)`).
pub fn split_sentences(text: &str) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }

    let bytes = text.as_bytes();
    let mut sentences = Vec::new();
    let mut start_byte = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'.' || b == b'!' || b == b'?' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j > i + 1 || j >= bytes.len() {
                let slice = text[start_byte..=i].trim();
                if !slice.is_empty() {
                    sentences.push(slice.to_string());
                }
                start_byte = j;
                i = j;
                continue;
            }
        }
        i += 1;
    }

    if start_byte < text.len() {
        let tail = text[start_byte..].trim();
        if !tail.is_empty() {
            sentences.push(tail.to_string());
        }
    }

    sentences
}

fn align_normalize(word: &str) -> String {
    normalize_word(word)
}

/// Find ASR word span matching `target_words` starting near `start_idx`.
fn find_word_sequence_match(
    asr_words: &[TimedWord],
    target_words: &[String],
    start_idx: usize,
) -> Option<(usize, usize)> {
    if target_words.is_empty() || asr_words.is_empty() {
        return None;
    }

    let first_target = align_normalize(&target_words[0]);
    if first_target.is_empty() {
        return None;
    }

    let end_search = (start_idx + 50).min(asr_words.len());
    let mut best_score = 0.0f32;
    let mut best_match: Option<(usize, usize)> = None;

    for i in start_idx..end_search {
        let asr_word = align_normalize(&asr_words[i].text);
        let words_match = asr_word == first_target
            || (asr_word.len() > 2
                && first_target.len() > 2
                && (asr_word.contains(&first_target) || first_target.contains(&asr_word)));

        if !words_match {
            continue;
        }

        let mut matched_count = 1usize;
        let mut last_matched_idx = i;
        let mut asr_offset = 0i32;

        for j in 1..target_words.len() {
            let target_word = align_normalize(&target_words[j]);
            if target_word.is_empty() {
                matched_count += 1;
                continue;
            }

            let asr_idx = i + j + asr_offset as usize;
            if asr_idx >= asr_words.len() {
                break;
            }

            let asr_target_word = align_normalize(&asr_words[asr_idx].text);
            let direct_match = asr_target_word == target_word
                || (asr_target_word.len() > 2
                    && target_word.len() > 2
                    && (asr_target_word.contains(&target_word)
                        || target_word.contains(&asr_target_word)));

            if direct_match {
                matched_count += 1;
                last_matched_idx = asr_idx;
            } else if asr_idx + 1 < asr_words.len() {
                let asr_next = align_normalize(&asr_words[asr_idx + 1].text);
                let next_match = asr_next == target_word
                    || (asr_next.len() > 2
                        && target_word.len() > 2
                        && (asr_next.contains(&target_word) || target_word.contains(&asr_next)));
                if next_match {
                    matched_count += 1;
                    last_matched_idx = asr_idx + 1;
                    asr_offset += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        let score = matched_count as f32 / target_words.len() as f32;
        if score > best_score {
            best_score = score;
            best_match = Some((i, last_matched_idx));
            if score >= 0.95 {
                break;
            }
        }
    }

    if best_score >= 0.7 {
        best_match
    } else {
        None
    }
}

/// Map punctuated sentences back to word timestamps (test ASR alignment phase).
pub fn align_sentences_to_words(
    sentences: &[String],
    words: &[TimedWord],
) -> Vec<(String, f64, f64)> {
    if sentences.is_empty() || words.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(sentences.len());
    let mut current_word_idx = 0usize;

    for sent in sentences {
        let sent_words: Vec<String> = sent
            .split_whitespace()
            .filter(|w| !w.trim().is_empty())
            .map(|w| w.to_string())
            .collect();
        if sent_words.is_empty() {
            continue;
        }

        let sent_words_clean: Vec<String> = sent_words
            .iter()
            .map(|w| align_normalize(w))
            .filter(|w| !w.is_empty())
            .collect();

        let (start_sec, end_sec, next_idx) =
            if let Some((match_start, match_end)) =
                find_word_sequence_match(words, &sent_words_clean, current_word_idx)
            {
                (
                    words[match_start].start_sec,
                    words[match_end].end_sec,
                    match_end + 1,
                )
            } else {
                align_sentence_fallback(words, &sent_words_clean, current_word_idx)
            };

        current_word_idx = next_idx.min(words.len());
        out.push((sent.clone(), start_sec * 1000.0, end_sec * 1000.0));
    }

    fix_overlapping_timestamps(&mut out);
    out
}

fn align_sentence_fallback(
    words: &[TimedWord],
    sent_words_clean: &[String],
    current_word_idx: usize,
) -> (f64, f64, usize) {
    let first_word = sent_words_clean.first().map(|w| align_normalize(w)).unwrap_or_default();

    let mut temp_idx = current_word_idx;
    let mut found_first = false;
    while temp_idx < words.len() {
        let asr_word = align_normalize(&words[temp_idx].text);
        if !first_word.is_empty()
            && (first_word.contains(&asr_word) || asr_word.contains(&first_word))
        {
            found_first = true;
            break;
        }
        temp_idx += 1;
    }

    if found_first {
        let end_idx = (temp_idx + sent_words_clean.len().saturating_sub(1)).min(words.len() - 1);
        return (
            words[temp_idx].start_sec,
            words[end_idx].end_sec,
            end_idx + 1,
        );
    }

    let fallback_idx = current_word_idx.min(words.len().saturating_sub(1));
    let end_idx = (fallback_idx + sent_words_clean.len().saturating_sub(1)).min(words.len() - 1);
    warn!(
        "[SentenceSegment] Alignment fallback at word {} for sentence starting {:?}",
        fallback_idx,
        sent_words_clean.first()
    );
    (
        words[fallback_idx].start_sec,
        words[end_idx].end_sec,
        end_idx + 1,
    )
}

fn fix_overlapping_timestamps(segments: &mut [(String, f64, f64)]) {
    for i in 0..segments.len().saturating_sub(1) {
        let next_start = segments[i + 1].1;
        if segments[i].2 > next_start {
            segments[i].2 = next_start;
        }
    }
}

/// Split segments longer than `max_duration_sec` by word count (test ASR `split_long_segments`).
pub fn split_long_segments(
    segments: Vec<(String, f64, f64)>,
    max_duration_sec: f64,
) -> Vec<(String, f64, f64)> {
    let mut result = Vec::new();

    for (text, start_ms, end_ms) in segments {
        let duration_sec = (end_ms - start_ms) / 1000.0;
        let text = text.trim();
        if duration_sec <= max_duration_sec || text.is_empty() {
            result.push((text.to_string(), start_ms, end_ms));
            continue;
        }

        let words: Vec<&str> = text.split_whitespace().collect();
        let total_words = words.len();
        if total_words == 0 {
            continue;
        }

        let mut num_parts = (duration_sec / max_duration_sec).floor() as usize + 1;
        if duration_sec % max_duration_sec == 0.0 {
            num_parts = (duration_sec / max_duration_sec) as usize;
        }
        num_parts = num_parts.max(2);

        if total_words < num_parts {
            result.push((text.to_string(), start_ms, end_ms));
            continue;
        }

        let words_per_part = total_words / num_parts;
        let remainder = total_words % num_parts;
        let time_per_word = duration_sec / total_words as f64;

        let mut word_idx = 0usize;
        let start_sec = start_ms / 1000.0;

        for part_idx in 0..num_parts {
            let current_part_words = words_per_part + if part_idx < remainder { 1 } else { 0 };
            if current_part_words == 0 {
                continue;
            }

            let part_text = words[word_idx..word_idx + current_part_words].join(" ");
            let part_start = start_sec + word_idx as f64 * time_per_word;
            let part_end = start_sec + (word_idx + current_part_words) as f64 * time_per_word;
            result.push((part_text, part_start * 1000.0, part_end * 1000.0));
            word_idx += current_part_words;
        }
    }

    result
}

/// Pause-based segmentation when punctuation is unavailable (test ASR no-punctuation path).
pub fn pause_based_segments(words: &[TimedWord]) -> Vec<(String, f64, f64)> {
    if words.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut current_texts: Vec<String> = Vec::new();
    let mut current_start = words[0].start_sec;

    for (i, w) in words.iter().enumerate() {
        current_texts.push(w.text.clone());

        let is_pause = if i + 1 < words.len() {
            words[i + 1].start_sec - w.end_sec > PAUSE_GAP_SEC
        } else {
            false
        };

        if is_pause || current_texts.len() > MAX_WORDS_PER_PAUSE_SEGMENT {
            out.push((
                current_texts.join(" "),
                current_start * 1000.0,
                w.end_sec * 1000.0,
            ));
            current_texts.clear();
            if i + 1 < words.len() {
                current_start = words[i + 1].start_sec;
            }
        }
    }

    if !current_texts.is_empty() {
        out.push((
            current_texts.join(" "),
            current_start * 1000.0,
            words.last().unwrap().end_sec * 1000.0,
        ));
    }

    out
}

/// Gap (seconds) after each word — used as CAPU pause hints (test ASR alignment).
pub fn compute_pause_hints(words: &[TimedWord]) -> Vec<f32> {
    if words.is_empty() {
        return Vec::new();
    }
    let mut hints = Vec::with_capacity(words.len());
    for i in 0..words.len() {
        if i + 1 < words.len() {
            hints.push((words[i + 1].start_sec - words[i].end_sec).max(0.0) as f32);
        } else {
            hints.push(1.0);
        }
    }
    hints
}

/// Run CAPU over the full transcript in word-budget chunks, preserving trailing context.
pub fn restore_full_text_capu(
    engine: &mut CapuEngine,
    raw_text: &str,
    pause_hints: Option<&[f32]>,
) -> String {
    if engine.punctuation_level() <= 1 {
        return raw_text.to_string();
    }

    let words: Vec<String> = raw_text.split_whitespace().map(|w| w.to_string()).collect();
    if words.is_empty() {
        return String::new();
    }

    let budget = CAPU_BATCH_WORD_BUDGET;
    let mut trailing_context: Vec<String> = Vec::new();
    let mut restored_parts: Vec<String> = Vec::new();
    let mut chunk_start = 0usize;

    while chunk_start < words.len() {
        let chunk_end = (chunk_start + budget).min(words.len());
        let chunk_text = words[chunk_start..chunk_end].join(" ");

        let chunk_hints = pause_hints.map(|hints| &hints[chunk_start..chunk_end]);

        match engine.restore_punctuation_with_hints(&trailing_context, &chunk_text, chunk_hints) {
            Ok((restored, next_context)) => {
                if !restored.trim().is_empty() {
                    restored_parts.push(restored);
                }
                trailing_context = next_context;
            }
            Err(e) => {
                warn!("[SentenceSegment] CAPU chunk failed, using raw text: {}", e);
                restored_parts.push(chunk_text);
                trailing_context.clear();
            }
        }

        chunk_start = chunk_end;
    }

    restored_parts.join(" ")
}

/// Full file-import finalize: CAPU on full text → sentence split → align → split long segments.
pub fn finalize_rover_word_timeline(
    words: &[TimedWord],
    engine: Option<&mut CapuEngine>,
) -> Vec<(String, f64, f64)> {
    if words.is_empty() {
        return Vec::new();
    }

    let raw_text: String = words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let raw_text = crate::audio::post_asr::normalize_asr_text(&raw_text);
    let pause_hints = compute_pause_hints(words);

    let punctuated = match engine {
        Some(engine) => restore_full_text_capu(engine, &raw_text, Some(&pause_hints)),
        None => raw_text.clone(),
    };

    let has_punctuation = punctuated
        .chars()
        .any(|c| matches!(c, '.' | ',' | '!' | '?' | ';' | ':'));

    let mut segments = if has_punctuation {
        let sentences = split_sentences(&punctuated);
        if sentences.is_empty() {
            pause_based_segments(words)
        } else {
            align_sentences_to_words(&sentences, words)
        }
    } else {
        pause_based_segments(words)
    };

    segments = split_long_segments(segments, MAX_SEGMENT_DURATION_SEC);
    segments
}

/// Expand a live utterance (one streaming-ASR chunk) into evenly spaced `TimedWord`s
/// so `align_sentences_to_words` can reuse the file-import path without word-level ASR.
pub fn utterances_to_timed_words(utterances: &[(&str, f64, f64)]) -> Vec<TimedWord> {
    let mut out = Vec::new();
    for (text, start, end) in utterances {
        let words: Vec<&str> = text
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .collect();
        if words.is_empty() {
            continue;
        }
        let span = (*end - *start).max(0.0);
        let n = words.len() as f64;
        let step = span / n;
        for (i, w) in words.iter().enumerate() {
            let s = *start + i as f64 * step;
            let e = if i + 1 == words.len() {
                *end
            } else {
                *start + (i + 1) as f64 * step
            };
            out.push(TimedWord {
                text: (*w).to_string(),
                start_sec: s,
                end_sec: e.max(s),
                confidence: 0.9,
            });
        }
    }
    out
}

/// Map punctuated CAPU text onto live utterance clocks. Returns **seconds**
/// (`align_sentences_to_words` uses milliseconds internally, matching import).
pub fn split_punctuated_onto_utterances(
    punctuated: &str,
    utterances: &[(&str, f64, f64)],
) -> Vec<(String, f64, f64)> {
    if utterances.is_empty() {
        return Vec::new();
    }
    let batch_start = utterances[0].1;
    let batch_end = utterances[utterances.len() - 1].2;
    let text = punctuated.trim();
    if text.is_empty() {
        return Vec::new();
    }

    let has_sentence_end = text.chars().any(|c| matches!(c, '.' | '!' | '?'));
    if !has_sentence_end {
        return vec![(text.to_string(), batch_start, batch_end)];
    }

    let sentences = split_sentences(text);
    if sentences.len() <= 1 {
        let only = sentences
            .into_iter()
            .next()
            .unwrap_or_else(|| text.to_string());
        return vec![(only, batch_start, batch_end)];
    }

    let words = utterances_to_timed_words(utterances);
    if words.is_empty() {
        return vec![(text.to_string(), batch_start, batch_end)];
    }

    align_sentences_to_words(&sentences, &words)
        .into_iter()
        .map(|(t, start_ms, end_ms)| (t, start_ms / 1000.0, end_ms / 1000.0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::chunk_word_stitch::TimedWord;

    fn tw(text: &str, start: f64) -> TimedWord {
        TimedWord {
            text: text.to_string(),
            start_sec: start,
            end_sec: start + 0.3,
            confidence: 0.9,
        }
    }

    #[test]
    fn compute_pause_hints_uses_gaps_between_words() {
        let mut words = vec![tw("a", 0.0), tw("b", 1.5), tw("c", 2.0)];
        words[0].end_sec = 0.3;
        words[1].end_sec = 1.8;
        let hints = compute_pause_hints(&words);
        assert_eq!(hints.len(), 3);
        assert!(hints[0] > 1.0);
        assert_eq!(hints[2], 1.0);
    }

    #[test]
    fn split_sentences_on_period_question_exclamation() {
        let s = "Xin chào. Hôm nay thế nào? Tốt lắm!";
        let parts = split_sentences(s);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "Xin chào.");
    }

    #[test]
    fn align_sentences_maps_timestamps_in_order() {
        let words = vec![
            tw("xin", 0.0),
            tw("chào", 0.5),
            tw("các", 1.0),
            tw("bạn", 1.5),
            tw("hôm", 2.0),
            tw("nay", 2.5),
        ];
        let sentences = vec![
            "Xin chào các bạn.".to_string(),
            "Hôm nay.".to_string(),
        ];
        let aligned = align_sentences_to_words(&sentences, &words);
        assert_eq!(aligned.len(), 2);
        assert!(aligned[0].1 < aligned[1].1);
        assert!(aligned[0].0.to_lowercase().contains("xin"));
    }

    #[test]
    fn split_long_segments_splits_by_duration() {
        let segments = vec![("a b c d e f".to_string(), 0.0, 30_000.0)];
        let split = split_long_segments(segments, 12.0);
        assert!(split.len() >= 2);
    }

    #[test]
    fn pause_based_segments_breaks_on_long_gap() {
        let mut words = vec![
            tw("một", 0.0),
            tw("hai", 0.4),
            tw("ba", 2.0),
            tw("bốn", 2.4),
        ];
        words[1].end_sec = 0.7;
        let segs = pause_based_segments(&words);
        assert!(segs.len() >= 2);
    }

    #[test]
    fn finalize_without_capu_uses_pause_segmentation() {
        let words: Vec<TimedWord> = (0..20)
            .map(|i| tw(&format!("từ{}", i), i as f64 * 0.4))
            .collect();
        let segs = finalize_rover_word_timeline(&words, None);
        assert!(!segs.is_empty());
        assert!(segs.len() > 1);
    }

    #[test]
    fn utterances_to_timed_words_interpolates_evenly_in_span() {
        let words = utterances_to_timed_words(&[("xin chao", 0.0, 2.0)]);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "xin");
        assert_eq!(words[1].text, "chao");
        assert!((words[0].start_sec - 0.0).abs() < 1e-9);
        assert!((words[0].end_sec - 1.0).abs() < 1e-9);
        assert!((words[1].start_sec - 1.0).abs() < 1e-9);
        assert!((words[1].end_sec - 2.0).abs() < 1e-9);
    }

    #[test]
    fn split_punctuated_onto_utterances_returns_seconds_per_sentence() {
        let utterances = [
            ("xin chao cac ban", 0.0, 2.0),
            ("hom nay", 2.0, 3.0),
        ];
        let split = split_punctuated_onto_utterances("Xin chào các bạn. Hôm nay.", &utterances);
        assert_eq!(split.len(), 2);
        assert!(split[0].0.to_lowercase().contains("xin"));
        assert!(split[1].0.to_lowercase().contains("hôm") || split[1].0.to_lowercase().contains("hom"));
        assert!(split[0].1 < split[1].1);
        assert!(split[0].2 <= split[1].1 + 1e-6);
        // Times are in seconds, not milliseconds (import align uses ms internally).
        assert!(split[0].2 < 100.0);
        assert!(split[1].2 <= 3.0 + 1e-6);
    }

    #[test]
    fn split_punctuated_onto_utterances_keeps_one_span_without_punctuation() {
        let utterances = [("xin chao", 0.0, 1.0), ("cac ban", 1.0, 2.0)];
        let split = split_punctuated_onto_utterances("xin chao cac ban", &utterances);
        assert_eq!(split.len(), 1);
        assert_eq!(split[0].0, "xin chao cac ban");
        assert!((split[0].1 - 0.0).abs() < 1e-9);
        assert!((split[0].2 - 2.0).abs() < 1e-9);
    }
}
