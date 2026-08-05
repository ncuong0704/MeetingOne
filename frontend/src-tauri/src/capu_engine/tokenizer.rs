use anyhow::{anyhow, Result};
use std::path::Path;
use tokenizers::models::wordpiece::WordPiece;
use tokenizers::normalizers::bert::BertNormalizer;
use tokenizers::pre_tokenizers::bert::BertPreTokenizer;
use tokenizers::processors::bert::BertProcessing;
use tokenizers::{Model, Tokenizer};

/// Encoded representation ready to feed to the ONNX session: `input_offsets` here is
/// the *raw* offsets list (length == words.len() + 2, includes the CLS/SEP sentinel
/// positions) — callers are responsible for skipping the first/last entry when they
/// read per-word predictions back out (see capu_engine.rs).
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

        let cls_id = wordpiece
            .token_to_id("[CLS]")
            .ok_or_else(|| anyhow!("[CLS] not found in vocab {:?}", vocab_path))?;
        let sep_id = wordpiece
            .token_to_id("[SEP]")
            .ok_or_else(|| anyhow!("[SEP] not found in vocab {:?}", vocab_path))?;
        // Not used directly below, but `WordPieceBuilder::build()` does not validate that
        // `unk_token` exists in the vocab — it only surfaces as `MissingUnkToken` later,
        // inside `encode`, the first time a word actually needs UNK fallback. Since
        // `vocab.txt` is downloaded over HTTP (Task 7) and a truncated/corrupted download
        // is a real failure mode, validate `[UNK]` eagerly here too, matching CLS/SEP.
        wordpiece
            .token_to_id("[UNK]")
            .ok_or_else(|| anyhow!("[UNK] not found in vocab {:?}", vocab_path))?;

        let mut tokenizer = Tokenizer::new(wordpiece);
        tokenizer
            .with_normalizer(Some(BertNormalizer::new(true, true, Some(false), false)))
            .map_err(|e| anyhow!("Failed to set normalizer for {:?}: {}", vocab_path, e))?;
        tokenizer.with_pre_tokenizer(Some(BertPreTokenizer));
        tokenizer.with_post_processor(Some(BertProcessing::new(
            ("[SEP]".to_string(), sep_id),
            ("[CLS]".to_string(), cls_id),
        )));

        Ok(Self { tokenizer })
    }

    /// Encodes a list of whitespace-delimited words into model inputs, computing
    /// `input_offsets` by replicating the reference `gec_model.py` logic: append the
    /// token index every time `word_ids()` changes value versus the previous token.
    ///
    /// Words must be passed as a pretokenized sequence (`is_split_into_words=True` in the
    /// reference) — joining and re-tokenizing would re-split on whitespace/punctuation and
    /// break the word↔offset alignment the ONNX model expects.
    pub fn encode_words(&self, words: &[String]) -> Result<CapuEncoding> {
        if words.is_empty() {
            return Err(anyhow!("Cannot encode empty word list"));
        }

        let word_refs: Vec<&str> = words.iter().map(|s| s.as_str()).collect();
        let encoding = self
            .tokenizer
            .encode(word_refs.as_slice(), true)
            .map_err(|e| anyhow!("Tokenization failed: {}", e))?;

        let word_ids = encoding.get_word_ids();
        let mut input_offsets: Vec<i64> = vec![0];
        for i in 1..word_ids.len() {
            if word_ids[i] != word_ids[i - 1] {
                input_offsets.push(i as i64);
            }
        }

        let expected_len = words.len() + 2;
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
    fn offsets_align_one_per_word_including_multi_subword_word() {
        let vocab = write_test_vocab();
        let tok = CapuTokenizer::from_vocab_file(vocab.path()).unwrap();

        let words = vec!["xin".to_string(), "chào".to_string(), "vietnam".to_string()];
        let encoding = tok.encode_words(&words).unwrap();

        // [CLS] xin chào viet ##nam [SEP] -> word_ids [None,0,1,2,2,None]
        // offsets computed by "append on word_id change": [0, 1, 2, 3, 5]
        assert_eq!(encoding.input_offsets, vec![0, 1, 2, 3, 5]);
        assert_eq!(encoding.input_offsets.len(), words.len() + 2);
        assert_eq!(encoding.attention_mask.len(), encoding.input_ids.len());
        assert_eq!(encoding.token_type_ids, vec![0; encoding.input_ids.len()]);
    }

    #[test]
    fn unknown_word_falls_back_to_unk_without_erroring() {
        let vocab = write_test_vocab();
        let tok = CapuTokenizer::from_vocab_file(vocab.path()).unwrap();

        let words = vec!["xin".to_string(), "gibberishword".to_string()];
        let encoding = tok.encode_words(&words).unwrap();

        assert_eq!(encoding.input_offsets.len(), words.len() + 2);
    }

    #[test]
    fn empty_words_list_returns_error_not_panic() {
        let vocab = write_test_vocab();
        let tok = CapuTokenizer::from_vocab_file(vocab.path()).unwrap();
        assert!(tok.encode_words(&[]).is_err());
    }
}
