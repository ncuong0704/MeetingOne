use std::path::Path;

use super::transcript_parser;

/// File extensions this module knows how to extract text from (meeting document attachments).
pub const SUPPORTED_EXTENSIONS: &[&str] = &["pdf", "docx", "pptx", "txt", "srt", "vtt"];

/// Extensions allowed when creating a meeting from transcript files.
pub const TRANSCRIPT_IMPORT_EXTENSIONS: &[&str] = &["txt", "srt", "vtt"];

/// Minimum number of characters (after trimming) a file must yield to be
/// considered "has real text content" rather than empty/scanned/corrupt.
pub const MIN_CONTENT_LENGTH: usize = 20;

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

fn collect_run_text(run: &docx_rs::Run) -> String {
    let mut out = String::new();
    for child in &run.children {
        match child {
            docx_rs::RunChild::Text(t) => out.push_str(&t.text),
            docx_rs::RunChild::Tab(_) => out.push('\t'),
            _ => {}
        }
    }
    out
}

fn collect_paragraph_children(children: &[docx_rs::ParagraphChild]) -> String {
    let mut out = String::new();
    for child in children {
        match child {
            docx_rs::ParagraphChild::Run(run) => out.push_str(&collect_run_text(run)),
            docx_rs::ParagraphChild::Hyperlink(link) => {
                out.push_str(&collect_paragraph_children(&link.children));
            }
            _ => {}
        }
    }
    out
}

fn paragraph_to_markdown(paragraph: &docx_rs::Paragraph) -> Option<String> {
    let text = collect_paragraph_children(&paragraph.children);
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let style = paragraph
        .property
        .style
        .as_ref()
        .map(|s| s.val.as_str())
        .unwrap_or("");
    if let Some(prefix) = super::to_markdown::heading_prefix(style) {
        return Some(format!("{prefix}{text}"));
    }
    if paragraph.has_numbering || paragraph.property.numbering_property.is_some() {
        return Some(format!("- {text}"));
    }
    Some(text.to_string())
}

fn docx_table_to_markdown(table: &docx_rs::Table) -> String {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for row in &table.rows {
        let docx_rs::TableChild::TableRow(row) = row;
        let mut cells: Vec<String> = Vec::new();
        for cell in &row.cells {
            let docx_rs::TableRowChild::TableCell(cell) = cell;
            let mut parts: Vec<String> = Vec::new();
            for content in &cell.children {
                if let docx_rs::TableCellContent::Paragraph(paragraph) = content {
                    let t = collect_paragraph_children(&paragraph.children);
                    let t = t.trim();
                    if !t.is_empty() {
                        parts.push(t.to_string());
                    }
                }
            }
            cells.push(parts.join(" "));
        }
        if cells.iter().any(|c| !c.is_empty()) {
            rows.push(cells);
        }
    }
    super::to_markdown::to_markdown_table(&rows)
        .trim()
        .to_string()
}

fn extract_from_docx(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Lỗi đọc file: {}", e))?;
    let docx = docx_rs::read_docx(&bytes).map_err(|e| format!("Lỗi đọc DOCX: {}", e))?;

    let mut blocks: Vec<String> = Vec::new();
    for child in docx.document.children {
        match child {
            docx_rs::DocumentChild::Paragraph(paragraph) => {
                if let Some(line) = paragraph_to_markdown(&paragraph) {
                    blocks.push(line);
                }
            }
            docx_rs::DocumentChild::Table(table) => {
                let md = docx_table_to_markdown(&table);
                if !md.is_empty() {
                    blocks.push(md);
                }
            }
            _ => {}
        }
    }
    Ok(blocks.join("\n\n"))
}

fn extract_from_pdf(path: &Path) -> Result<String, String> {
    pdf_extract::extract_text(path).map_err(|e| format!("Lỗi đọc PDF: {}", e))
}

/// PPTX is a ZIP archive of per-slide XML files under `ppt/slides/slideN.xml`.
/// Converts each slide to Markdown (slide marker, title heading, tables).
fn extract_from_pptx(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Lỗi đọc file: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Lỗi đọc PPTX: {}", e))?;

    let slide_path_re =
        regex::Regex::new(r"^ppt/slides/slide(\d+)\.xml$").expect("static regex is valid");

    let mut slide_indices: Vec<(usize, usize)> = Vec::new();
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| format!("Lỗi đọc PPTX: {}", e))?;
        if let Some(captures) = slide_path_re.captures(entry.name()) {
            let slide_num: usize = captures[1].parse().unwrap_or(0);
            slide_indices.push((slide_num, i));
        }
    }
    slide_indices.sort_by_key(|(num, _)| *num);

    let mut slides: Vec<String> = Vec::new();
    for (slide_num, index) in slide_indices {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("Lỗi đọc PPTX: {}", e))?;
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut entry, &mut xml)
            .map_err(|e| format!("Lỗi đọc PPTX: {}", e))?;

        let body = super::to_markdown::pptx_slide_xml_to_markdown(&xml);
        if body.trim().is_empty() {
            continue;
        }
        slides.push(format!(
            "<!-- Slide number: {} -->\n\n{}",
            slide_num, body
        ));
    }

    Ok(slides.join("\n\n"))
}

