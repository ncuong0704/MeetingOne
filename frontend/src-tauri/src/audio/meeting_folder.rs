use std::path::{Path, PathBuf};

/// Destructive recovery operations only accept folders created by this app.
pub(crate) fn validate_meeting_folder(folder: &Path) -> Result<PathBuf, String> {
    let folder = folder
        .canonicalize()
        .map_err(|e| format!("Invalid meeting folder: {e}"))?;
    if !folder.is_dir() || folder.parent().is_none() {
        return Err("Expected a meeting directory".into());
    }
    let metadata: serde_json::Value = serde_json::from_slice(
        &std::fs::read(folder.join("metadata.json"))
            .map_err(|e| format!("Missing meeting metadata: {e}"))?,
    )
    .map_err(|e| format!("Invalid meeting metadata: {e}"))?;
    if metadata["version"] != "1.0"
        || metadata["transcript_file"] != "transcripts.json"
        || !metadata["meeting_name"].is_string()
    {
        return Err("Folder is not a recognized meeting recording".into());
    }
    Ok(folder)
}

pub(crate) fn validate_checkpoint_directory(folder: &Path) -> Result<PathBuf, String> {
    let folder = validate_meeting_folder(folder)?;
    let checkpoints = folder.join(".checkpoints");
    if checkpoints.exists() {
        let resolved = checkpoints.canonicalize().map_err(|e| e.to_string())?;
        if resolved.parent() != Some(folder.as_path()) || !resolved.is_dir() {
            return Err("Checkpoint directory escapes the meeting folder".into());
        }
    }
    Ok(checkpoints)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unrelated_directory_and_accepts_meeting_metadata() {
        let temp = tempfile::tempdir().unwrap();
        assert!(validate_meeting_folder(temp.path()).is_err());
        std::fs::write(
            temp.path().join("metadata.json"),
            r#"{"version":"1.0","meeting_name":"Test","transcript_file":"transcripts.json"}"#,
        )
        .unwrap();
        assert_eq!(
            validate_meeting_folder(temp.path()).unwrap(),
            temp.path().canonicalize().unwrap()
        );
    }
}
