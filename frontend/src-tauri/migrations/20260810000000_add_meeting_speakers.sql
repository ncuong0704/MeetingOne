-- Migration: Add meeting_speakers table for offline speaker diarization
-- Each row is one detected speaker cluster within a meeting, with a user-editable
-- display name and a palette color. Transcript segments are attributed to a speaker
-- via the new transcripts.speaker_id column.

CREATE TABLE IF NOT EXISTS meeting_speakers (
    id TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL,
    cluster_index INTEGER NOT NULL,
    display_name TEXT NOT NULL,
    color TEXT NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_meeting_speakers_meeting_id ON meeting_speakers(meeting_id);

ALTER TABLE transcripts ADD COLUMN speaker_id TEXT REFERENCES meeting_speakers(id);
