use once_cell::sync::Lazy;
use regex::Regex;

static PPTX_SHAPE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<p:sp\b[^>]*>.*?</p:sp>").expect("static regex is valid"));
static PPTX_TABLE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<a:tbl\b[^>]*>.*?</a:tbl>").expect("static regex is valid"));
static PPTX_ROW_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<a:tr\b[^>]*>.*?</a:tr>").expect("static regex is valid"));
static PPTX_CELL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<a:tc\b[^>]*>.*?</a:tc>").expect("static regex is valid"));
static PPTX_PARA_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<a:p\b[^>]*>.*?</a:p>").expect("static regex is valid"));
// `<a:t>` must not also match `<a:tc>` / `<a:tbl>` (`[^>]*` would swallow the extra letters).
static TEXT_RUN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"<a:t(?:\s[^>]*)?>(.*?)</a:t>").expect("static regex is valid")
});

pub(super) fn decode_xml_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Map Word paragraph style ids (Heading1 / "Heading 1" / Title) to a Markdown
/// ATX prefix. Returns None for body styles.
pub(super) fn heading_prefix(style: &str) -> Option<&'static str> {
    let normalized: String = style
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    match normalized.as_str() {
        "heading1" | "title" => Some("# "),
        "heading2" => Some("## "),
        "heading3" => Some("### "),
        "heading4" => Some("#### "),
        "heading5" => Some("##### "),
        "heading6" => Some("###### "),
        _ => None,
    }
}

pub(super) fn escape_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ").trim().to_string()
}

/// Convert a rectangular row list into a GitHub-flavored Markdown table.
/// First row is the header. Empty input yields an empty string.
pub(super) fn to_markdown_table(rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if cols == 0 {
        return String::new();
    }

    let pad = |row: &[String]| -> Vec<String> {
        let mut padded: Vec<String> = row.iter().map(|c| escape_cell(c)).collect();
        padded.resize(cols, String::new());
        padded
    };

    let mut out = String::new();
    let header = pad(&rows[0]);
    out.push('|');
    for cell in &header {
        out.push(' ');
        out.push_str(cell);
        out.push_str(" |");
    }
    out.push('\n');
    out.push('|');
    for _ in 0..cols {
        out.push_str(" --- |");
    }
    out.push('\n');
    for row in rows.iter().skip(1) {
        let cells = pad(row);
        out.push('|');
        for cell in &cells {
            out.push(' ');
            out.push_str(&cell);
            out.push_str(" |");
        }
        out.push('\n');
    }
    out
}

fn collect_text_runs(xml: &str) -> String {
    TEXT_RUN_RE
        .captures_iter(xml)
        .map(|cap| decode_xml_entities(&cap[1]))
        .collect::<Vec<_>>()
        .join("")
}

fn paragraphs_from_xml(xml: &str) -> Vec<String> {
    PPTX_PARA_RE
        .find_iter(xml)
        .map(|m| collect_text_runs(m.as_str()).trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

fn pptx_table_rows(tbl_xml: &str) -> Vec<Vec<String>> {
    PPTX_ROW_RE
        .find_iter(tbl_xml)
        .map(|row| {
            PPTX_CELL_RE
                .find_iter(row.as_str())
                .map(|cell| collect_text_runs(cell.as_str()).trim().to_string())
                .collect()
        })
        .filter(|row: &Vec<String>| row.iter().any(|c| !c.is_empty()))
        .collect()
}

fn is_title_shape(shape_xml: &str) -> bool {
    shape_xml.contains("type=\"title\"") || shape_xml.contains("type=\"ctrTitle\"")
}

/// Convert one PPTX slide's XML into Markdown (title heading, body lines, tables).
/// Minimal fixtures that only contain `<a:t>` runs fall back to space-joined text.
pub(super) fn pptx_slide_xml_to_markdown(xml: &str) -> String {
    let mut pieces: Vec<(usize, String)> = Vec::new();

    for sp in PPTX_SHAPE_RE.find_iter(xml) {
        let paras = paragraphs_from_xml(sp.as_str());
        if paras.is_empty() {
            continue;
        }
        let markdown = if is_title_shape(sp.as_str()) {
            format!("# {}", paras.join(" "))
        } else {
            paras.join("\n")
        };
        pieces.push((sp.start(), markdown));
    }

    for tbl in PPTX_TABLE_RE.find_iter(xml) {
        let md = to_markdown_table(&pptx_table_rows(tbl.as_str()));
        let trimmed = md.trim().to_string();
        if !trimmed.is_empty() {
            pieces.push((tbl.start(), trimmed));
        }
    }

    if pieces.is_empty() {
        let runs: Vec<String> = TEXT_RUN_RE
            .captures_iter(xml)
            .map(|cap| decode_xml_entities(&cap[1]))
            .filter(|s| !s.is_empty())
            .collect();
        return runs.join(" ");
    }

    pieces.sort_by_key(|(start, _)| *start);
    pieces
        .into_iter()
        .map(|(_, md)| md)
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_prefix_maps_word_styles() {
        assert_eq!(heading_prefix("Heading1"), Some("# "));
        assert_eq!(heading_prefix("Heading 2"), Some("## "));
        assert_eq!(heading_prefix("Title"), Some("# "));
        assert_eq!(heading_prefix("Normal"), None);
    }

    #[test]
    fn to_markdown_table_formats_header_and_rows() {
        let md = to_markdown_table(&[
            vec!["Col A".into(), "Col B".into()],
            vec!["1".into(), "2".into()],
        ]);
        assert!(md.starts_with("| Col A | Col B |\n| --- | --- |\n"));
        assert!(md.contains("| 1 | 2 |"));
    }

    #[test]
    fn to_markdown_table_empty_is_empty_string() {
        assert_eq!(to_markdown_table(&[]), "");
    }

    #[test]
    fn to_markdown_table_escapes_pipes() {
        let md = to_markdown_table(&[vec!["a|b".into(), "c".into()]]);
        assert!(md.contains("a\\|b"));
    }

    #[test]
    fn pptx_slide_fallback_joins_bare_text_runs() {
        let md = pptx_slide_xml_to_markdown("<a:t>Q&amp;A session</a:t>");
        assert_eq!(md, "Q&A session");
    }

    #[test]
    fn pptx_slide_title_and_table_become_markdown() {
        let xml = r#"
        <p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
          <p:txBody><a:p><a:r><a:t>Agenda</a:t></a:r></a:p></p:txBody>
        </p:sp>
        <p:sp><p:txBody><a:p><a:r><a:t>Intro</a:t></a:r></a:p></p:txBody></p:sp>
        <a:tbl>
          <a:tr><a:tc><a:t>H1</a:t></a:tc><a:tc><a:t>H2</a:t></a:tc></a:tr>
          <a:tr><a:tc><a:t>A</a:t></a:tc><a:tc><a:t>B</a:t></a:tc></a:tr>
        </a:tbl>
        "#;
        let md = pptx_slide_xml_to_markdown(xml);
        assert!(md.contains("# Agenda"), "got: {md}");
        assert!(md.contains("Intro"), "got: {md}");
        assert!(md.contains("| H1 | H2 |"), "got: {md}");
        assert!(md.contains("| A | B |"), "got: {md}");
    }

    #[test]
    fn pptx_slide_does_not_double_unescape_nested_entities() {
        let md = pptx_slide_xml_to_markdown("<a:t>&amp;lt;div&amp;gt;</a:t>");
        assert_eq!(md, "&lt;div&gt;");
    }
}
