ALTER TABLE transcript_settings ADD COLUMN diarizationEnabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE transcript_settings ADD COLUMN diarizationNumSpeakers INTEGER;