fn extract_text(path: &Path) -> Result<String, String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    match extension.as_str() {
        "pdf" => extract_from_pdf(path),
        "docx" => extract_from_docx(path),
        "pptx" => extract_from_pptx(path),
        "srt" | "vtt" => extract_from_subtitle(path),
        "txt" => extract_from_plain_text(path),
        other => Err(format!("Định dạng .{} không được hỗ trợ", other)),
    }
}

/// Parse a file into transcript segments, preserving timestamps when present.
pub fn parse_file_segments(path: &Path) -> Result<Vec<transcript_parser::ParsedSegment>, String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    let text = match extension.as_str() {
        "txt" | "srt" | "vtt" => {
            std::fs::read_to_string(path).map_err(|e| format!("Lỗi đọc file: {}", e))?
        }
        other => {
            return Err(format!(
                "Định dạng .{} không được hỗ trợ. Chỉ chấp nhận TXT, SRT hoặc VTT",
                other
            ));
        }
    };

    let trimmed = text.trim();
    if trimmed.chars().count() < MIN_CONTENT_LENGTH {
        return Err(
            "Không trích xuất được nội dung văn bản (file rỗng hoặc quá ngắn)".to_string(),
        );
    }

    Ok(transcript_parser::parse_document_content(trimmed, &extension))
}

