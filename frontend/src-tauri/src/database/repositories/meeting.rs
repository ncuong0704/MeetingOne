use crate::api::{MeetingDetails, MeetingTranscript};
use crate::database::models::{MeetingModel, TranscriptWithSpeaker};
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, SqliteConnection, SqlitePool};
use tracing::{error, info, warn};

const TRANSCRIPT_WITH_SPEAKER_SQL: &str = r#"
    SELECT
        t.id, t.meeting_id, t.transcript, t.timestamp,
        t.summary, t.action_items, t.key_points,
        t.audio_start_time, t.audio_end_time, t.duration,
        t.speaker_id,
        s.display_name AS speaker_name,
        s.color AS speaker_color
    FROM transcripts t
    LEFT JOIN meeting_speakers s ON t.speaker_id = s.id
"#;

fn to_meeting_transcript(t: TranscriptWithSpeaker) -> MeetingTranscript {
    MeetingTranscript {
        id: t.id,
        text: t.transcript,
        timestamp: t.timestamp,
        audio_start_time: t.audio_start_time,
        audio_end_time: t.audio_end_time,
        duration: t.duration,
        speaker_id: t.speaker_id,
        speaker_name: t.speaker_name,
        speaker_color: t.speaker_color,
    }
}

pub struct MeetingsRepository;

impl MeetingsRepository {
    pub async fn get_meetings(pool: &SqlitePool) -> Result<Vec<MeetingModel>, sqlx::Error> {
        let meetings =
            sqlx::query_as::<_, MeetingModel>("SELECT * FROM meetings ORDER BY created_at DESC")
                .fetch_all(pool)
                .await?;
        Ok(meetings)
    }

    pub async fn delete_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<bool, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let folder_path: Option<Option<String>> =
            sqlx::query_scalar("SELECT folder_path FROM meetings WHERE id = ?")
                .bind(meeting_id)
                .fetch_optional(&mut *transaction)
                .await?;

