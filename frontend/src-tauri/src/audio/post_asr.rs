/// Lowercases raw ASR output — the cheap part of post-ASR processing, safe to run inline
/// on the live transcription hot path. CAPU (punctuation/capitalization) is intentionally
/// NOT applied here — see `CapuBatcher` (capu_engine::batch), which runs it off the hot
/// path in batches.
///
/// No longer applies Vietnamese ITN (number/unit normalization) here — CAPU's model was
/// trained on already-ITN'd text (digit-form numbers), so ITN has to run before CAPU to
/// stay on-distribution; removed rather than reordered per explicit decision to match the
/// reference app, which doesn't do ITN at all.
pub fn normalize_asr_text(raw: &str) -> String {
    raw.to_lowercase()
}

/// Apply CAPU to raw (lowercased) ASR text. Falls back gracefully on any failure. When the
/// punctuation level is at its minimum (1), CAPU is skipped entirely — matching the
/// reference app's `bypass_restorer` behavior — rather than running inference with an
/// extreme bias. Used by the file/batch paths (`import.rs`, `retranscription.rs`), which
/// still call CAPU per-segment today. The live path uses `normalize_asr_text` +
/// `CapuBatcher` instead.
pub fn process_asr_text(raw: &str, capu_trailing: &mut Vec<String>) -> String {
    let normalized = normalize_asr_text(raw);

    match crate::capu_engine::commands::get_engine_arc() {
        Some(engine_arc) => {
            let mut engine = engine_arc.lock().unwrap();
            if engine.punctuation_level() <= 1 {
                return normalized;
            }
            match engine.restore_punctuation(capu_trailing, &normalized) {
                Ok((restored, next_context)) => {
                    *capu_trailing = next_context;
                    restored
                }
                Err(e) => {
                    log::warn!("CAPU failed: {}", e);
                    normalized
                }
            }
        }
        None => normalized,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_asr_text_lowercases_input() {
        assert_eq!(normalize_asr_text("XIN CHAO"), "xin chao");
    }

    #[test]
    fn normalize_asr_text_does_not_panic_on_empty_input() {
        assert_eq!(normalize_asr_text(""), "");
    }
}
