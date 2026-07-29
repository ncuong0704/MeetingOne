CREATE TABLE IF NOT EXISTS meeting_documents (
    id TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL,
    filename TEXT NOT NULL,
    extracted_text TEXT NOT NULL,
    char_count INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_meeting_documents_meeting_id ON meeting_documents(meeting_id);
