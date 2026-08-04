ALTER TABLE transcript_settings ADD COLUMN capuCpuThreads INTEGER;
ALTER TABLE transcript_settings ADD COLUMN capuPunctuationLevel INTEGER NOT NULL DEFAULT 7;
ALTER TABLE transcript_settings ADD COLUMN capuCaseLevel INTEGER NOT NULL DEFAULT 3;
