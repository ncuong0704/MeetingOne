use crate::api::TranscriptSegment;

#[derive(Debug, Clone, PartialEq)]
pub enum LiveTranscriptEvent {
    Interim { text: String },
    Final { text: String },
    GoAway,
    SetupComplete,
    Resumption { handle: String },
    Ignored,
}

pub fn parse_live_message(value: &serde_json::Value) -> LiveTranscriptEvent {
    if value.get("setupComplete").is_some() {
        return LiveTranscriptEvent::SetupComplete;
    }
    if value.get("goAway").is_some() {
        return LiveTranscriptEvent::GoAway;
    }
    if let Some(handle) = value
        .pointer("/sessionResumptionUpdate/newHandle")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        return LiveTranscriptEvent::Resumption {
            handle: handle.to_string(),
        };
    }
    let server = value.get("serverContent");
    if let Some(text) = server
        .and_then(|s| s.pointer("/inputTranscription/text"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return LiveTranscriptEvent::Final {
            text: text.to_string(),
        };
    }
    if let Some(text) = server
        .and_then(|s| s.pointer("/interimInputTranscription/text"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return LiveTranscriptEvent::Interim {
            text: text.to_string(),
        };
    }
    LiveTranscriptEvent::Ignored
}

pub fn segments_from_file_output(
    output_text: &str,
    timed: &[(String, f64, f64)],
    duration_seconds: f64,
) -> Vec<TranscriptSegment> {
    if !timed.is_empty() {
        return timed
            .iter()
            .enumerate()
            .filter(|(_, (t, _, _))| !t.trim().is_empty())
            .map(|(i, (text, start, end))| TranscriptSegment {
                id: format!("seg_{i}"),
                text: text.clone(),
                timestamp: format_hms(*start),
                audio_start_time: Some(*start),
                audio_end_time: Some(*end),
                duration: Some((end - start).max(0.0)),
                speaker_cluster: None,
                speaker_name: None,
            })
            .collect();
    }
    let text = output_text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    vec![TranscriptSegment {
        id: "seg_0".to_string(),
        text: text.to_string(),
        timestamp: "00:00:00".to_string(),
        audio_start_time: Some(0.0),
        audio_end_time: Some(duration_seconds.max(0.0)),
        duration: Some(duration_seconds.max(0.0)),
        speaker_cluster: None,
        speaker_name: None,
    }]
}

fn format_hms(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!(
        "{:02}:{:02}:{:02}",
        total / 3600,
        (total % 3600) / 60,
        total % 60
    )
}

pub fn extract_interaction_text(body: &serde_json::Value) -> String {
    if let Some(t) = body.get("output_text").and_then(|v| v.as_str()) {
        return t.to_string();
    }
    body.pointer("/outputs/0/text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_interim_then_final() {
        let interim = serde_json::json!({
            "serverContent": {
                "interimInputTranscription": {
                    "text": "xin chào"
                }
            }
        });
        assert_eq!(
            parse_live_message(&interim),
            LiveTranscriptEvent::Interim {
                text: "xin chào".to_string()
            }
        );

        let final_msg = serde_json::json!({
            "serverContent": {
                "inputTranscription": {
                    "text": "Xin chào."
                }
            }
        });
        assert_eq!(
            parse_live_message(&final_msg),
            LiveTranscriptEvent::Final {
                text: "Xin chào.".to_string()
            }
        );
    }

    #[test]
    fn live_goaway_and_setup() {
        let go_away = serde_json::json!({ "goAway": {} });
        assert_eq!(parse_live_message(&go_away), LiveTranscriptEvent::GoAway);

        let setup = serde_json::json!({ "setupComplete": {} });
        assert_eq!(
            parse_live_message(&setup),
            LiveTranscriptEvent::SetupComplete
        );
    }

    #[test]
    fn file_untimed_is_single_span() {
        let segs = segments_from_file_output("Họp xong.", &[], 12.5);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "Họp xong.");
        assert_eq!(segs[0].audio_end_time, Some(12.5));
    }

    #[test]
    fn file_timed_maps_each_utterance() {
        let timed = vec![
            ("A".to_string(), 0.0, 1.2),
            ("B".to_string(), 1.2, 3.0),
        ];
        let segs = segments_from_file_output("", &timed, 3.0);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].id, "seg_0");
        assert_eq!(segs[1].id, "seg_1");
        assert_eq!(segs[1].audio_start_time, Some(1.2));
    }
}
