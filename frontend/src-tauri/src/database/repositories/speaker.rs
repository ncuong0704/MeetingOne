//! Meeting speakers (offline diarization) — rename / merge.

use sqlx::{Error as SqlxError, SqlitePool};

pub struct SpeakersRepository;

impl SpeakersRepository {
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
