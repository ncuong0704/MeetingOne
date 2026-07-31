use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSegment {
    pub text: String,
    pub start_seconds: Option<f64>,
    pub end_seconds: Option<f64>,
}

fn subtitle_ts_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(\d{1,2}:\d{2}(?::\d{2})?(?:[.,]\d{1,3})?)\s*-->\s*(\d{1,2}:\d{2}(?::\d{2})?(?:[.,]\d{1,3})?)",
        )
        .expect("subtitle timestamp regex is valid")
    })
}

fn sequence_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d+$").expect("sequence regex is valid"))
}

fn line_range_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^\s*(\d{1,2}:\d{2}(?::\d{2})?(?:[.,]\d{1,3})?)\s*-->\s*(\d{1,2}:\d{2}(?::\d{2})?(?:[.,]\d{1,3})?)\s*(.*)$",
        )
        .expect("line range regex is valid")
    })
}

fn line_ts_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^\s*(?:[\[(])?(\d{1,2}):(\d{2})(?::(\d{2}))?(?:[.,](\d{1,3}))?(?:[\])])?\s*(?:[-–—:]\s*)?(.*)$",
        )
        .expect("line timestamp regex is valid")
    })
}

/// Parse a time token like `01:23`, `00:01:23`, `01:23.500`, or `00:01:23,000`.
fn parse_time_token(token: &str) -> Option<f64> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }

    let (main, millis) = match token.find([',', '.']) {
        Some(idx) => (&token[..idx], Some(&token[idx + 1..])),
        None => (token, None),
    };

    let parts: Vec<u32> = main
        .split(':')
        .filter_map(|part| part.trim().parse().ok())
        .collect();

    let base_seconds = match parts.len() {
        3 => Some(parts[0] as f64 * 3600.0 + parts[1] as f64 * 60.0 + parts[2] as f64),
        2 => Some(parts[0] as f64 * 60.0 + parts[1] as f64),
        _ => None,
    }?;

    let millis_fraction = millis
        .and_then(|ms| {
            let digits = ms.chars().take(3).collect::<String>();
            let padded = format!("{:0<3}", digits);
            padded.parse::<f64>().ok()
        })
        .unwrap_or(0.0)
        / 1000.0;

    Some(base_seconds + millis_fraction)
}

fn parse_hms_capture(
    hours: Option<&str>,
    minutes: &str,
    seconds: &str,
    millis: Option<&str>,
) -> Option<f64> {
    let h: u32 = hours.unwrap_or("0").parse().ok()?;
    let m: u32 = minutes.parse().ok()?;
    let s: u32 = seconds.parse().ok()?;
    let mut total = h as f64 * 3600.0 + m as f64 * 60.0 + s as f64;

    if let Some(ms) = millis {
        let digits = ms.chars().take(3).collect::<String>();
        let padded = format!("{:0<3}", digits);
        if let Ok(value) = padded.parse::<f64>() {
            total += value / 1000.0;
        }
    }

    Some(total)
}

fn push_segment(
    segments: &mut Vec<ParsedSegment>,
    text_lines: &[String],
    start_seconds: Option<f64>,
    end_seconds: Option<f64>,
) {
    let text = text_lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if text.is_empty() {
        return;
    }

    segments.push(ParsedSegment {
        text,
        start_seconds,
        end_seconds,
    });
}

