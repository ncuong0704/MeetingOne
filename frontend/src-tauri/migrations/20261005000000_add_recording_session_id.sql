ALTER TABLE meetings ADD COLUMN recording_session_id TEXT;
CREATE UNIQUE INDEX meetings_recording_session_id
    ON meetings(recording_session_id) WHERE recording_session_id IS NOT NULL;