        match delete_meeting_with_transaction(&mut transaction, meeting_id).await {
            Ok(success) => {
                if success {
                    transaction.commit().await?;
                    if let Some(path) = folder_path.flatten() {
                        remove_meeting_folder_best_effort(&path).await;
                    }
                    info!(
                        "Successfully deleted meeting {} and all associated data",
                        meeting_id
                    );
                    Ok(true)
                } else {
                    transaction.rollback().await?;
                    Ok(false)
                }
            }
            Err(e) => {
                let _ = transaction.rollback().await;
                error!("Failed to delete meeting {}: {}", meeting_id, e);
                Err(e)
            }
        }
    }

    pub async fn get_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<MeetingDetails>, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        // Get meeting details
        let meeting: Option<MeetingModel> =
            sqlx::query_as("SELECT id, title, created_at, updated_at, folder_path FROM meetings WHERE id = ?")
                .bind(meeting_id)
                .fetch_optional(&mut *transaction)
                .await?;

        if meeting.is_none() {
            transaction.rollback().await?;
            return Err(SqlxError::RowNotFound);
        }

        if let Some(meeting) = meeting {
            // Get all transcripts for this meeting (with optional speaker labels)
            let transcripts = sqlx::query_as::<_, TranscriptWithSpeaker>(&format!(
                "{TRANSCRIPT_WITH_SPEAKER_SQL} WHERE t.meeting_id = ? ORDER BY t.audio_start_time ASC"
            ))
            .bind(meeting_id)
            .fetch_all(&mut *transaction)
            .await?;

            transaction.commit().await?;

            let meeting_transcripts = transcripts
                .into_iter()
                .map(to_meeting_transcript)
                .collect::<Vec<_>>();

            Ok(Some(MeetingDetails {
                id: meeting.id,
                title: meeting.title,
                created_at: meeting.created_at.0.to_rfc3339(),
                updated_at: meeting.updated_at.0.to_rfc3339(),
                transcripts: meeting_transcripts,
            }))
        } else {
            transaction.rollback().await?;
            Ok(None)
        }
    }

    /// Get meeting metadata without transcripts (for pagination)
    pub async fn get_meeting_metadata(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<MeetingModel>, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let meeting: Option<MeetingModel> =
            sqlx::query_as("SELECT id, title, created_at, updated_at, folder_path FROM meetings WHERE id = ?")
                .bind(meeting_id)
                .fetch_optional(pool)
                .await?;

        Ok(meeting)
    }

    /// Get meeting transcripts with pagination support
    pub async fn get_meeting_transcripts_paginated(
        pool: &SqlitePool,
        meeting_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<TranscriptWithSpeaker>, i64), SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        // Get total count of transcripts for this meeting
        let total: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM transcripts WHERE meeting_id = ?"
        )
        .bind(meeting_id)
        .fetch_one(pool)
        .await?;

        let transcripts = sqlx::query_as::<_, TranscriptWithSpeaker>(&format!(
            "{TRANSCRIPT_WITH_SPEAKER_SQL}
             WHERE t.meeting_id = ?
             ORDER BY t.audio_start_time ASC
             LIMIT ? OFFSET ?"
        ))
        .bind(meeting_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok((transcripts, total.0))
    }

    pub async fn update_meeting_title(
        pool: &SqlitePool,
        meeting_id: &str,
        new_title: &str,
    ) -> Result<bool, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let now = Utc::now().naive_utc();

        let rows_affected =
            sqlx::query("UPDATE meetings SET title = ?, updated_at = ? WHERE id = ?")
                .bind(new_title)
                .bind(now)
                .bind(meeting_id)
                .execute(&mut *transaction)
                .await?;
        if rows_affected.rows_affected() == 0 {
            transaction.rollback().await?;
            return Ok(false);
        }
        transaction.commit().await?;
        Ok(true)
    }

    pub async fn update_meeting_name(
        pool: &SqlitePool,
        meeting_id: &str,
        new_title: &str,
    ) -> Result<bool, SqlxError> {
        let mut transaction = pool.begin().await?;
        let now = Utc::now();

        // Update meetings table
        let meeting_update =
            sqlx::query("UPDATE meetings SET title = ?, updated_at = ? WHERE id = ?")
                .bind(new_title)
                .bind(now)
                .bind(meeting_id)
                .execute(&mut *transaction)
                .await?;

        if meeting_update.rows_affected() == 0 {
            transaction.rollback().await?;
            return Ok(false); // Meeting not found
        }

        // Update transcript_chunks table
        sqlx::query("UPDATE transcript_chunks SET meeting_name = ? WHERE meeting_id = ?")
            .bind(new_title)
            .bind(meeting_id)
            .execute(&mut *transaction)
            .await?;

        transaction.commit().await?;
        Ok(true)
    }
}