/// Extract text from `path` and validate it has real content.
/// Returns a trimmed, non-empty string, or an error describing why the file
/// was rejected (unsupported format, read/parse failure, or too little text —
/// e.g. a scanned/image-only PDF with no extractable text layer).
pub fn extract_text_validated(path: &Path) -> Result<String, String> {
    let text = extract_text(path)?;
    let trimmed = text.trim();
    if trimmed.chars().count() < MIN_CONTENT_LENGTH {
        return Err(
            "Không trích xuất được nội dung văn bản (có thể là file PDF dạng scan/ảnh, hoặc file rỗng)"
                .to_string(),
        );
    }
    Ok(trimmed.to_string())
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

    #[test]
    fn test_extract_from_docx_heading_and_table_as_markdown() {
        use docx_rs::{Docx, Paragraph, Run, Table, TableCell, TableRow};
        use std::io::Cursor;

        let mut buf: Vec<u8> = Vec::new();
        Docx::new()
            .add_paragraph(
                Paragraph::new()
                    .style("Heading1")
                    .add_run(Run::new().add_text("Agenda")),
            )
            .add_table(Table::new(vec![
                TableRow::new(vec![
                    TableCell::new().add_paragraph(
                        Paragraph::new().add_run(Run::new().add_text("Mục")),
                    ),
                    TableCell::new().add_paragraph(
                        Paragraph::new().add_run(Run::new().add_text("Chủ trì")),
                    ),
                ]),
                TableRow::new(vec![
                    TableCell::new().add_paragraph(
                        Paragraph::new().add_run(Run::new().add_text("Khai mạc")),
                    ),
                    TableCell::new().add_paragraph(
                        Paragraph::new().add_run(Run::new().add_text("Lan")),
                    ),
                ]),
            ]))
            .build()
            .pack(Cursor::new(&mut buf))
            .expect("failed to pack test docx");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("structured.docx");
        std::fs::write(&path, &buf).unwrap();

        let text = extract_from_docx(&path).unwrap();
        assert!(text.contains("# Agenda"), "got: {}", text);
        assert!(text.contains("| Mục | Chủ trì |"), "got: {}", text);
        assert!(text.contains("| Khai mạc | Lan |"), "got: {}", text);
    }

    #[test]
    fn test_extract_from_pdf_invalid_bytes_returns_err() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.pdf");
        std::fs::write(&path, b"this is not a real pdf file").unwrap();

        let result = extract_from_pdf(&path);
        assert!(result.is_err(), "expected an error for a non-PDF file");
    }

    #[test]
    fn test_extract_from_pptx_reads_slide_text_in_order() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.pptx");

        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();

        // Written out of order on purpose — extraction must sort by slide number.
        zip.start_file("ppt/slides/slide2.xml", options).unwrap();
        zip.write_all(b"<p:sld><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Second slide</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>").unwrap();

        zip.start_file("ppt/slides/slide1.xml", options).unwrap();
        zip.write_all(b"<p:sld><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>First slide</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>").unwrap();

        zip.finish().unwrap();

        let text = extract_from_pptx(&path).unwrap();
        let first_pos = text.find("First slide").expect("First slide text missing");
        let second_pos = text.find("Second slide").expect("Second slide text missing");
        assert!(
            first_pos < second_pos,
            "slides should be ordered by slide number, got: {}",
            text
        );
    }

    #[test]
    fn test_extract_from_pptx_decodes_xml_entities() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entities.pptx");

        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();

        zip.start_file("ppt/slides/slide1.xml", options).unwrap();
        zip.write_all(b"<a:t>Q&amp;A session</a:t>").unwrap();
        zip.finish().unwrap();

        let text = extract_from_pptx(&path).unwrap();
        assert!(text.contains("Q&A session"), "got: {}", text);
    }

    #[test]
    fn test_extract_from_pptx_reads_attributed_text_tags() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("attributed.pptx");

        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();

        zip.start_file("ppt/slides/slide1.xml", options).unwrap();
        zip.write_all(b"<a:r><a:t xml:space=\"preserve\">preserved text</a:t></a:r>")
            .unwrap();
        zip.finish().unwrap();

        let text = extract_from_pptx(&path).unwrap();
        assert!(text.contains("preserved text"), "got: {}", text);
    }

    #[test]
    fn test_extract_from_pptx_does_not_double_unescape_nested_entities() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested_entities.pptx");

        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();

        // The correctly-escaped representation of the literal text "&lt;div&gt;".
        zip.start_file("ppt/slides/slide1.xml", options).unwrap();
        zip.write_all(b"<a:t>&amp;lt;div&amp;gt;</a:t>").unwrap();
        zip.finish().unwrap();

        let text = extract_from_pptx(&path).unwrap();
        assert!(
            text.contains("&lt;div&gt;"),
            "should decode to literal '&lt;div&gt;', not double-unescape to '<div>'; got: {}",
            text
        );
    }

    #[test]
    fn test_extract_from_pptx_emits_slide_number_comment() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("numbered.pptx");

        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();

        zip.start_file("ppt/slides/slide1.xml", options).unwrap();
        zip.write_all(b"<p:sld><a:t>Opening remarks for the meeting today</a:t></p:sld>")
            .unwrap();
        zip.finish().unwrap();

        let text = extract_from_pptx(&path).unwrap();
        assert!(
            text.starts_with("<!-- Slide number: 1 -->"),
            "got: {}",
            text
        );
        assert!(
            text.contains("Opening remarks for the meeting today"),
            "got: {}",
            text
        );
    }

    #[test]
    fn test_extract_from_pptx_table_as_markdown() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("table.pptx");

        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();

        zip.start_file("ppt/slides/slide1.xml", options).unwrap();
        zip.write_all(
            br#"<p:sld>
            <a:tbl>
              <a:tr><a:tc><a:t>KPI</a:t></a:tc><a:tc><a:t>Q1</a:t></a:tc></a:tr>
              <a:tr><a:tc><a:t>Doanh thu</a:t></a:tc><a:tc><a:t>10</a:t></a:tc></a:tr>
            </a:tbl>
        </p:sld>"#,
        )
        .unwrap();
        zip.finish().unwrap();

        let text = extract_from_pptx(&path).unwrap();
        assert!(text.contains("<!-- Slide number: 1 -->"), "got: {}", text);
        assert!(text.contains("| KPI | Q1 |"), "got: {}", text);
        assert!(text.contains("| Doanh thu | 10 |"), "got: {}", text);
    }

    #[test]
    fn test_extract_text_dispatches_by_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.txt");
        std::fs::write(&path, "Nội dung mẫu").unwrap();

        let text = extract_text(&path).unwrap();
        assert_eq!(text, "Nội dung mẫu");
    }

    #[test]
    fn test_extract_text_unsupported_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.xyz");
        std::fs::write(&path, "some content").unwrap();

        let result = extract_text(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_text_validated_rejects_short_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("short.txt");
        std::fs::write(&path, "hi").unwrap();

        let result = extract_text_validated(&path);
        assert!(result.is_err(), "content shorter than MIN_CONTENT_LENGTH should be rejected");
    }

    #[test]
    fn test_extract_text_validated_accepts_real_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("real.txt");
        std::fs::write(&path, "Đây là nội dung cuộc họp có đủ độ dài để vượt qua ngưỡng kiểm tra").unwrap();

        let result = extract_text_validated(&path);
        assert!(result.is_ok());
    }
}
