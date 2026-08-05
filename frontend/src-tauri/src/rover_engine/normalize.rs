use unicode_normalization::UnicodeNormalization;

/// Normalizes a word for cross-model comparison: lowercase, trim, NFC-normalize
/// (so precomposed and decomposed Vietnamese diacritics compare equal), then keep
/// only alphanumeric characters. Mirrors `normalize_word_for_overlap` in the
/// reference app.
pub fn normalize_word(word: &str) -> String {
    let lowered = word.to_lowercase();
    let trimmed = lowered.trim();
    let nfc_normalized: String = trimmed.nfc().collect();
    nfc_normalized.chars().filter(|c| c.is_alphanumeric()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_and_strips_whitespace() {
        assert_eq!(normalize_word("  HAI  "), "hai");
    }

    #[test]
    fn strips_punctuation() {
        assert_eq!(normalize_word("dấu?"), "dấu");
        assert_eq!(normalize_word("chào,"), "chào");
    }

    #[test]
    fn nfc_normalizes_combining_diacritics() {
        // "ế" as a single precomposed codepoint (U+1EBF) vs "e" + combining
        // circumflex (U+0302) + combining acute (U+0301) must normalize equal —
        // ASR output and hand-typed test strings can differ in which form they use.
        let precomposed = "\u{1EBF}"; // "ế"
        let decomposed = "e\u{0302}\u{0301}"; // "e" + combining circumflex + combining acute
        assert_eq!(normalize_word(precomposed), normalize_word(decomposed));
    }

    #[test]
    fn empty_string_normalizes_to_empty() {
        assert_eq!(normalize_word(""), "");
        assert_eq!(normalize_word("   "), "");
    }
}
