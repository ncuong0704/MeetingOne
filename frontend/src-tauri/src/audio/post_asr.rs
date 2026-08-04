/// Apply ITN then CAPU to raw ASR text. Falls back gracefully on any failure. When the
/// punctuation level is at its minimum (1), CAPU is skipped entirely — matching the
/// reference app's `bypass_restorer` behavior — rather than running inference with an
/// extreme bias.
pub fn process_asr_text(raw: &str, capu_trailing: &mut Vec<String>) -> String {
    let lowered = raw.to_lowercase();
    let after_itn = crate::itn_engine::engine::inverse_normalize_or_pass(&lowered);

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
