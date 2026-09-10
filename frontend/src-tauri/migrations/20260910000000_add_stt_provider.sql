ALTER TABLE transcript_settings ADD COLUMN liveProvider TEXT NOT NULL DEFAULT 'asr';
ALTER TABLE transcript_settings ADD COLUMN fileProvider TEXT NOT NULL DEFAULT 'asr';
ALTER TABLE transcript_settings ADD COLUMN geminiApiKey TEXT;
