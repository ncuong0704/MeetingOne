use anyhow::{anyhow, Result};
use std::path::Path;
use tokenizers::models::wordpiece::WordPiece;
use tokenizers::normalizers::bert::BertNormalizer;
use tokenizers::pre_tokenizers::bert::BertPreTokenizer;
use tokenizers::{AddedToken, Model, Tokenizer};

/// The reference GECToR model was exported with a dedicated `$START` embedding appended
/// after the base WordPiece vocab (`config.json`: `bert_vocab_size: 38168` for a
/// 38167-line `vocab.txt` — one extra row), and its input pipeline uses
/// `add_special_tokens=False`: no `[CLS]`/`[SEP]`, just `$START` prepended to the word
/// list. Feeding this model `[CLS]`/`[SEP]` instead — plausible-looking since they're
/// also BERT special tokens, but never trained for this role here — means the model's
/// very first hidden state comes from an embedding it never learned to use that way, and
/// every real word's self-attention sees a foreign trailing `[SEP]` it was never trained
/// to expect. `encode_words` below reproduces the reference scheme exactly instead.
const START_TOKEN: &str = "$START";

/// Encoded representation ready to feed to the ONNX session: `input_offsets` here is
/// the *raw* offsets list (length == words.len() + 1 — one for the prepended `$START`,
/// one per real word, no CLS/SEP) — callers skip only the first entry when reading
/// per-word predictions back out (see capu_engine.rs).
pub struct CapuEncoding {
    pub input_ids: Vec<i64>,
    pub attention_mask: Vec<i64>,
    pub token_type_ids: Vec<i64>,
    pub input_offsets: Vec<i64>,
}

pub struct CapuTokenizer {
    tokenizer: Tokenizer,
}

impl CapuTokenizer {
    /// Builds a cased BERT WordPiece tokenizer from a raw `vocab.txt` file. Vietnamese
    /// relies heavily on diacritics, so this is explicitly a *cased* tokenizer:
    /// `strip_accents: Some(false)`, `lowercase: false`. These aren't a guess — they
    /// follow directly from the base model being named `vibert-base-cased`.
    pub fn from_vocab_file(vocab_path: &Path) -> Result<Self> {
        let vocab_path_str = vocab_path
            .to_str()
            .ok_or_else(|| anyhow!("Non-UTF8 vocab path: {:?}", vocab_path))?;

        let wordpiece: WordPiece = WordPiece::from_file(vocab_path_str)
            .build()
            .map_err(|e| anyhow!("Failed to build WordPiece from {:?}: {}", vocab_path, e))?;

        // Not used directly, but `WordPieceBuilder::build()` does not validate that
        // `unk_token` exists in the vocab — it only surfaces as `MissingUnkToken` later,
        // inside `encode`, the first time a word actually needs UNK fallback. Since
        // `vocab.txt` is downloaded over HTTP and a truncated/corrupted download is a
        // real failure mode, validate `[UNK]` eagerly here.
        wordpiece
            .token_to_id("[UNK]")
            .ok_or_else(|| anyhow!("[UNK] not found in vocab {:?}", vocab_path))?;

        let mut tokenizer = Tokenizer::new(wordpiece);
        tokenizer
            .with_normalizer(Some(BertNormalizer::new(true, true, Some(false), false)))
            .map_err(|e| anyhow!("Failed to set normalizer for {:?}: {}", vocab_path, e))?;
        tokenizer.with_pre_tokenizer(Some(BertPreTokenizer));
        // No post-processor: the reference model's input is exactly `$START` + words,
        // nothing else added around it.

        // Registers `$START` as an added (non-normalized, atomic) token. The tokenizers
        // crate assigns the next free id after the base vocab to the first added token
        // (verified against this crate version's `add_tokens` — `next_id` starts at
        // `model.get_vocab_size()`), landing it at exactly index 38167 for this vocab —
        // matching `bert_vocab_size: 38168` (ids 0..=38167) in the model's own config.
        tokenizer
            .add_special_tokens([AddedToken::from(START_TOKEN, true)])
            .map_err(|e| anyhow!("Failed to register {} token: {}", START_TOKEN, e))?;

        Ok(Self { tokenizer })
    }

