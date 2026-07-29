use crate::database::models::{DateTimeUtc, MeetingDocument};
use chrono::Utc;
use sqlx::{Error as SqlxError, SqlitePool};
use tracing::info;
use uuid::Uuid;

pub struct MeetingDocumentsRepository;

impl MeetingDocumentsRepository {
    /// Extracts and stores a new reference document attached to a meeting.
    pub async fn create(
        pool: &SqlitePool,
        meeting_id: &str,
        filename: &str,
        extracted_text: &str,
    ) -> Result<MeetingDocument, SqlxError> {
        let id = format!("meeting-document-{}", Uuid::new_v4());
        let now = Utc::now();
        let char_count = extracted_text.chars().count() as i64;

        sqlx::query(
            "INSERT INTO meeting_documents (id, meeting_id, filename, extracted_text, char_count, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(meeting_id)
        .bind(filename)
        .bind(extracted_text)
        .bind(char_count)
        .bind(now)
        .execute(pool)
        .await?;

        info!("Attached document '{}' to meeting {}", filename, meeting_id);

        Ok(MeetingDocument {
            id,
            meeting_id: meeting_id.to_string(),
            filename: filename.to_string(),
            extracted_text: extracted_text.to_string(),
            char_count,
            created_at: DateTimeUtc(now),
        })
    }

    /// Lists all documents attached to a meeting, oldest first.
    pub async fn list_by_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<MeetingDocument>, SqlxError> {
        sqlx::query_as::<_, MeetingDocument>(
            "SELECT id, meeting_id, filename, extracted_text, char_count, created_at
             FROM meeting_documents WHERE meeting_id = ? ORDER BY created_at ASC",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
    }

    /// Deletes a document by id. Returns true if a row was actually deleted.
    pub async fn delete(pool: &SqlitePool, document_id: &str) -> Result<bool, SqlxError> {
        let result = sqlx::query("DELETE FROM meeting_documents WHERE id = ?")
            .bind(document_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
