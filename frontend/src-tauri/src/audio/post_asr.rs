/// Lowercase + inverse text normalization (numbers, units, etc.) — the cheap part of
/// post-ASR processing, safe to run inline on the live transcription hot path. CAPU
/// (punctuation/capitalization) is intentionally NOT applied here — see `CapuBatcher`
/// (capu_engine::batch), which runs it off the hot path in batches.
pub fn apply_itn(raw: &str) -> String {
    let lowered = raw.to_lowercase();
    crate::itn_engine::engine::inverse_normalize_or_pass(&lowered)
}

/// Apply ITN then CAPU to raw ASR text. Falls back gracefully on any failure. When the
/// punctuation level is at its minimum (1), CAPU is skipped entirely — matching the
/// reference app's `bypass_restorer` behavior — rather than running inference with an
/// extreme bias. Used by the file/batch paths (`import.rs`, `retranscription.rs`), which
/// still call CAPU per-segment today. The live path uses `apply_itn` + `CapuBatcher` instead.
pub fn process_asr_text(raw: &str, capu_trailing: &mut Vec<String>) -> String {
    let after_itn = apply_itn(raw);

    match crate::capu_engine::commands::get_engine_arc() {
        Some(engine_arc) => {
            let mut engine = engine_arc.lock().unwrap();
            if engine.punctuation_level() <= 1 {
                return after_itn;
            }
            match engine.restore_punctuation(capu_trailing, &after_itn) {
                Ok((restored, next_context)) => {
                    *capu_trailing = next_context;
                    restored
                }
                Err(e) => {
                    log::warn!("CAPU failed after ITN: {}", e);
                    after_itn
                }
            }
        }
        None => after_itn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_itn_lowercases_input() {
        let result = apply_itn("XIN CHAO");
        assert_eq!(result, result.to_lowercase());
    }

    #[test]
    fn apply_itn_does_not_panic_on_empty_input() {
        assert_eq!(apply_itn(""), "");
    }
}