    /// Encodes a list of real (whitespace-delimited) words into model inputs. Internally
    /// prepends `$START` — see the module doc comment for why this replaces the more
    /// obvious-looking `[CLS]`/`[SEP]` scheme — and computes `input_offsets` by
    /// replicating the reference `gec_model.py` logic: append the token index every time
    /// `word_ids()` changes value versus the previous token.
    ///
    /// Words must be passed as a pretokenized sequence (`is_split_into_words=True` in the
    /// reference) — joining and re-tokenizing would re-split on whitespace/punctuation and
    /// break the word↔offset alignment the ONNX model expects.
    pub fn encode_words(&self, words: &[String]) -> Result<CapuEncoding> {
        if words.is_empty() {
            return Err(anyhow!("Cannot encode empty word list"));
        }

        let mut word_refs: Vec<&str> = Vec::with_capacity(words.len() + 1);
        word_refs.push(START_TOKEN);
        word_refs.extend(words.iter().map(|s| s.as_str()));

        let encoding = self
            .tokenizer
            .encode(word_refs.as_slice(), false)
            .map_err(|e| anyhow!("Tokenization failed: {}", e))?;

        let word_ids = encoding.get_word_ids();
        let mut input_offsets: Vec<i64> = vec![0];
        for i in 1..word_ids.len() {
            if word_ids[i] != word_ids[i - 1] {
                input_offsets.push(i as i64);
            }
        }

        let expected_len = words.len() + 1;
        if input_offsets.len() != expected_len {
            return Err(anyhow!(
                "Offset alignment mismatch: got {} offsets for {} words (expected {})",
                input_offsets.len(),
                words.len(),
                expected_len
            ));
        }

        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        let attention_mask: Vec<i64> = vec![1; input_ids.len()];
        let token_type_ids: Vec<i64> = vec![0; input_ids.len()];

        Ok(CapuEncoding {
            input_ids,
            attention_mask,
            token_type_ids,
            input_offsets,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_test_vocab() -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        // [PAD]=0 [UNK]=1 [CLS]=2 [SEP]=3 [MASK]=4, then real (word, subword) tokens.
        // "vietnam" isn't whole-word in vocab, forcing a viet/##nam split so the test
        // exercises multi-subword offset alignment, not just the trivial 1:1 case.
        writeln!(
            file,
            "[PAD]\n[UNK]\n[CLS]\n[SEP]\n[MASK]\nxin\nchào\nviet\n##nam"
        )
        .unwrap();
        file
    }

    #[test]
    fn start_token_is_registered_right_after_the_base_vocab() {
        // Base vocab here has 9 lines (indices 0..8) -> $START must land at 9, matching
        // how the reference model's own embedding table appends exactly one extra row
        // after its base vocab (config.json: bert_vocab_size = vocab.txt lines + 1).
        let vocab = write_test_vocab();
        let tok = CapuTokenizer::from_vocab_file(vocab.path()).unwrap();
        assert_eq!(tok.tokenizer.token_to_id(START_TOKEN), Some(9));
    }

    #[test]
    fn offsets_align_one_per_word_including_multi_subword_word() {
        let vocab = write_test_vocab();
        let tok = CapuTokenizer::from_vocab_file(vocab.path()).unwrap();

        let words = vec!["xin".to_string(), "chào".to_string(), "vietnam".to_string()];
        let encoding = tok.encode_words(&words).unwrap();

        // $START xin chào viet ##nam -> word_ids [0,1,2,3,3], no CLS/SEP at all.
        // offsets computed by "append on word_id change": [0, 1, 2, 3]
        assert_eq!(encoding.input_offsets, vec![0, 1, 2, 3]);
        assert_eq!(encoding.input_offsets.len(), words.len() + 1);
        assert_eq!(encoding.input_ids[0], 9, "first token must be $START, not [CLS]");
        assert_eq!(encoding.attention_mask.len(), encoding.input_ids.len());
        assert_eq!(encoding.token_type_ids, vec![0; encoding.input_ids.len()]);
    }

    #[test]
    fn unknown_word_falls_back_to_unk_without_erroring() {
        let vocab = write_test_vocab();
        let tok = CapuTokenizer::from_vocab_file(vocab.path()).unwrap();

        let words = vec!["xin".to_string(), "gibberishword".to_string()];
        let encoding = tok.encode_words(&words).unwrap();

        assert_eq!(encoding.input_offsets.len(), words.len() + 1);
    }

    #[test]
    fn empty_words_list_returns_error_not_panic() {
        let vocab = write_test_vocab();
        let tok = CapuTokenizer::from_vocab_file(vocab.path()).unwrap();
        assert!(tok.encode_words(&[]).is_err());
    }
}
