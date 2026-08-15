// Live recording: run CAPU once after ASR completes (at stop_recording), not during recording.

use crate::audio::recording_saver::TranscriptSegment;
use crate::capu_engine::batch::{CapuBatcher, FinalizedSegment, PendingSegment};
use log::warn;

/// Runs CAPU over all non-user-edited transcript segments collected during a live session.
pub fn finalize_live_with_capu(segments: &[TranscriptSegment]) -> Vec<FinalizedSegment> {
    let mut batcher = CapuBatcher::new();
    let mut finalized = Vec::new();
    let mut batch_speaker: Option<Option<String>> = None;
    let engine_arc = crate::capu_engine::commands::get_engine_arc();

    for seg in segments {
        if seg.user_edited {
            continue;
        }
        if seg.text.trim().is_empty() {
            continue;
        }
        if let Some(current) = &batch_speaker {
            if current != &seg.speaker_name && !batcher.is_empty() {
                flush_batch(&mut batcher, &engine_arc, &mut finalized);
            }
        }
        batch_speaker = Some(seg.speaker_name.clone());
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

    fn seg(id: u64, text: &str, speaker: Option<&str>, user_edited: bool) -> TranscriptSegment {
        TranscriptSegment {
            id: format!("t{id}"),
            text: text.to_string(),
            audio_start_time: id as f64,
            audio_end_time: id as f64 + 1.0,
            duration: 1.0,
            display_time: "[00:00]".to_string(),
            confidence: 0.9,
            sequence_id: id,
            user_edited,
            speaker_name: speaker.map(|s| s.to_string()),
        }
    }

    #[test]
    fn finalize_live_with_capu_groups_small_segments_without_engine() {
        let segments = vec![
            seg(0, "xin chao", None, false),
            seg(1, "cac ban", None, false),
        ];
        let out = finalize_live_with_capu(&segments);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source_ids, vec![0, 1]);
        assert_eq!(out[0].text, "xin chao cac ban");
    }

    #[test]
    fn finalize_live_with_capu_skips_user_edited() {
        let segments = vec![seg(0, "edited", None, true)];
        let out = finalize_live_with_capu(&segments);
        assert!(out.is_empty());
    }

    #[test]
    fn finalize_live_does_not_merge_across_speaker_change() {
        let segments = vec![
            seg(0, "xin chao", Some("Lan"), false),
            seg(1, "xin chao", Some("Minh"), false),
        ];
        let out = finalize_live_with_capu(&segments);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].source_ids, vec![0]);
        assert_eq!(out[1].source_ids, vec![1]);
        assert_eq!(out[0].text, "xin chao");
        assert_eq!(out[1].text, "xin chao");
    }
}
