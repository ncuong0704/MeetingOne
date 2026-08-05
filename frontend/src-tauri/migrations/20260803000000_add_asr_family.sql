UPDATE transcript_settings SET provider = 'asr' WHERE provider = 'zipformer';
ALTER TABLE transcript_settings RENAME COLUMN zipformerVariant TO asrVariant;
