//! Meeting speakers (offline diarization) — rename / merge.

use crate::database::models::MeetingSpeakerWithPreview;
use sqlx::{Error as SqlxError, SqlitePool};

pub struct SpeakersRepository;

impl SpeakersRepository {
    pub async fn list_for_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<MeetingSpeakerWithPreview>, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol("meeting_id is required".into()));
        }
        sqlx::query_as::<_, MeetingSpeakerWithPreview>(
            r#"
            SELECT
                s.id,
                s.meeting_id,
                s.cluster_index,
                s.display_name,
                s.color,
                MIN(t.audio_start_time) AS preview_start
            FROM meeting_speakers s
            LEFT JOIN transcripts t ON t.speaker_id = s.id
            WHERE s.meeting_id = ?
            GROUP BY s.id
            ORDER BY s.cluster_index ASC, s.id ASC
            "#,
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
    }

    /// Reassign every transcript of `source_speaker_id` onto `target_speaker_id`, then delete source.
    pub async fn merge_into(
        pool: &SqlitePool,
        source_speaker_id: &str,
        target_speaker_id: &str,
    ) -> Result<bool, SqlxError> {
        let source = source_speaker_id.trim();
        let target = target_speaker_id.trim();
        if source.is_empty() || target.is_empty() {
            return Err(SqlxError::Protocol(
                "source and target speaker_id are required".into(),
            ));
        }
        if source == target {
            return Err(SqlxError::Protocol(
                "source and target must differ".into(),
            ));
        }

        let mut tx = pool.begin().await?;

        let source_row: Option<(String,)> =
            sqlx::query_as("SELECT meeting_id FROM meeting_speakers WHERE id = ?")
                .bind(source)
                .fetch_optional(&mut *tx)
                .await?;
        let target_row: Option<(String,)> =
            sqlx::query_as("SELECT meeting_id FROM meeting_speakers WHERE id = ?")
                .bind(target)
                .fetch_optional(&mut *tx)
                .await?;

        let (Some((source_meeting,)), Some((target_meeting,))) = (source_row, target_row) else {
            return Ok(false);
        };
        if source_meeting != target_meeting {
            return Err(SqlxError::Protocol(
                "speakers belong to different meetings".into(),
            ));
        }

        sqlx::query("UPDATE transcripts SET speaker_id = ? WHERE speaker_id = ?")
            .bind(target)
            .bind(source)
            .execute(&mut *tx)
            .await?;

        let deleted = sqlx::query("DELETE FROM meeting_speakers WHERE id = ?")
            .bind(source)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(deleted.rows_affected() > 0)
    }

    pub async fn rename_speaker(
        pool: &SqlitePool,
        speaker_id: &str,
        display_name: &str,
    ) -> Result<bool, SqlxError> {
        let name = display_name.trim();
        if speaker_id.trim().is_empty() || name.is_empty() {
            return Err(SqlxError::Protocol(
                "speaker_id and display_name are required".into(),
            ));
        }
        let result = sqlx::query("UPDATE meeting_speakers SET display_name = ? WHERE id = ?")
            .bind(name)
            .bind(speaker_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Assign this transcript the same speaker_id as the chronologically previous segment.
    pub async fn merge_with_previous(
        pool: &SqlitePool,
        transcript_id: &str,
    ) -> Result<bool, SqlxError> {
        if transcript_id.trim().is_empty() {
            return Err(SqlxError::Protocol("transcript_id is required".into()));
        }

        let row: Option<(String, Option<f64>, Option<String>)> = sqlx::query_as(
            "SELECT meeting_id, audio_start_time, speaker_id FROM transcripts WHERE id = ?",
        )
        .bind(transcript_id)
        .fetch_optional(pool)
        .await?;

        let Some((meeting_id, start_time, _)) = row else {
            return Ok(false);
        };

        let prev: Option<(Option<String>,)> = sqlx::query_as(
            r#"
            SELECT speaker_id FROM transcripts
            WHERE meeting_id = ?
              AND audio_start_time IS NOT NULL
              AND (? IS NULL OR audio_start_time < ?)
            ORDER BY audio_start_time DESC
            LIMIT 1
            "#,
        )
        .bind(&meeting_id)
        .bind(start_time)
        .bind(start_time)
        .fetch_optional(pool)
        .await?;

        let Some((Some(prev_speaker_id),)) = prev else {
            return Ok(false);
        };

        let result = sqlx::query("UPDATE transcripts SET speaker_id = ? WHERE id = ?")
            .bind(&prev_speaker_id)
            .bind(transcript_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::models::MeetingSpeakerWithPreview;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("memory sqlite");
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
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
            "CREATE TABLE meeting_speakers (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                cluster_index INTEGER NOT NULL,
                display_name TEXT NOT NULL,
                color TEXT NOT NULL,
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
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
                audio_start_time REAL,
                audio_end_time REAL,
                duration REAL,
                speaker_id TEXT REFERENCES meeting_speakers(id)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn seed_two_speakers(pool: &SqlitePool) {
        sqlx::query("INSERT INTO meetings (id, title) VALUES ('m1', 'Hop')")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO meeting_speakers (id, meeting_id, cluster_index, display_name, color)
             VALUES ('sa', 'm1', 0, 'Người nói 1', '#111'),
                    ('sb', 'm1', 1, 'Người nói 2', '#222')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, speaker_id)
             VALUES ('t1', 'm1', 'A sớm', '', 4.0, 'sa'),
                    ('t2', 'm1', 'B sớm', '', 2.0, 'sb'),
                    ('t3', 'm1', 'A muộn', '', 20.0, 'sa')",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn list_for_meeting_returns_preview_start_min() {
        let pool = test_pool().await;
        seed_two_speakers(&pool).await;
        let rows: Vec<MeetingSpeakerWithPreview> =
            SpeakersRepository::list_for_meeting(&pool, "m1")
                .await
                .expect("list");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "sa");
        assert_eq!(rows[0].preview_start, Some(4.0));
        assert_eq!(rows[1].id, "sb");
        assert_eq!(rows[1].preview_start, Some(2.0));
    }

    #[tokio::test]
    async fn merge_into_reassigns_all_segments_and_deletes_source() {
        let pool = test_pool().await;
        seed_two_speakers(&pool).await;
        let ok = SpeakersRepository::merge_into(&pool, "sb", "sa")
            .await
            .expect("merge");
        assert!(ok);

        let speakers = SpeakersRepository::list_for_meeting(&pool, "m1")
            .await
            .expect("list");
        assert_eq!(speakers.len(), 1);
        assert_eq!(speakers[0].id, "sa");
        assert_eq!(speakers[0].display_name, "Người nói 1");
        assert_eq!(speakers[0].preview_start, Some(2.0));

        let ids: Vec<(String, Option<String>)> =
            sqlx::query_as("SELECT id, speaker_id FROM transcripts ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            ids,
            vec![
                ("t1".into(), Some("sa".into())),
                ("t2".into(), Some("sa".into())),
                ("t3".into(), Some("sa".into())),
            ]
        );
    }

    #[tokio::test]
    async fn merge_into_rejects_same_id() {
        let pool = test_pool().await;
        seed_two_speakers(&pool).await;
        let err = SpeakersRepository::merge_into(&pool, "sa", "sa")
            .await
            .expect_err("same id");
        assert!(err.to_string().contains("source and target"));
    }

    #[tokio::test]
    async fn merge_with_previous_still_only_moves_one_transcript() {
        let pool = test_pool().await;
        seed_two_speakers(&pool).await;
        let ok = SpeakersRepository::merge_with_previous(&pool, "t1")
            .await
            .expect("merge previous");
        assert!(ok);

        let speakers = SpeakersRepository::list_for_meeting(&pool, "m1")
            .await
            .expect("list");
        assert_eq!(speakers.len(), 2);

        let t1: (Option<String>,) =
            sqlx::query_as("SELECT speaker_id FROM transcripts WHERE id = 't1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(t1.0.as_deref(), Some("sb"));
        let t3: (Option<String>,) =
            sqlx::query_as("SELECT speaker_id FROM transcripts WHERE id = 't3'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(t3.0.as_deref(), Some("sa"));
    }
}
