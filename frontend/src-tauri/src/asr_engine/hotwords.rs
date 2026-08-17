use anyhow::{anyhow, Result};
use sentencepiece_model::SentencePieceModel;
use std::path::{Path, PathBuf};

/// Strips comment lines (`#...`) and blank lines from raw hotword text. Sherpa-onnx
/// natively understands the `PHRASE :score` syntax on each remaining line — Meetily does
/// not parse the score itself, only removes what sherpa-onnx wouldn't expect to see.
pub fn filter_hotwords_text(raw: &str) -> String {
    raw.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Ensures a `bpe.vocab` (piece\tscore per line) file exists next to `bpe_model_path`,
/// generating it from the binary SentencePiece model if missing (cached — regenerated
/// only if the `.vocab` file doesn't already exist). Returns `None`, not an error, on
/// failure: hotwords are an optional enhancement, and a bad/unreadable `bpe.model` must
/// not block loading the ASR model itself.
pub fn ensure_bpe_vocab(bpe_model_path: &Path) -> Option<PathBuf> {
    let vocab_path = bpe_model_path.with_extension("vocab");
    if vocab_path.exists() {
        return Some(vocab_path);
    }
    match generate_bpe_vocab(bpe_model_path, &vocab_path) {
        Ok(()) => Some(vocab_path),
        Err(e) => {
            log::warn!("Failed to generate bpe.vocab from {:?}: {}", bpe_model_path, e);
            None
        }
    }
}

fn generate_bpe_vocab(bpe_model_path: &Path, vocab_path: &Path) -> Result<()> {
    let model = SentencePieceModel::from_file(bpe_model_path)
        .map_err(|e| anyhow!("Failed to parse {:?}: {}", bpe_model_path, e))?;

    let mut out = String::new();
    for piece in model.pieces() {
        let text = piece.piece.as_deref().unwrap_or("");
        let score = piece.score.unwrap_or(0.0);
        out.push_str(text);
        out.push('\t');
        out.push_str(&score.to_string());
        out.push('\n');
    }

    std::fs::write(vocab_path, out).map_err(|e| anyhow!("Failed to write {:?}: {}", vocab_path, e))
}

fn hotword_phrase_key(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let phrase = trimmed
        .rsplit_once(" :")
        .map(|(p, _)| p.trim())
        .unwrap_or(trimmed);
    Some(phrase.to_lowercase())
}

fn extra_hotword_lines<'a>(stored: &'a str, bundled: &str) -> Vec<&'a str> {
    let bundled_keys: std::collections::HashSet<String> = bundled
        .lines()
        .filter_map(hotword_phrase_key)
        .collect();
    stored
        .lines()
        .filter(|line| {
            hotword_phrase_key(line)
                .map(|key| !bundled_keys.contains(&key))
                .unwrap_or(false)
        })
        .collect()
}

/// Rebuild user hotwords against the current bundled file: keep official lines (so app
/// updates pick up new defaults) and append phrases the user added that are not in bundled.
pub fn overlay_hotwords(stored: &str, bundled: &str) -> String {
    let extras = extra_hotword_lines(stored, bundled);
    if extras.is_empty() {
        return bundled.to_string();
    }
    let mut out = bundled.trim_end().to_string();
    out.push('\n');
    out.push_str(&extras.join("\n"));
    out
}

/// What to store in DB: NULL follows bundled across updates; empty string disables;
/// any other string keeps user extras (and old snapshots that already differ).
pub fn persist_hotwords_value(textarea: Option<&str>, bundled: Option<&str>) -> Option<String> {
    match textarea {
        None => None,
        Some(s) if s.trim().is_empty() => Some(String::new()),
        Some(s) => {
            let bundled = bundled.unwrap_or("");
            if extra_hotword_lines(s, bundled).is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        }
    }
}

/// Hotwords to pass to sherpa-onnx: use DB value when set; if DB is NULL (never saved),
/// use bundled defaults; if DB is empty string (user cleared and saved), use no hotwords.
/// Non-empty DB values are overlaid so a new bundled list is included after app updates.
pub fn effective_hotwords_text(stored: Option<&str>, bundled_raw: Option<&str>) -> String {
    match stored {
        Some(s) if !s.trim().is_empty() => {
            filter_hotwords_text(&overlay_hotwords(s, bundled_raw.unwrap_or("")))
        }
        Some(_) => String::new(),
        None => filter_hotwords_text(bundled_raw.unwrap_or("")),
    }
}