async fn delete_meeting_with_transaction(
    transaction: &mut SqliteConnection,
    meeting_id: &str,
) -> Result<bool, SqlxError> {
    // Check if meeting exists
    let meeting_exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(&mut *transaction)
        .await?;

    if meeting_exists.is_none() {
        error!("Meeting {} not found for deletion", meeting_id);
        return Ok(false);
    }

    // Delete from related tables in proper order
    // 1. Delete from transcript_chunks
    sqlx::query("DELETE FROM transcript_chunks WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // 2. Delete from summary_processes
    sqlx::query("DELETE FROM summary_processes WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // 3. Delete from transcripts
    sqlx::query("DELETE FROM transcripts WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // 4. Delete from meeting_documents (no enforced FK cascade — SQLite foreign_keys
    // pragma is never enabled anywhere in this app, so the migration's
    // ON DELETE CASCADE clause is inert; must delete explicitly like the others above)
    sqlx::query("DELETE FROM meeting_documents WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // 5. Finally, delete the meeting
    let result = sqlx::query("DELETE FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    Ok(result.rows_affected() > 0)
}

async fn remove_meeting_folder_best_effort(folder_path: &str) {
    let trimmed = folder_path.trim();
    if trimmed.is_empty() {
        return;
    }
    let path = std::path::Path::new(trimmed);
    if !path.is_dir() {
        return;
    }
    if let Err(e) = tokio::fs::remove_dir_all(path).await {
        warn!(
            "Failed to delete meeting folder {}: {}",
            path.display(),
            e
        );
    } else {
        info!("Deleted meeting folder {}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::fs;

    /// Foreign keys stay off so the schema matches production
    /// (the app never enables `PRAGMA foreign_keys`).
    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("memory sqlite");

        sqlx::query(
            "CREATE TABLE meetings (
                id TEXT PRIMARY KEY,
                title TEXT,
                created_at TEXT,
                updated_at TEXT,
                folder_path TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE transcripts (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                transcript TEXT,
                timestamp TEXT,
                summary TEXT,
                action_items TEXT,
                key_points TEXT,
                audio_start_time REAL,
                audio_end_time REAL,
                duration REAL,
                speaker_id TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE transcript_chunks (
                meeting_id TEXT PRIMARY KEY,
                meeting_name TEXT,
                transcript_text TEXT NOT NULL,
                model TEXT NOT NULL,
                model_name TEXT NOT NULL,
                chunk_size INTEGER,
                overlap INTEGER,
                created_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE summary_processes (
                meeting_id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                error TEXT,
                result TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE meeting_documents (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                filename TEXT NOT NULL,
                extracted_text TEXT NOT NULL,
                char_count INTEGER NOT NULL,
                created_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE meeting_speakers (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                cluster_index INTEGER NOT NULL,
                display_name TEXT NOT NULL,
                color TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    async fn seed_meeting(pool: &SqlitePool, id: &str, title: &str, folder_path: Option<&str>) {
        sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at, folder_path)
             VALUES (?, ?, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', ?)",
        )
        .bind(id)
        .bind(title)
        .bind(folder_path)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp)
             VALUES (?, ?, 'xin chào', '')",
        )
        .bind(format!("t-{id}"))
        .bind(id)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO transcript_chunks
                (meeting_id, meeting_name, transcript_text, model, model_name, created_at)
             VALUES (?, ?, 'xin chào', 'zipformer', 'vi', '2026-01-01T00:00:00Z')",
        )
        .bind(id)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO summary_processes (meeting_id, status, created_at, updated_at)
             VALUES (?, 'idle', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO meeting_documents
                (id, meeting_id, filename, extracted_text, char_count, created_at)
             VALUES (?, ?, 'ghi-chu.txt', 'noi dung', 8, '2026-01-01T00:00:00Z')",
        )
        .bind(format!("d-{id}"))
        .bind(id)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO meeting_speakers (id, meeting_id, cluster_index, display_name, color)
             VALUES (?, ?, 0, 'Người nói 1', '#111')",
        )
        .bind(format!("s-{id}"))
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn count(pool: &SqlitePool, sql: &str) -> i64 {
        sqlx::query_as::<_, (i64,)>(sql)
            .fetch_one(pool)
            .await
            .unwrap()
            .0
    }

    fn write_meeting_folder() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "act-delete-meeting-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("audio.mp4"), b"fake-audio").unwrap();
        fs::write(dir.join("transcripts.json"), b"[]").unwrap();
        fs::write(dir.join("metadata.json"), b"{}").unwrap();
        dir
    }

    #[tokio::test]
    async fn delete_meeting_removes_sqlite_rows_for_that_meeting() {
        let pool = test_pool().await;
        seed_meeting(&pool, "m1", "Hop 1", None).await;

        let deleted = MeetingsRepository::delete_meeting(&pool, "m1")
            .await
            .expect("delete");
        assert!(deleted);

        assert_eq!(count(&pool, "SELECT COUNT(*) FROM meetings WHERE id = 'm1'").await, 0);
        assert_eq!(
            count(&pool, "SELECT COUNT(*) FROM transcripts WHERE meeting_id = 'm1'").await,
            0
        );
        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM transcript_chunks WHERE meeting_id = 'm1'"
            )
            .await,
            0
        );
        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM summary_processes WHERE meeting_id = 'm1'"
            )
            .await,
            0
        );
        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM meeting_documents WHERE meeting_id = 'm1'"
            )
            .await,
            0
        );
    }

    #[tokio::test]
    async fn delete_meeting_leaves_other_meetings_sqlite_data_intact() {
        let pool = test_pool().await;
        seed_meeting(&pool, "m1", "Hop 1", None).await;
        seed_meeting(&pool, "m2", "Hop 2", None).await;

        MeetingsRepository::delete_meeting(&pool, "m1")
            .await
            .expect("delete");

        assert_eq!(count(&pool, "SELECT COUNT(*) FROM meetings WHERE id = 'm2'").await, 1);
        assert_eq!(
            count(&pool, "SELECT COUNT(*) FROM transcripts WHERE meeting_id = 'm2'").await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM transcript_chunks WHERE meeting_id = 'm2'"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM summary_processes WHERE meeting_id = 'm2'"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM meeting_documents WHERE meeting_id = 'm2'"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM meeting_speakers WHERE meeting_id = 'm2'"
            )
            .await,
            1
        );

        let remaining = MeetingsRepository::get_meeting(&pool, "m2")
            .await
            .expect("get remaining")
            .expect("meeting exists");
        assert_eq!(remaining.id, "m2");
        assert_eq!(remaining.title, "Hop 2");
        assert_eq!(remaining.transcripts.len(), 1);

        let renamed = MeetingsRepository::update_meeting_title(&pool, "m2", "Hop 2 renamed")
            .await
            .expect("rename");
        assert!(renamed);
        let after_rename = MeetingsRepository::get_meeting_metadata(&pool, "m2")
            .await
            .expect("metadata")
            .expect("still exists");
        assert_eq!(after_rename.title, "Hop 2 renamed");
    }

    #[tokio::test]
    async fn delete_meeting_returns_false_when_missing() {
        let pool = test_pool().await;
        let deleted = MeetingsRepository::delete_meeting(&pool, "missing")
            .await
            .expect("delete missing");
        assert!(!deleted);
    }

    #[tokio::test]
    async fn delete_meeting_rejects_empty_id() {
        let pool = test_pool().await;
        let err = MeetingsRepository::delete_meeting(&pool, "  ")
            .await
            .expect_err("empty id");
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[tokio::test]
    async fn delete_meeting_removes_audio_transcript_metadata_on_disk() {
        let dir = write_meeting_folder();
        let audio = dir.join("audio.mp4");
        let transcripts = dir.join("transcripts.json");
        let metadata = dir.join("metadata.json");

        let pool = test_pool().await;
        seed_meeting(
            &pool,
            "m1",
            "Hop 1",
            Some(dir.to_string_lossy().as_ref()),
        )
        .await;

        MeetingsRepository::delete_meeting(&pool, "m1")
            .await
            .expect("delete");

        assert!(!dir.exists(), "meeting folder must be removed from disk");
        assert!(!audio.exists());
        assert!(!transcripts.exists());
        assert!(!metadata.exists());
    }

    #[tokio::test]
    async fn delete_meeting_does_not_remove_other_meeting_folder() {
        let dir1 = write_meeting_folder();
        let dir2 = write_meeting_folder();

        let pool = test_pool().await;
        seed_meeting(&pool, "m1", "Hop 1", Some(dir1.to_string_lossy().as_ref())).await;
        seed_meeting(&pool, "m2", "Hop 2", Some(dir2.to_string_lossy().as_ref())).await;

        MeetingsRepository::delete_meeting(&pool, "m1")
            .await
            .expect("delete");

        assert!(!dir1.exists());
        assert!(dir2.join("audio.mp4").exists());
        assert!(dir2.join("transcripts.json").exists());
        assert!(dir2.join("metadata.json").exists());

        let _ = fs::remove_dir_all(&dir2);
    }

    #[tokio::test]
    async fn delete_meeting_succeeds_when_folder_already_gone() {
        let missing = std::env::temp_dir().join(format!(
            "act-delete-meeting-missing-{}",
            uuid::Uuid::new_v4()
        ));
        assert!(!missing.exists());

        let pool = test_pool().await;
        seed_meeting(
            &pool,
            "m1",
            "Hop 1",
            Some(missing.to_string_lossy().as_ref()),
        )
        .await;

        let deleted = MeetingsRepository::delete_meeting(&pool, "m1")
            .await
            .expect("delete");
        assert!(deleted);
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM meetings WHERE id = 'm1'").await, 0);
    }
}
