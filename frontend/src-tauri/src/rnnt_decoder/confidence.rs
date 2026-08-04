// frontend/src-tauri/src/rnnt_decoder/confidence.rs
//
// Per-token and per-word confidence scoring for the hand-written RNNT decoder path.
// `margin` = softmax probability gap between the top-1 and runner-up token (large gap
// = unambiguous decision = confident). `tsallis_norm` = normalized Tsallis-2 (collision)
// entropy of the softmax distribution, in [0, 1] (0 = a single dominant token, 1 =
// uniform/maximally uncertain). `word_confidence` combines both: a word is only
// confident if EVERY constituent token was both unambiguous (min margin) and low-entropy
// (max tsallis) — see `WordResult`'s doc comment in `engine.rs` for the exact formula.
//
// NOTE: this file was reconstructed after accidental deletion of the untracked
// original during development (see git history around 2026-08-04). The
// `word_confidence` formula is preserved verbatim from a surviving doc comment in
// `engine.rs`; `compute_token_confidence`'s exact margin/Tsallis-entropy math is a
// best-effort reconstruction of a standard technique, not a byte-for-byte restoration.

pub struct TokenConfidence {
    pub margin: f32,
    pub tsallis_norm: f32,
}

fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = logits.iter().map(|&x| (x - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    exps.into_iter().map(|x| x / sum).collect()
}

/// Confidence for one emitted token, from its raw joiner logits at the step it was
/// emitted.
pub fn compute_token_confidence(logits: &[f32]) -> TokenConfidence {
    if logits.is_empty() {
        return TokenConfidence {
            margin: 0.0,
            tsallis_norm: 1.0,
        };
    }

    let probs = softmax(logits);

    let mut sorted = probs.clone();
    sorted.sort_unstable_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let margin = if sorted.len() >= 2 {
        sorted[0] - sorted[1]
    } else {
        1.0
    };

    // Tsallis-2 (collision) entropy: 1 - sum(p_i^2), normalized by its max possible
    // value (1 - 1/n) for an n-way distribution so the result always lands in [0, 1].
    let n = probs.len() as f32;
    let sum_sq: f32 = probs.iter().map(|p| p * p).sum();
    let raw = 1.0 - sum_sq;
    let max_raw = 1.0 - 1.0 / n;
    let tsallis_norm = if max_raw > 0.0 {
        (raw / max_raw).clamp(0.0, 1.0)
    } else {
        0.0
    };

    TokenConfidence { margin, tsallis_norm }
}

/// Combines a word's minimum per-token margin and maximum per-token entropy into one
/// confidence score in [0, 1]. Formula preserved verbatim from `WordResult`'s doc
/// comment in `engine.rs`.
pub fn word_confidence(margin_min: f32, tsallis_max: f32) -> f32 {
    (margin_min * (1.0 - tsallis_max)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_token_confidence_is_maximally_confident_for_a_dominant_logit() {
        let logits = vec![10.0, -10.0, -10.0, -10.0];
        let conf = compute_token_confidence(&logits);
        assert!(conf.margin > 0.99, "margin={}", conf.margin);
        assert!(conf.tsallis_norm < 0.01, "tsallis_norm={}", conf.tsallis_norm);
    }

    #[test]
    fn compute_token_confidence_is_maximally_uncertain_for_uniform_logits() {
        let logits = vec![0.0, 0.0, 0.0, 0.0];
        let conf = compute_token_confidence(&logits);
        assert!(conf.margin < 1e-6, "margin={}", conf.margin);
        assert!(
            (conf.tsallis_norm - 1.0).abs() < 1e-4,
            "tsallis_norm={}",
            conf.tsallis_norm
        );
    }

    #[test]
    fn compute_token_confidence_handles_single_logit_without_panicking() {
        let conf = compute_token_confidence(&[5.0]);
        assert_eq!(conf.margin, 1.0);
    }

    #[test]
    fn compute_token_confidence_handles_empty_logits_without_panicking() {
        let conf = compute_token_confidence(&[]);
        assert_eq!(conf.margin, 0.0);
        assert_eq!(conf.tsallis_norm, 1.0);
    }

    #[test]
    fn word_confidence_matches_documented_formula() {
        assert!((word_confidence(0.8, 0.2) - 0.8 * (1.0 - 0.2)).abs() < 1e-6);
    }

    #[test]
    fn word_confidence_is_zero_when_max_entropy() {
        assert_eq!(word_confidence(0.9, 1.0), 0.0);
    }
}
