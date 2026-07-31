use anyhow::{anyhow, Result};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Keep,
    TransformCaseCapital,
    AppendComma,
    AppendPeriod,
    TransformVerbVbVbn,
    TransformCaseUpper,
    AppendColon,
    AppendQuestion,
    TransformVerbVbVbc,
    TransformCaseLower,
    TransformCaseCapital1,
    TransformCaseUpperMinus1,
    MergeSpace,
    Unknown,
    Padding,
}

impl Action {
    pub fn from_label(label: &str) -> Action {
        match label {
            "$KEEP" => Action::Keep,
            "$TRANSFORM_CASE_CAPITAL" => Action::TransformCaseCapital,
            "$APPEND_," => Action::AppendComma,
            "$APPEND_." => Action::AppendPeriod,
            "$TRANSFORM_VERB_VB_VBN" => Action::TransformVerbVbVbn,
            "$TRANSFORM_CASE_UPPER" => Action::TransformCaseUpper,
            "$APPEND_:" => Action::AppendColon,
            "$APPEND_?" => Action::AppendQuestion,
            "$TRANSFORM_VERB_VB_VBC" => Action::TransformVerbVbVbc,
            "$TRANSFORM_CASE_LOWER" => Action::TransformCaseLower,
            "$TRANSFORM_CASE_CAPITAL_1" => Action::TransformCaseCapital1,
            "$TRANSFORM_CASE_UPPER_-1" => Action::TransformCaseUpperMinus1,
            "$MERGE_SPACE" => Action::MergeSpace,
            "@@PADDING@@" => Action::Padding,
            _ => Action::Unknown,
        }
    }
}

/// Loads a newline-separated label file (labels.txt or d_tags.txt) and maps each
/// line, in order, to an `Action` via `Action::from_label`. The returned Vec's index
/// corresponds exactly to the model's output class index for that head.
pub fn load_action_labels(path: &Path) -> Result<Vec<Action>> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("Failed to read label file {:?}: {}", path, e))?;
    let labels: Vec<Action> = content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(Action::from_label)
        .collect();
    if labels.is_empty() {
        return Err(anyhow!("Label file {:?} was empty", path));
    }
    Ok(labels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn loads_real_capu_label_order() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            file,
            "$KEEP\n$TRANSFORM_CASE_CAPITAL\n$APPEND_,\n$APPEND_.\n$TRANSFORM_VERB_VB_VBN\n$TRANSFORM_CASE_UPPER\n$APPEND_:\n$APPEND_?\n$TRANSFORM_VERB_VB_VBC\n$TRANSFORM_CASE_LOWER\n$TRANSFORM_CASE_CAPITAL_1\n$TRANSFORM_CASE_UPPER_-1\n$MERGE_SPACE\n@@UNKNOWN@@\n@@PADDING@@"
        )
        .unwrap();

        let labels = load_action_labels(file.path()).unwrap();

        assert_eq!(labels.len(), 15);
        assert_eq!(labels[0], Action::Keep);
        assert_eq!(labels[2], Action::AppendComma);
        assert_eq!(labels[10], Action::TransformCaseCapital1);
        assert_eq!(labels[11], Action::TransformCaseUpperMinus1);
        assert_eq!(labels[12], Action::MergeSpace);
        assert_eq!(labels[13], Action::Unknown);
        assert_eq!(labels[14], Action::Padding);
    }

    #[test]
    fn unknown_label_string_maps_to_unknown_action() {
        assert_eq!(Action::from_label("$SOMETHING_NEW"), Action::Unknown);
    }
}
