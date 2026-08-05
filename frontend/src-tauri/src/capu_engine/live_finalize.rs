// Live recording: run CAPU once after ASR completes (at stop_recording), not during recording.

use crate::audio::recording_saver::TranscriptSegment;
use crate::capu_engine::batch::{CapuBatcher, FinalizedSegment, PendingSegment};
use log::warn;

/// Runs CAPU over all non-user-edited transcript segments collected during a live session.
pub fn finalize_live_with_capu(segments: &[TranscriptSegment]) -> Vec<FinalizedSegment> {
    let mut batcher = CapuBatcher::new();
    let mut finalized = Vec::new();
    let engine_arc = crate::capu_engine::commands::get_engine_arc();

    for seg in segments {
        if seg.user_edited {
            continue;
        }
        if seg.text.trim().is_empty() {
            continue;
        }
        batcher.push(PendingSegment {
            source_id: seg.sequence_id,
            raw_text: seg.text.clone(),
            audio_start_time: seg.audio_start_time,
            audio_end_time: seg.audio_end_time,
        });
        if batcher.should_flush(crate::config::CAPU_BATCH_WORD_BUDGET) {
            flush_batch(&mut batcher, &engine_arc, &mut finalized);
        }
    }

    if !batcher.is_empty() {
        flush_batch(&mut batcher, &engine_arc, &mut finalized);
    }

    finalized
}

fn flush_batch(
    batcher: &mut CapuBatcher,
    engine_arc: &Option<std::sync::Arc<std::sync::Mutex<crate::capu_engine::CapuEngine>>>,
    out: &mut Vec<FinalizedSegment>,
) {
    let result = match engine_arc {
        Some(arc) => {
            let mut engine = arc.lock().unwrap();
            batcher.flush_with_fallback(Some(&mut engine))
        }
        None => batcher.flush_with_fallback(None),
    };

    if let Some(f) = result {
        out.push(f);
    } else if !batcher.is_empty() {
        warn!("finalize_live_with_capu: batch flush returned None while pending segments remain");
        batcher.discard_pending();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finalize_live_with_capu_groups_small_segments_without_engine() {
        let segments = vec![
            TranscriptSegment {
                id: "t1".to_string(),
                text: "xin chao".to_string(),
                audio_start_time: 0.0,
                audio_end_time: 1.0,
                duration: 1.0,
                display_time: "[00:00]".to_string(),
                confidence: 0.9,
                sequence_id: 0,
                user_edited: false,
            },
            TranscriptSegment {
                id: "t2".to_string(),
                text: "cac ban".to_string(),
                audio_start_time: 1.0,
                audio_end_time: 2.0,
                duration: 1.0,
                display_time: "[00:01]".to_string(),
                confidence: 0.9,
                sequence_id: 1,
                user_edited: false,
            },
        ];
        let out = finalize_live_with_capu(&segments);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source_ids, vec![0, 1]);
        assert_eq!(out[0].text, "xin chao cac ban");
    }

    #[test]
    fn finalize_live_with_capu_skips_user_edited() {
        let segments = vec![
            TranscriptSegment {
                id: "t1".to_string(),
                text: "edited".to_string(),
                audio_start_time: 0.0,
                audio_end_time: 1.0,
                duration: 1.0,
                display_time: "[00:00]".to_string(),
                confidence: 0.9,
                sequence_id: 0,
                user_edited: true,
            },
        ];
        let out = finalize_live_with_capu(&segments);
        assert!(out.is_empty());
    }
}