/// Parse SRT/VTT subtitle content into timestamped segments.
pub fn parse_subtitle(content: &str) -> Vec<ParsedSegment> {
    let mut segments = Vec::new();
    let mut current_start: Option<f64> = None;
    let mut current_end: Option<f64> = None;
    let mut current_text: Vec<String> = Vec::new();

    let mut flush = |segments: &mut Vec<ParsedSegment>,
                     current_start: &mut Option<f64>,
                     current_end: &mut Option<f64>,
                     current_text: &mut Vec<String>| {
        if current_start.is_some() || current_end.is_some() {
            push_segment(
                segments,
                current_text,
                *current_start,
                *current_end,
            );
        }
        current_start.take();
        current_end.take();
        current_text.clear();
    };

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty()
            || trimmed == "WEBVTT"
            || trimmed.starts_with("NOTE")
            || trimmed.starts_with("STYLE")
            || trimmed.starts_with("REGION")
        {
            flush(
                &mut segments,
                &mut current_start,
                &mut current_end,
                &mut current_text,
            );
            continue;
        }

        if sequence_re().is_match(trimmed) {
            continue;
        }

        if let Some(caps) = subtitle_ts_re().captures(trimmed) {
            flush(
                &mut segments,
                &mut current_start,
                &mut current_end,
                &mut current_text,
            );
            current_start = parse_time_token(caps.get(1).map(|m| m.as_str()).unwrap_or_default());
            current_end = parse_time_token(caps.get(2).map(|m| m.as_str()).unwrap_or_default());
            continue;
        }

        if current_start.is_some() || current_end.is_some() {
            current_text.push(trimmed.to_string());
        }
    }

    flush(
        &mut segments,
        &mut current_start,
        &mut current_end,
        &mut current_text,
    );

    finalize_segments(segments)
}

/// Parse plain text where each line may start with a timestamp marker.
pub fn parse_timestamped_lines(content: &str) -> Vec<ParsedSegment> {
    let mut segments = Vec::new();
    let mut current_start: Option<f64> = None;
    let mut current_end: Option<f64> = None;
    let mut current_text: Vec<String> = Vec::new();

    let mut flush = |segments: &mut Vec<ParsedSegment>,
                     current_start: &mut Option<f64>,
                     current_end: &mut Option<f64>,
                     current_text: &mut Vec<String>| {
        if current_start.is_some() {
            push_segment(
                segments,
                current_text,
                *current_start,
                *current_end,
            );
        }
        current_start.take();
        current_end.take();
        current_text.clear();
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush(
                &mut segments,
                &mut current_start,
                &mut current_end,
                &mut current_text,
            );
            continue;
        }

        if let Some(caps) = line_range_re().captures(trimmed) {
            flush(
                &mut segments,
                &mut current_start,
                &mut current_end,
                &mut current_text,
            );
            current_start = parse_time_token(caps.get(1).map(|m| m.as_str()).unwrap_or_default());
            current_end = parse_time_token(caps.get(2).map(|m| m.as_str()).unwrap_or_default());
            let text = caps.get(3).map(|m| m.as_str()).unwrap_or("").trim();
            if !text.is_empty() {
                current_text.push(text.to_string());
            }
            continue;
        }

        if let Some(caps) = line_ts_re().captures(trimmed) {
            let part1 = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let part2 = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
            let part3 = caps.get(3).map(|m| m.as_str());
            let millis = caps.get(4).map(|m| m.as_str());
            let text = caps.get(5).map(|m| m.as_str()).unwrap_or("").trim();

            let start = if let Some(seconds_part) = part3 {
                parse_hms_capture(Some(part1), part2, seconds_part, millis)
            } else {
                parse_hms_capture(None, part1, part2, millis)
            };

            if let Some(start) = start {
                flush(
                    &mut segments,
                    &mut current_start,
                    &mut current_end,
                    &mut current_text,
                );
                current_start = Some(start);
                if !text.is_empty() {
                    current_text.push(text.to_string());
                }
                continue;
            }
        }

        if current_start.is_some() {
            current_text.push(trimmed.to_string());
        }
    }

    flush(
        &mut segments,
        &mut current_start,
        &mut current_end,
        &mut current_text,
    );

    finalize_segments(segments)
}

fn finalize_segments(mut segments: Vec<ParsedSegment>) -> Vec<ParsedSegment> {
    for i in 0..segments.len() {
        if segments[i].end_seconds.is_none() {
            if let Some(start) = segments[i].start_seconds {
                let end = if i + 1 < segments.len() {
                    segments[i + 1].start_seconds.unwrap_or(start + 5.0)
                } else {
                    start + 5.0
                };
                segments[i].end_seconds = Some(end);
            }
        }
    }
    segments
}