/// Raw text for Settings UI: show bundled file when DB has never stored hotwords.
/// Stored lists are overlaid so the textarea shows current defaults plus user extras.
pub fn display_hotwords_text(stored: Option<&str>, bundled_raw: Option<&str>) -> Option<String> {
    match stored {
        Some(s) if !s.trim().is_empty() => {
            Some(overlay_hotwords(s, bundled_raw.unwrap_or("")))
        }
        Some(_) => Some(String::new()),
        None => bundled_raw.map(|s| s.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_hotwords_text_uses_bundled_when_db_is_null() {
        let bundled = "ỦY BAN NHÂN DÂN :2.5\n# skip";
        assert_eq!(
            effective_hotwords_text(None, Some(bundled)),
            "ỦY BAN NHÂN DÂN :2.5"
        );
    }

    #[test]
    fn effective_hotwords_text_empty_string_means_user_disabled_hotwords() {
        assert_eq!(effective_hotwords_text(Some(""), Some("TERM")), "");
        assert_eq!(effective_hotwords_text(Some("  "), Some("TERM")), "");
    }

    #[test]
    fn display_hotwords_text_shows_bundled_when_db_is_null() {
        assert_eq!(
            display_hotwords_text(None, Some("LINE1\nLINE2")),
            Some("LINE1\nLINE2".to_string())
        );
    }

    #[test]
    fn overlay_hotwords_follows_bundled_when_stored_is_old_snapshot() {
        let stored = "Ban Chấp Hành\nThành Ủy\n";
        let bundled = "Ban Chấp Hành\nThành Ủy\nỦy Ban Nhân Dân\n";
        assert_eq!(overlay_hotwords(stored, bundled), bundled);
    }

    #[test]
    fn overlay_hotwords_keeps_user_extras_after_bundled() {
        let stored = "Ban Chấp Hành\nANH MINH :2.5\n";
        let bundled = "# Đảng\nBan Chấp Hành\nThành Ủy\n";
        assert_eq!(
            overlay_hotwords(stored, bundled),
            "# Đảng\nBan Chấp Hành\nThành Ủy\nANH MINH :2.5"
        );
    }

    #[test]
    fn persist_hotwords_value_clears_db_when_textarea_matches_bundled() {
        let bundled = "Ban Chấp Hành\nThành Ủy\n";
        assert_eq!(persist_hotwords_value(Some(bundled), Some(bundled)), None);
        assert_eq!(
            persist_hotwords_value(Some("Ban Chấp Hành\n"), Some(bundled)),
            None
        );
        assert_eq!(persist_hotwords_value(None, Some(bundled)), None);
        assert_eq!(
            persist_hotwords_value(Some("ANH MINH\n"), Some(bundled)),
            Some("ANH MINH\n".to_string())
        );
        assert_eq!(persist_hotwords_value(Some(""), Some(bundled)), Some(String::new()));
    }

    #[test]
    fn effective_hotwords_text_uses_bundled_when_stored_is_subset() {
        let stored = "TERM_A\n";
        let bundled = "TERM_A\nTERM_B\n# skip";
        assert_eq!(effective_hotwords_text(Some(stored), Some(bundled)), "TERM_A\nTERM_B");
    }

    #[test]
    fn effective_hotwords_text_includes_user_extras() {
        let stored = "CUSTOM\n";
        let bundled = "TERM_A\n";
        assert_eq!(
            effective_hotwords_text(Some(stored), Some(bundled)),
            "TERM_A\nCUSTOM"
        );
    }

    #[test]
    fn filter_hotwords_text_strips_comments_and_blank_lines() {
        let raw = "# comment\nỦY BAN NHÂN DÂN :2.5\n\n  \nCHUYỂN ĐỔI SỐ\n#another comment";
        let filtered = filter_hotwords_text(raw);
        assert_eq!(filtered, "ỦY BAN NHÂN DÂN :2.5\nCHUYỂN ĐỔI SỐ");
    }

    #[test]
    fn filter_hotwords_text_on_empty_or_blank_input_returns_empty() {
        assert_eq!(filter_hotwords_text(""), "");
        assert_eq!(filter_hotwords_text("\n\n  \n"), "");
    }

    #[test]
    fn filter_hotwords_text_keeps_inline_score_syntax_untouched() {
        let raw = "BAN CHẤP HÀNH :3.0";
        assert_eq!(filter_hotwords_text(raw), "BAN CHẤP HÀNH :3.0");
    }

    #[test]
    fn bundled_hotwords_txt_is_nonempty_after_filter() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("mac-dinh")
            .join("hotwords.txt");
        let raw = std::fs::read_to_string(&path).expect("bundled hotwords.txt");
        let filtered = filter_hotwords_text(&raw);
        assert!(filtered.lines().count() > 10);
        assert!(filtered.contains("Ủy Ban Nhân Dân"));
        assert!(filtered.contains("Ban Chấp Hành"));
        assert!(!filtered.lines().any(|l| l.starts_with('#')));
    }

    /// Ignored by default — needs a real downloaded model's bpe.model. Run explicitly:
    /// `cargo test asr_engine::hotwords::tests::ensure_bpe_vocab_generates_a_real_model -- --ignored --nocapture`
    /// with `BPE_MODEL_PATH` pointing at e.g. the already-downloaded
    /// `zipformer-vi-int8/bpe.model` under the app's models directory.
    #[test]
    #[ignore]
    fn ensure_bpe_vocab_generates_a_real_model() {
        let bpe_model_path = std::path::PathBuf::from(
            std::env::var("BPE_MODEL_PATH").expect("set BPE_MODEL_PATH to a real bpe.model"),
        );
        let vocab_path = bpe_model_path.with_extension("vocab");
        let _ = std::fs::remove_file(&vocab_path);

        let result = ensure_bpe_vocab(&bpe_model_path);
        assert!(result.is_some(), "ensure_bpe_vocab should succeed on a real bpe.model");

        let content = std::fs::read_to_string(&vocab_path).expect("read generated vocab");
        assert!(
            content.lines().count() > 0,
            "generated bpe.vocab should have at least one entry"
        );
    }
}
