use docx_rs::{
    Paragraph, Run, Table, TableCell, TableRow, WidthType, Docx, AlignmentType,
};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
enum MarkdownBlock {
    Heading { level: u8, text: String },
    Paragraph(String),
    Bullet(String),
    Numbered(String),
    Table { rows: Vec<Vec<String>> },
}

fn normalize_line_endings(markdown: &str) -> String {
    markdown.replace("\r\n", "\n")
}

fn parse_markdown_blocks(markdown: &str) -> Vec<MarkdownBlock> {
    let normalized = normalize_line_endings(markdown);
    let lines: Vec<&str> = normalized.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        let line = lines[i].trim();

        if line.is_empty() {
            i += 1;
            continue;
        }

        if line.starts_with('|') {
            let mut table_rows: Vec<Vec<String>> = Vec::new();
            while i < lines.len() {
                let table_line = lines[i].trim();
                if !table_line.starts_with('|') {
                    break;
                }
                if table_line
                    .chars()
                    .all(|c| c == '|' || c == '-' || c == ' ' || c == ':')
                {
                    i += 1;
                    continue;
                }
                let cells = table_line
                    .trim_matches('|')
                    .split('|')
                    .map(|s| s.trim().to_string())
                    .collect::<Vec<String>>();
                if !cells.is_empty() {
                    table_rows.push(cells);
                }
                i += 1;
            }
            if !table_rows.is_empty() {
                blocks.push(MarkdownBlock::Table { rows: table_rows });
            }
            continue;
        }

        if let Some(stripped) = line.strip_prefix("# ") {
            blocks.push(MarkdownBlock::Heading {
                level: 1,
                text: stripped.trim().to_string(),
            });
            i += 1;
            continue;
        }
        if let Some(stripped) = line.strip_prefix("## ") {
            blocks.push(MarkdownBlock::Heading {
                level: 2,
                text: stripped.trim().to_string(),
            });
            i += 1;
            continue;
        }
        if let Some(stripped) = line.strip_prefix("### ") {
            blocks.push(MarkdownBlock::Heading {
                level: 3,
                text: stripped.trim().to_string(),
            });
            i += 1;
            continue;
        }

        if let Some(stripped) = line.strip_prefix("- ") {
            blocks.push(MarkdownBlock::Bullet(stripped.trim().to_string()));
            i += 1;
            continue;
        }

        let numbered = line
            .find(". ")
            .and_then(|dot_idx| line[..dot_idx].parse::<u32>().ok().map(|_| dot_idx));
        if let Some(dot_idx) = numbered {
            blocks.push(MarkdownBlock::Numbered(line[dot_idx + 2..].trim().to_string()));
            i += 1;
            continue;
        }

        let mut paragraph_lines = vec![line.to_string()];
        i += 1;
        while i < lines.len() {
            let candidate = lines[i].trim();
            if candidate.is_empty()
                || candidate.starts_with('#')
                || candidate.starts_with('|')
                || candidate.starts_with("- ")
            {
                break;
            }
            let numbered_candidate = candidate
                .find(". ")
                .and_then(|dot_idx| candidate[..dot_idx].parse::<u32>().ok().map(|_| dot_idx));
            if numbered_candidate.is_some() {
                break;
            }
            paragraph_lines.push(candidate.to_string());
            i += 1;
        }
        blocks.push(MarkdownBlock::Paragraph(paragraph_lines.join(" ")));
    }

    blocks
}

pub fn sanitize_file_stem(title: &str) -> String {
    let trimmed = title.trim();
    let raw = if trimmed.is_empty() {
        "meeting-summary"
    } else {
        trimmed
    };
    let cleaned: String = raw
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => c,
        })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join("_");
    if collapsed.is_empty() {
        "meeting-summary".to_string()
    } else {
        collapsed
    }
}

pub fn export_markdown_to_docx(markdown: &str, destination: &Path) -> Result<(), String> {
    let blocks = parse_markdown_blocks(markdown);
    let mut doc = Docx::new();

    for block in blocks {
        match block {
            MarkdownBlock::Heading { level, text } => {
                let size_half_points = match level {
                    1 => 56usize,
                    2 => 42usize,
                    _ => 34usize,
                };
                let p = Paragraph::new()
                    .align(AlignmentType::Left)
                    .add_run(Run::new().add_text(text).size(size_half_points).bold());
                doc = doc.add_paragraph(p);
            }
            MarkdownBlock::Paragraph(text) => {
                doc = doc.add_paragraph(Paragraph::new().add_run(Run::new().add_text(text)));
            }
            MarkdownBlock::Bullet(text) => {
                doc = doc.add_paragraph(
                    Paragraph::new().add_run(Run::new().add_text(format!("• {}", text))),
                );
            }
            MarkdownBlock::Numbered(text) => {
                doc = doc.add_paragraph(
                    Paragraph::new().add_run(Run::new().add_text(format!("1. {}", text))),
                );
            }
            MarkdownBlock::Table { rows } => {
                let mut table_rows = Vec::new();
                for row in rows {
                    let cells = row
                        .into_iter()
                        .map(|cell| {
                            TableCell::new()
                                .add_paragraph(Paragraph::new().add_run(Run::new().add_text(cell)))
                        })
                        .collect::<Vec<TableCell>>();
                    table_rows.push(TableRow::new(cells));
                }

                let mut table = Table::new(table_rows);
                table = table.width(5000, WidthType::Pct);
                doc = doc.add_table(table);
            }
        }
    }

    let file = fs::File::create(destination)
        .map_err(|e| format!("Không thể tạo file DOCX: {}", e))?;
    doc.build()
        .pack(file)
        .map_err(|e| format!("Không thể ghi nội dung DOCX: {}", e))
}

pub fn ensure_extension(path: &Path, extension: &str) -> PathBuf {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case(extension))
        .unwrap_or(false)
    {
        return path.to_path_buf();
    }
    path.with_extension(extension)
}