/// Parse document text into transcript segments, preserving timestamps when present.
pub fn parse_document_content(content: &str, extension: &str) -> Vec<ParsedSegment> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut segments = match extension {
        "srt" | "vtt" => parse_subtitle(trimmed),
        "txt" => {
            if trimmed.contains("-->") {
                parse_subtitle(trimmed)
            } else {
                parse_timestamped_lines(trimmed)
            }
        }
        _ => Vec::new(),
    };

    let has_timestamps = segments.iter().any(|segment| segment.start_seconds.is_some());
    if has_timestamps {
        return segments;
    }

    vec![ParsedSegment {
        text: trimmed.to_string(),
        start_seconds: None,
        end_seconds: None,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_subtitle_srt_format() {
        let srt = "1\n00:00:01,000 --> 00:00:04,000\nXin chào các bạn\n\n2\n00:00:04,500 --> 00:00:07,000\nChúng ta bắt đầu cuộc họp\n";
        let segments = parse_subtitle(srt);

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Xin chào các bạn");
        assert_eq!(segments[0].start_seconds, Some(1.0));
        assert_eq!(segments[0].end_seconds, Some(4.0));
        assert_eq!(segments[1].start_seconds, Some(4.5));
        assert_eq!(segments[1].end_seconds, Some(7.0));
    }

    #[test]
    fn parse_subtitle_vtt_format() {
        let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nHello everyone\n";
        let segments = parse_subtitle(vtt);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Hello everyone");
        assert_eq!(segments[0].start_seconds, Some(1.0));
        assert_eq!(segments[0].end_seconds, Some(4.0));
    }

    #[test]
    fn parse_timestamped_lines_bracket_mm_ss() {
        let content = "[00:05] Xin chào các bạn\n[00:12] Hôm nay chúng ta họp về dự án\n";
        let segments = parse_timestamped_lines(content);

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Xin chào các bạn");
        assert_eq!(segments[0].start_seconds, Some(5.0));
        assert_eq!(segments[1].start_seconds, Some(12.0));
    }

    #[test]
    fn parse_timestamped_lines_hh_mm_ss_without_brackets() {
        let content = "00:01:23 - Bắt đầu cuộc họp\n00:02:45 Kết luận phần một\n";
        let segments = parse_timestamped_lines(content);

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Bắt đầu cuộc họp");
        assert_eq!(segments[0].start_seconds, Some(83.0));
        assert_eq!(segments[1].text, "Kết luận phần một");
        assert_eq!(segments[1].start_seconds, Some(165.0));
    }

    #[test]
    fn parse_timestamped_lines_mm_ss_only() {
        let content = "1:05 Nội dung đầu tiên\n2:30 Nội dung tiếp theo\n";
        let segments = parse_timestamped_lines(content);

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].start_seconds, Some(65.0));
        assert_eq!(segments[1].start_seconds, Some(150.0));
    }

    #[test]
    fn parse_timestamped_lines_multiline_segment() {
        let content = "[00:05] Dòng đầu\nDòng tiếp theo cùng mốc\n[00:20] Đoạn mới\n";
        let segments = parse_timestamped_lines(content);

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Dòng đầu Dòng tiếp theo cùng mốc");
        assert_eq!(segments[1].text, "Đoạn mới");
    }

    #[test]
    fn parse_document_content_plain_text_fallback() {
        let content = "Đây là nội dung cuộc họp không có mốc thời gian nào cả.";
        let segments = parse_document_content(content, "txt");

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, content);
        assert!(segments[0].start_seconds.is_none());
    }

    #[test]
    fn parse_document_content_txt_with_timestamps() {
        let content = "[01:00] Mở đầu\n[02:15] Thảo luận\n";
        let segments = parse_document_content(content, "txt");

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].start_seconds, Some(60.0));
        assert_eq!(segments[1].start_seconds, Some(135.0));
    }
}
