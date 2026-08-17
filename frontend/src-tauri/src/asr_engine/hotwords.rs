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

/// Hotwords to pass to sherpa-onnx: use DB value when set; if DB is NULL (never saved),
/// use bundled defaults; if DB is empty string (user cleared and saved), use no hotwords.
pub fn effective_hotwords_text(stored: Option<&str>, bundled_raw: Option<&str>) -> String {
    match stored {
        Some(s) if !s.trim().is_empty() => filter_hotwords_text(s),
        Some(_) => String::new(),
        None => filter_hotwords_text(bundled_raw.unwrap_or("")),
    }
}

/// Raw text for Settings UI: show bundled file when DB has never stored hotwords.
pub fn display_hotwords_text(stored: Option<&str>, bundled_raw: Option<&str>) -> Option<String> {
    match stored {
        Some(s) if !s.trim().is_empty() => Some(s.to_string()),
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
