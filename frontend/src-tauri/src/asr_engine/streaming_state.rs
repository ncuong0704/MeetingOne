//! Utterance state for live streaming ASR (no ONNX).
//! Partial emits reuse `sequence_id`; endpoint / max duration finalizes and advances.

#[derive(Debug, Clone, PartialEq)]
pub struct DecodeEmit {
    pub text: String,
    pub is_partial: bool,
    pub sequence_id: u64,
    pub audio_start_time: f64,
    pub audio_end_time: f64,
}

pub struct StreamingSession {
    sequence_id: u64,
    last_emitted_text: String,
    utterance_samples: u64,
    utterance_start_samples: u64,
    total_samples: u64,
    sample_rate: u32,
    max_utterance_secs: f64,
    pending_reset: bool,
}

impl StreamingSession {
    pub fn new(sample_rate: u32, max_utterance_secs: f64) -> Self {
        Self {
            sequence_id: 1,
            last_emitted_text: String::new(),
            utterance_samples: 0,
            utterance_start_samples: 0,
            total_samples: 0,
            sample_rate: sample_rate.max(1),
            max_utterance_secs,
            pending_reset: false,
        }
    }

    pub fn note_samples(&mut self, n: usize) {
        self.total_samples += n as u64;
        self.utterance_samples += n as u64;
    }

    /// Downsample mixed 48 kHz audio to 16 kHz by averaging groups of 3 samples.
    /// Leftover 1–2 samples stay in `remainder` for the next window.
    pub fn downsample_48k_to_16k(samples: &[f32], remainder: &mut Vec<f32>) -> Vec<f32> {
        remainder.extend_from_slice(samples);
        let n = remainder.len() / 3;
        let mut out = Vec::with_capacity(n);
        for chunk in remainder.chunks_exact(3).take(n) {
            out.push((chunk[0] + chunk[1] + chunk[2]) / 3.0);
        }
        let leftover = remainder.len() % 3;
        let start = remainder.len() - leftover;
        remainder.copy_within(start.., 0);
        remainder.truncate(leftover);
        out
    }

    pub fn take_pending_reset(&mut self) -> bool {
        let reset = self.pending_reset;
        self.pending_reset = false;
        reset
    }

    pub fn on_hypothesis(&mut self, raw_text: &str, is_endpoint: bool) -> Vec<DecodeEmit> {
        let text = raw_text.trim().to_lowercase();
        let speech_dur = self.utterance_samples as f64 / self.sample_rate as f64;
        let force_max = speech_dur > self.max_utterance_secs;
        let finalize = is_endpoint || force_max;

        if finalize {
            self.pending_reset = true;
            let emit_text = if !text.is_empty() {
                text
            } else {
                self.last_emitted_text.clone()
            };
            let out = if emit_text.is_empty() {
                Vec::new()
            } else {
                vec![self.build_emit(emit_text, false)]
            };
            self.reset_utterance();
            return out;
        }

        if text.is_empty() || text == self.last_emitted_text {
            return Vec::new();
        }
        self.last_emitted_text = text.clone();
        vec![self.build_emit(text, true)]
    }

    fn build_emit(&self, text: String, is_partial: bool) -> DecodeEmit {
        let start = self.utterance_start_samples as f64 / self.sample_rate as f64;
        let end = self.total_samples as f64 / self.sample_rate as f64;
        DecodeEmit {
            text,
            is_partial,
            sequence_id: self.sequence_id,
            audio_start_time: start,
            audio_end_time: end.max(start),
        }
    }

    fn reset_utterance(&mut self) {
        self.sequence_id = self.sequence_id.saturating_add(1);
        self.last_emitted_text.clear();
        self.utterance_samples = 0;
        self.utterance_start_samples = self.total_samples;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_then_final_share_sequence_id() {
        let mut s = StreamingSession::new(16000, 12.0);
        s.note_samples(1600);
        let p1 = s.on_hypothesis("xin", false);
        assert_eq!(p1.len(), 1);
        assert!(p1[0].is_partial);
        assert_eq!(p1[0].sequence_id, 1);
        assert_eq!(p1[0].text, "xin");

        s.note_samples(1600);
        let p2 = s.on_hypothesis("xin chao", false);
        assert_eq!(p2.len(), 1);
        assert!(p2[0].is_partial);
        assert_eq!(p2[0].sequence_id, 1);
        assert_eq!(p2[0].text, "xin chao");

        let f = s.on_hypothesis("xin chao", true);
        assert_eq!(f.len(), 1);
        assert!(!f[0].is_partial);
        assert_eq!(f[0].sequence_id, 1);
        assert!(s.take_pending_reset());

        s.note_samples(800);
        let next = s.on_hypothesis("ban", false);
        assert_eq!(next[0].sequence_id, 2);
        assert!(next[0].is_partial);
    }

    #[test]
    fn max_duration_finalizes_without_endpoint() {
        let mut s = StreamingSession::new(16000, 12.0);
        s.note_samples(16000 * 12 + 1);
        let out = s.on_hypothesis("xin chao", false);
        assert_eq!(out.len(), 1);
        assert!(!out[0].is_partial);
        assert_eq!(out[0].sequence_id, 1);
        assert!(s.take_pending_reset());
    }

    #[test]
    fn empty_endpoint_does_not_emit() {
        let mut s = StreamingSession::new(16000, 12.0);
        s.note_samples(3200);
        let out = s.on_hypothesis("  ", true);
        assert!(out.is_empty());
        assert!(s.take_pending_reset());

        s.note_samples(800);
        let next = s.on_hypothesis("alo", false);
        assert_eq!(next[0].sequence_id, 2);
    }

    #[test]
    fn unchanged_partial_is_silent() {
        let mut s = StreamingSession::new(16000, 12.0);
        s.note_samples(800);
        assert_eq!(s.on_hypothesis("xin", false).len(), 1);
        s.note_samples(800);
        assert!(s.on_hypothesis("xin", false).is_empty());
    }

    #[test]
    fn downsample_averages_triples_and_keeps_remainder() {
        let mut rem = Vec::new();
        let out = StreamingSession::downsample_48k_to_16k(&[1.0, 2.0, 3.0, 4.0, 5.0], &mut rem);
        assert_eq!(out, vec![2.0]);
        assert_eq!(rem, vec![4.0, 5.0]);
        let out2 = StreamingSession::downsample_48k_to_16k(&[6.0], &mut rem);
        assert_eq!(out2, vec![5.0]);
        assert!(rem.is_empty());
    }
}
