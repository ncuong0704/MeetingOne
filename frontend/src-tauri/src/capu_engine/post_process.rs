// Deterministic text cleanup applied after every CAPU model inference — ported from the
// reference app's `ImprovedPunctuationRestorer._post_process`
// (punctuation_restorer_improved.py). The CAPU model alone tends to over-punctuate short
// sentences and occasionally misses capitalizing a sentence start; this catches both
// unconditionally, regardless of what the model predicted.

use regex::Regex;
use std::sync::LazyLock;

static REPEATED_COMMA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",+").unwrap());
static REPEATED_PERIOD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.{4,}").unwrap());
static COMMA_BEFORE_PERIOD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",\s*\.").unwrap());
static SENTENCE_BOUNDARY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[.!?]\s+").unwrap());
static PUNCT_NO_SPACE_AFTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([,.!?])(\S)").unwrap());
static SPACE_BEFORE_PUNCT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+([,.!?])").unwrap());
static LEADING_COMMA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^,\s*").unwrap());
static PERIOD_THEN_COMMA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.\s*,").unwrap());
static WHITESPACE_RUN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

/// Runs the full cleanup pipeline. Safe to call on an empty/whitespace-only string.
pub fn post_process(text: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }

    // 0. Colons rarely belong in speech-to-text output and can read like a stray
    //    sentence break — drop them (reference app's own rationale).
    let mut text = text.replace(':', " ");
    // 1. Collapse repeated commas.
    text = REPEATED_COMMA.replace_all(&text, ",").into_owned();
    // 2. Cap long period runs at "...".
    text = REPEATED_PERIOD.replace_all(&text, "...").into_owned();
    // 3. No comma directly before a period.
    text = COMMA_BEFORE_PERIOD.replace_all(&text, ".").into_owned();
    // 4. At most one comma survives in a short (<8-word) sentence.
    text = limit_commas_in_short_sentences(&text);
    // 5. Ensure a space follows every punctuation mark.
    text = PUNCT_NO_SPACE_AFTER.replace_all(&text, "$1 $2").into_owned();
    // 6. Remove space before punctuation.
    text = SPACE_BEFORE_PUNCT.replace_all(&text, "$1").into_owned();
    // 7. No leading comma; no comma right after a period.
    text = LEADING_COMMA.replace(&text, "").into_owned();
    text = PERIOD_THEN_COMMA.replace_all(&text, ". ").into_owned();
    // 8. Normalize whitespace runs to a single space.
    text = WHITESPACE_RUN.replace_all(&text, " ").into_owned();
    // 9. Deterministic capitalization safety net, independent of whether the model's own
    //    $TRANSFORM_CASE prediction fired at this position.
    text = capitalize_sentence_starts(&text);

    text.trim().to_string()
}

/// Splits `text` right after each `[.!?]` run of trailing whitespace, keeping the
/// punctuation with the sentence before it and dropping the separating whitespace — a
/// lookbehind-free equivalent of the reference's `re.split(r'(?<=[.!?])\s+', text)` (the
/// `regex` crate has no lookbehind support).
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut last_end = 0;
    for m in SENTENCE_BOUNDARY.find_iter(text) {
        // The match always starts with the punctuation char itself (a single ASCII byte:
        // `.`, `!`, or `?`), so `m.start() + 1` is exactly the byte offset right after it.
        let punct_end = m.start() + 1;
        sentences.push(text[last_end..punct_end].to_string());
        last_end = m.end();
    }
    sentences.push(text[last_end..].to_string());
    sentences
}

fn limit_commas_in_short_sentences(text: &str) -> String {
    split_sentences(text)
        .into_iter()
        .map(|sent| limit_commas_in_one_sentence(&sent))
        .collect::<Vec<_>>()
        .join(" ")
}

/// In a short sentence (fewer than 8 whitespace-separated words) with more than one
/// comma, keeps only the first comma and strips every comma after it — CAPU tends to
/// over-punctuate short utterances, and dense commas there read as noise rather than
/// structure.
fn limit_commas_in_one_sentence(sent: &str) -> String {
    let comma_count = sent.matches(',').count();
    let word_count = sent.split_whitespace().count();
    if word_count >= 8 || comma_count <= 1 {
        return sent.to_string();
    }
    let Some(first_comma) = sent.find(',') else {
        return sent.to_string();
    };
    let before = &sent[..first_comma];
    let after = &sent[first_comma + 1..];
    let Some(second_comma) = after.find(',') else {
        return sent.to_string();
    };
    let kept = &after[..second_comma];
    let rest = after[second_comma + 1..].replace(',', "");
    format!("{},{}{}", before, kept, rest)
}

