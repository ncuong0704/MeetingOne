use crate::api::{TranscriptSearchResult, TranscriptSegment};
use crate::audio::transcription::live_speaker::color_for_name;
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, SqlitePool};
use std::collections::HashMap;
use tracing::{error, info};
use uuid::Uuid;

/// First-seen unique live speaker names (empty/whitespace skipped).
pub(crate) fn unique_speaker_names<'a, I>(names: I) -> Vec<String>
where
    I: IntoIterator<Item = Option<&'a str>>,
{
    let mut out = Vec::new();
    for name in names {
        let Some(trimmed) = name.map(str::trim).filter(|n| !n.is_empty()) else {
            continue;
        };
        if !out.iter().any(|n| n == trimmed) {
            out.push(trimmed.to_string());
        }
    }
    out
}

pub struct TranscriptsRepository;

impl TranscriptsRepository {
    /// Saves a new meeting and its associated transcript segments.
    /// This function uses a transaction to ensure that either both the meeting
    /// and all its transcripts are saved, or none of them are.
    pub async fn save_transcript(
        pool: &SqlitePool,
        meeting_title: &str,
        transcripts: &[TranscriptSegment],
        folder_path: Option<String>,
    ) -> Result<String, SqlxError> {
        let meeting_id = format!("meeting-{}", Uuid::new_v4());

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let now = Utc::now();

        // 1. Create the new meeting
        let result = sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&meeting_id)
        .bind(meeting_title)
        .bind(now)
        .bind(now)
        .bind(&folder_path)
        .execute(&mut *transaction)
        .await;

        if let Err(e) = result {
            error!("Failed to create meeting '{}': {}", meeting_title, e);
            transaction.rollback().await?;
            return Err(e);
        }

        info!("Successfully created meeting with id: {}", meeting_id);

        let speaker_names = unique_speaker_names(
            transcripts
                .iter()
                .map(|s| s.speaker_name.as_deref()),
        );
        let mut name_to_id: HashMap<String, String> = HashMap::new();
        for (cluster_index, name) in speaker_names.iter().enumerate() {
            let speaker_id = format!("speaker-{}", Uuid::new_v4());
            let color = color_for_name(name);
            let result = sqlx::query(
                "INSERT INTO meeting_speakers (id, meeting_id, cluster_index, display_name, color)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&speaker_id)
            .bind(&meeting_id)
            .bind(cluster_index as i32)
            .bind(name)
            .bind(&color)
            .execute(&mut *transaction)
            .await;

            if let Err(e) = result {
                error!(
                    "Failed to insert live speaker '{}' for meeting {}: {}",
                    name, meeting_id, e
                );
                transaction.rollback().await?;
                return Err(e);
            }
            name_to_id.insert(name.clone(), speaker_id);
        }

        // 2. Save each transcript segment with audio timing fields
        for segment in transcripts {
            let transcript_id = format!("transcript-{}", Uuid::new_v4());
            let speaker_id = segment.speaker_name.as_deref().and_then(|n| {
                let trimmed = n.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    name_to_id.get(trimmed).cloned()
                }
            });
            let result = sqlx::query(
                "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&transcript_id)
            .bind(&meeting_id)
            .bind(&segment.text)
            .bind(&segment.timestamp)
            .bind(segment.audio_start_time)
            .bind(segment.audio_end_time)
            .bind(segment.duration)
            .bind(speaker_id)
            .execute(&mut *transaction)
            .await;

            if let Err(e) = result {
                error!(
                    "Failed to save transcript segment for meeting {}: {}",
                    meeting_id, e
                );
                transaction.rollback().await?;
                return Err(e);
            }
        }

        info!(
            "Successfully saved {} transcript segments for meeting {}",
            transcripts.len(),
            meeting_id
        );

        // Commit the transaction
        transaction.commit().await?;

        Ok(meeting_id)
    }

    /// Searches for a query string within the transcripts.
    /// It returns a list of matching transcripts with context.
    pub async fn search_transcripts(
        pool: &SqlitePool,
        query: &str,
    ) -> Result<Vec<TranscriptSearchResult>, SqlxError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let search_query = format!("%{}%", query.to_lowercase());

        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT m.id, m.title, t.transcript, t.timestamp
             FROM meetings m
             JOIN transcripts t ON m.id = t.meeting_id
             WHERE LOWER(t.transcript) LIKE ?",
        )
        .bind(&search_query)
        .fetch_all(pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|(id, title, transcript, timestamp)| {
                let match_context = Self::get_match_context(&transcript, query);
                TranscriptSearchResult {
                    id,
                    title,
                    match_context,
                    timestamp,
                }
            })
            .collect();

        Ok(results)
    }

    /// Update the text of a single transcript segment.
    pub async fn update_transcript_text(
        pool: &SqlitePool,
        transcript_id: &str,
        new_text: &str,
    ) -> Result<(), SqlxError> {
        sqlx::query("UPDATE transcripts SET transcript = ? WHERE id = ?")
            .bind(new_text)
            .bind(transcript_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Helper function to extract a snippet of text around the first match of a query.
    fn get_match_context(transcript: &str, query: &str) -> String {
        let transcript_lower = transcript.to_lowercase();
        let query_lower = query.to_lowercase();

        match transcript_lower.find(&query_lower) {
            Some(match_index) => {
                let start_index = match_index.saturating_sub(100);
                let end_index = (match_index + query.len() + 100).min(transcript.len());

                let mut context = String::new();
                if start_index > 0 {
                    context.push_str("...");
                }
                context.push_str(&transcript[start_index..end_index]);
                if end_index < transcript.len() {
                    context.push_str("...");
                }
                context
            }
            None => transcript.chars().take(200).collect(), // Fallback to the start of the transcript
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_speaker_names_skips_empty_and_dedupes_in_order() {
        let names = [
            None,
            Some("  "),
            Some("Lan"),
            Some(" Minh "),
            Some("Lan"),
            Some(""),
            Some("Minh"),
        ];
        assert_eq!(
            unique_speaker_names(names),
            vec!["Lan".to_string(), "Minh".to_string()]
        );
    }

    #[test]
    fn unique_speaker_names_all_blank_is_empty() {
        assert!(unique_speaker_names([None, Some(""), Some("  ")]).is_empty());
    }
}
