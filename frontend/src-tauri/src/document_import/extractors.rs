use std::path::Path;

/// File extensions this module knows how to extract text from.
pub const SUPPORTED_EXTENSIONS: &[&str] = &["pdf", "docx", "txt", "srt", "vtt"];

/// Minimum number of characters (after trimming) a file must yield to be
/// considered "has real text content" rather than empty/scanned/corrupt.
const MIN_CONTENT_LENGTH: usize = 20;

fn extract_from_plain_text(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("Lỗi đọc file: {}", e))
}

fn extract_from_subtitle(path: &Path) -> Result<String, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("Lỗi đọc file: {}", e))?;

    let timestamp_re = regex::Regex::new(
        r"^\s*\d{2}:\d{2}:\d{2}[.,]\d{3}\s*-->\s*\d{2}:\d{2}:\d{2}[.,]\d{3}",
    )
    .expect("static regex is valid");
    let sequence_re = regex::Regex::new(r"^\s*\d+\s*$").expect("static regex is valid");

    let mut lines_out: Vec<String> = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "WEBVTT" {
            continue;
        }
        if timestamp_re.is_match(trimmed) || sequence_re.is_match(trimmed) {
            continue;
        }
        lines_out.push(trimmed.to_string());
    }
    Ok(lines_out.join("\n"))
}

fn extract_from_docx(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Lỗi đọc file: {}", e))?;
    let docx = docx_rs::read_docx(&bytes).map_err(|e| format!("Lỗi đọc DOCX: {}", e))?;

    let mut text = String::new();
    for child in docx.document.children {
        if let docx_rs::DocumentChild::Paragraph(paragraph) = child {
            let mut paragraph_text = String::new();
            for pchild in paragraph.children {
                if let docx_rs::ParagraphChild::Run(run) = pchild {
                    for rchild in run.children {
                        if let docx_rs::RunChild::Text(t) = rchild {
                            paragraph_text.push_str(&t.text);
                        }
                    }
                }
            }
            if !paragraph_text.is_empty() {
                text.push_str(&paragraph_text);
                text.push('\n');
            }
        }
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_from_plain_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.txt");
        std::fs::write(&path, "Nội dung cuộc họp mẫu").unwrap();

        let text = extract_from_plain_text(&path).unwrap();
        assert_eq!(text, "Nội dung cuộc họp mẫu");
    }

    #[test]
    fn test_extract_from_subtitle_strips_srt_markup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.srt");
        let srt = "1\n00:00:01,000 --> 00:00:04,000\nXin chào các bạn\n\n2\n00:00:04,500 --> 00:00:07,000\nChúng ta bắt đầu cuộc họp\n";
        std::fs::write(&path, srt).unwrap();

        let text = extract_from_subtitle(&path).unwrap();
        assert_eq!(text, "Xin chào các bạn\nChúng ta bắt đầu cuộc họp");
    }

    #[test]
    fn test_extract_from_subtitle_strips_vtt_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.vtt");
        let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nHello everyone\n";
        std::fs::write(&path, vtt).unwrap();

        let text = extract_from_subtitle(&path).unwrap();
        assert_eq!(text, "Hello everyone");
    }

    #[test]
    fn test_extract_from_docx_reads_paragraph_text() {
        use docx_rs::{Docx, Paragraph, Run};
        use std::io::Cursor;

        let mut buf: Vec<u8> = Vec::new();
        Docx::new()
            .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Hello world")))
            .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Second paragraph")))
            .build()
            .pack(Cursor::new(&mut buf))
            .expect("failed to pack test docx");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.docx");
        std::fs::write(&path, &buf).unwrap();

        let text = extract_from_docx(&path).unwrap();
        assert!(text.contains("Hello world"), "got: {}", text);
        assert!(text.contains("Second paragraph"), "got: {}", text);
    }
}