/// Capitalizes the first letter of `text`, and the first letter after every run of
/// whitespace that immediately follows a `.`/`!`/`?` — mirrors the reference's
/// `re.sub(r'(^|[.!?]\s+)([^\W_])', ..., text)`. Requires at least one whitespace
/// character between the punctuation and the letter (matching `\s+`, not `\s*`); by the
/// time this runs, step 5 above has already guaranteed one is present.
fn capitalize_sentence_starts(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut capitalize_next = true;
    let mut after_sentence_punct = false;

    for ch in text.chars() {
        if capitalize_next && ch.is_alphanumeric() {
            result.extend(ch.to_uppercase());
            capitalize_next = false;
            after_sentence_punct = false;
            continue;
        }
        result.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            after_sentence_punct = true;
        } else if after_sentence_punct && ch.is_whitespace() {
            capitalize_next = true;
        } else if !ch.is_whitespace() {
            after_sentence_punct = false;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_whitespace_only_input_returns_empty() {
        assert_eq!(post_process(""), "");
        assert_eq!(post_process("   "), "");
    }

    #[test]
    fn colons_become_spaces() {
        assert_eq!(post_process("kết quả: tốt"), "Kết quả tốt");
    }

    #[test]
    fn repeated_commas_collapse_to_one() {
        assert_eq!(post_process("xin chào,, các bạn."), "Xin chào, các bạn.");
    }

    #[test]
    fn long_period_runs_cap_at_ellipsis() {
        assert_eq!(post_process("và sau đó......."), "Và sau đó...");
    }

    #[test]
    fn comma_directly_before_period_is_dropped() {
        assert_eq!(post_process("xin chào,."), "Xin chào.");
    }

    #[test]
    fn short_sentence_keeps_only_the_first_of_several_commas() {
        // 5 words, 3 commas -> only the first comma should survive.
        let input = "hôm nay, trời, đẹp, quá.";
        assert_eq!(post_process(input), "Hôm nay, trời đẹp quá.");
    }

    #[test]
    fn long_sentence_keeps_all_its_commas() {
        let input = "một, hai, ba, bốn, năm, sáu, bảy, tám, chín, mười.";
        let result = post_process(input);
        assert_eq!(result.matches(',').count(), 9, "8+ words: commas untouched");
    }

    #[test]
    fn missing_space_after_punctuation_gets_one_inserted() {
        assert_eq!(post_process("xin chào,các bạn."), "Xin chào, các bạn.");
    }

    #[test]
    fn extra_space_before_punctuation_is_removed() {
        assert_eq!(post_process("xin chào , các bạn ."), "Xin chào, các bạn.");
    }

    #[test]
    fn leading_comma_is_stripped() {
        assert_eq!(post_process(", xin chào."), "Xin chào.");
    }

    #[test]
    fn comma_right_after_a_period_is_removed() {
        assert_eq!(post_process("Xin chào.,Tạm biệt."), "Xin chào. Tạm biệt.");
    }

    #[test]
    fn whitespace_runs_collapse_to_a_single_space() {
        assert_eq!(post_process("xin   chào    bạn."), "Xin chào bạn.");
    }

    #[test]
    fn capitalizes_first_letter_of_the_whole_text() {
        assert_eq!(post_process("chào bạn."), "Chào bạn.");
    }

    #[test]
    fn capitalizes_after_every_sentence_boundary() {
        assert_eq!(
            post_process("xin chào. tạm biệt! hẹn gặp lại?"),
            "Xin chào. Tạm biệt! Hẹn gặp lại?"
        );
    }

    #[test]
    fn capitalizes_vietnamese_diacritic_letters_correctly() {
        assert_eq!(post_process("đây là ví dụ."), "Đây là ví dụ.");
    }

    #[test]
    fn does_not_capitalize_mid_sentence() {
        assert_eq!(post_process("xin chào các bạn."), "Xin chào các bạn.");
    }
}
