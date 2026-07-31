ALTER TABLE transcript_settings ADD COLUMN zipformerVariant TEXT NOT NULL DEFAULT 'int8';
ALTER TABLE transcript_settings ADD COLUMN decodingMethod TEXT NOT NULL DEFAULT 'modified_beam_search';
ALTER TABLE transcript_settings ADD COLUMN numActivePaths INTEGER NOT NULL DEFAULT 15;
