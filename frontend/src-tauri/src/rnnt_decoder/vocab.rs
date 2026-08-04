// frontend/src-tauri/src/rnnt_decoder/vocab.rs
//
// BPE/SentencePiece vocabulary for the hand-written RNNT decoder path, plus the
// piece-to-word merge step (SentencePiece convention: a piece prefixed with `▁`
// (U+2581) starts a new word; a piece without it continues the previous word).
//
// NOTE: this file was reconstructed after accidental deletion of the untracked
// original during development (see git history around 2026-08-04). The tokens.txt
// format (`<piece> <id>` per line) is verified against a real model file
// (models/zipformer-30m-rnnt-6000h/tokens.txt in the reference app); the word-merge
// logic and confidence-aggregation choices (min margin, max tsallis per word) are a
// best-effort reconstruction consistent with `WordResult`'s surviving doc comment in
// `engine.rs`, not a byte-for-byte restoration of the original file.

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::Path;

const WORD_BOUNDARY_MARKER: char = '\u{2581}'; // SentencePiece "▁"

pub struct Vocab {
    id_to_piece: HashMap<i64, String>,
    num_pieces: usize,
}

impl Vocab {
    /// Parses a `tokens.txt` file: one `<piece> <id>` pair per line (whitespace
    /// separated), e.g. `<blk> 0`, `▁HAI 3`. Lines that don't match this shape are
    /// skipped rather than treated as a hard error, since blank trailing lines are
    /// common in these files.
    pub fn from_tokens_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow!("Failed to read tokens file {:?}: {}", path, e))?;

        let mut id_to_piece = HashMap::new();
        let mut max_id: i64 = -1;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.rsplitn(2, char::is_whitespace);
            let id_str = match parts.next() {
                Some(s) => s,
                None => continue,
            };
            let piece = match parts.next() {
                Some(s) => s.trim(),
                None => continue,
            };
            let id: i64 = match id_str.parse() {
                Ok(id) => id,
                Err(_) => continue,
            };
            max_id = max_id.max(id);
            id_to_piece.insert(id, piece.to_string());
        }

        if id_to_piece.is_empty() {
            return Err(anyhow!("No valid entries found in tokens file {:?}", path));
        }

        Ok(Self {
            id_to_piece,
            num_pieces: (max_id + 1) as usize,
        })
    }

    pub fn vocab_size(&self) -> usize {
        self.num_pieces
    }

    pub fn piece(&self, id: i64) -> Option<&str> {
        self.id_to_piece.get(&id).map(|s| s.as_str())
    }
}

/// One decoded word, merged from one or more `PieceToken`s.
pub struct Word {
    pub text: String,
    pub start_frame: usize,
    pub end_frame: usize,
    /// Minimum per-token margin across the word's constituent pieces — the word is
    /// only as confident as its least-confident piece.
    pub margin_min: f32,
    /// Maximum per-token Tsallis entropy across the word's constituent pieces — the
    /// word is only as certain as its most-uncertain piece.
    pub tsallis_max: f32,
}

/// One emitted BPE piece with its frame position and per-token confidence, ready to be
/// merged into words.
pub struct PieceToken {
    pub id: i64,
    pub frame: usize,
    pub margin: f32,
    pub tsallis_norm: f32,
}

/// Merges a flat sequence of BPE pieces into words, splitting at each piece that starts
/// with the SentencePiece word-boundary marker `▁`. A piece with an unknown id (not in
/// `vocab`) is skipped rather than erroring, since a single bad id shouldn't lose the
/// entire utterance.
pub fn pieces_to_words(vocab: &Vocab, pieces: &[PieceToken]) -> Result<Vec<Word>> {
    let mut words: Vec<Word> = Vec::new();

    for piece in pieces {
        let raw = match vocab.piece(piece.id) {
            Some(p) => p,
            None => continue,
        };

        let starts_new_word = raw.starts_with(WORD_BOUNDARY_MARKER) || words.is_empty();
        let text_part = raw.trim_start_matches(WORD_BOUNDARY_MARKER);

        if starts_new_word {
            words.push(Word {
                text: text_part.to_string(),
                start_frame: piece.frame,
                end_frame: piece.frame + 1,
                margin_min: piece.margin,
                tsallis_max: piece.tsallis_norm,
            });
        } else if let Some(last) = words.last_mut() {
            last.text.push_str(text_part);
            last.end_frame = piece.frame + 1;
            last.margin_min = last.margin_min.min(piece.margin);
            last.tsallis_max = last.tsallis_max.max(piece.tsallis_norm);
        }
    }

    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_tokens_file(dir: &std::path::Path, content: &str) -> std::path::PathBuf {
        let path = dir.join("tokens.txt");
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn from_tokens_file_parses_piece_and_id_pairs() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_tokens_file(dir.path(), "<blk> 0\n<unk> 1\n\u{2581}XIN 2\n");
        let vocab = Vocab::from_tokens_file(&path).unwrap();
        assert_eq!(vocab.vocab_size(), 3);
        assert_eq!(vocab.piece(0), Some("<blk>"));
        assert_eq!(vocab.piece(2), Some("\u{2581}XIN"));
        assert_eq!(vocab.piece(99), None);
    }

    #[test]
    fn from_tokens_file_errors_on_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_tokens_file(dir.path(), "");
        assert!(Vocab::from_tokens_file(&path).is_err());
    }

    #[test]
    fn pieces_to_words_splits_at_word_boundary_marker() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_tokens_file(dir.path(), "\u{2581}XIN 0\n\u{2581}CHA 1\nO 2\n");
        let vocab = Vocab::from_tokens_file(&path).unwrap();

        let pieces = vec![
            PieceToken {
                id: 0,
                frame: 0,
                margin: 0.9,
                tsallis_norm: 0.1,
            },
            PieceToken {
                id: 1,
                frame: 5,
                margin: 0.5,
                tsallis_norm: 0.4,
            },
            PieceToken {
                id: 2,
                frame: 6,
                margin: 0.7,
                tsallis_norm: 0.2,
            },
        ];

        let words = pieces_to_words(&vocab, &pieces).unwrap();
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "XIN");
        assert_eq!(words[1].text, "CHAO");
        assert_eq!(words[1].start_frame, 5);
        assert_eq!(words[1].end_frame, 7);
        assert!(
            (words[1].margin_min - 0.5).abs() < 1e-6,
            "expected min of 0.5 and 0.7"
        );
        assert!(
            (words[1].tsallis_max - 0.4).abs() < 1e-6,
            "expected max of 0.4 and 0.2"
        );
    }

    #[test]
    fn pieces_to_words_skips_unknown_ids() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_tokens_file(dir.path(), "\u{2581}XIN 0\n");
        let vocab = Vocab::from_tokens_file(&path).unwrap();

        let pieces = vec![
            PieceToken {
                id: 0,
                frame: 0,
                margin: 0.9,
                tsallis_norm: 0.1,
            },
            PieceToken {
                id: 999,
                frame: 1,
                margin: 0.5,
                tsallis_norm: 0.5,
            },
        ];

        let words = pieces_to_words(&vocab, &pieces).unwrap();
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "XIN");
    }
}
