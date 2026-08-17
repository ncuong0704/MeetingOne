//! Persistent speaker directory (name, title, department) for autocomplete.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, Runtime};

const DIRECTORY_FILE: &str = "speaker_directory.json";
const DEFAULTS_FILE: &str = "nguoi-noi.json";
const DEFAULTS_SUBDIR: &str = "mac-dinh";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DirectorySpeaker {
    #[serde(default)]
    pub id: String,
    #[serde(alias = "hoTen")]
    pub full_name: String,
    #[serde(default, alias = "chucVu")]
    pub title: String,
    #[serde(default, alias = "phongBan")]
    pub department: String,
}

fn directory_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(dir.join(DIRECTORY_FILE))
}

fn normalize_people(raw: Vec<DirectorySpeaker>) -> Vec<DirectorySpeaker> {
    let mut out = Vec::new();
    for person in raw {
        let full_name = person.full_name.trim().to_string();
        if full_name.is_empty() {
            continue;
        }
        let id = person.id.trim().to_string();
        let id = if id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            id
        };
        out.push(DirectorySpeaker {
            id,
            full_name,
            title: person.title.trim().to_string(),
            department: person.department.trim().to_string(),
        });
    }
    out
}

fn read_directory_file(path: &PathBuf) -> Vec<DirectorySpeaker> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<DirectorySpeaker>>(&raw) {
        Ok(people) => normalize_people(people),
        Err(_) => Vec::new(),
    }
}

fn defaults_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    if let Ok(resource) = app.path().resource_dir() {
        for path in [
            resource.join(DEFAULTS_SUBDIR).join(DEFAULTS_FILE),
            resource.join("resources").join(DEFAULTS_SUBDIR).join(DEFAULTS_FILE),
            resource.join(DEFAULTS_FILE),
        ] {
            if path.exists() {
                return Some(path);
            }
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(DEFAULTS_SUBDIR)
        .join(DEFAULTS_FILE);
    if dev.exists() {
        Some(dev)
    } else {
        None
    }
}

fn persist_people(path: &PathBuf, people: &[DirectorySpeaker]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(people).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_speaker_directory<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Vec<DirectorySpeaker>, String> {
    let path = directory_path(&app)?;
    let saved = read_directory_file(&path);
    if !saved.is_empty() {
        return Ok(saved);
    }
    let Some(defaults) = defaults_path(&app) else {
        return Ok(saved);
    };
    let seeded = read_directory_file(&defaults);
    if seeded.is_empty() {
        return Ok(saved);
    }
    persist_people(&path, &seeded)?;
    Ok(seeded)
}

#[tauri::command]
pub fn save_speaker_directory<R: Runtime>(
    app: AppHandle<R>,
    people: Vec<DirectorySpeaker>,
) -> Result<Vec<DirectorySpeaker>, String> {
    let path = directory_path(&app)?;
    let normalized = normalize_people(people);
    persist_people(&path, &normalized)?;
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_drops_blank_names_and_fills_missing_ids() {
        let raw = vec![
            DirectorySpeaker {
                id: " a ".into(),
                full_name: "  Lan  ".into(),
                title: " CV ".into(),
                department: " KT ".into(),
            },
            DirectorySpeaker {
                id: "".into(),
                full_name: "Minh".into(),
                title: String::new(),
                department: String::new(),
            },
            DirectorySpeaker {
                id: "b".into(),
                full_name: "   ".into(),
                title: "x".into(),
                department: "y".into(),
            },
        ];
        let out = normalize_people(raw);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].id, "a");
        assert_eq!(out[0].full_name, "Lan");
        assert_eq!(out[0].title, "CV");
        assert_eq!(out[0].department, "KT");
        assert!(!out[1].id.is_empty());
        assert_eq!(out[1].full_name, "Minh");
    }

    #[test]
    fn camel_case_json_roundtrip() {
        let json = r#"[{"id":"1","fullName":"Nguyễn Văn A","title":"TP","department":"KH"}]"#;
        let people: Vec<DirectorySpeaker> = serde_json::from_str(json).unwrap();
        assert_eq!(people[0].full_name, "Nguyễn Văn A");
        let encoded = serde_json::to_string(&people).unwrap();
        assert!(encoded.contains("fullName"));
    }

    #[test]
    fn defaults_json_accepts_vietnamese_keys_and_missing_id() {
        let json = r#"[{"hoTen":"Nguyễn Cường","chucVu":"Chuyên viên AI","phongBan":"Phòng CNTT"}]"#;
        let people: Vec<DirectorySpeaker> = serde_json::from_str(json).unwrap();
        let out = normalize_people(people);
        assert_eq!(out.len(), 1);
        assert!(!out[0].id.is_empty());
        assert_eq!(out[0].full_name, "Nguyễn Cường");
        assert_eq!(out[0].title, "Chuyên viên AI");
        assert_eq!(out[0].department, "Phòng CNTT");
    }

    #[test]
    fn defaults_json_skips_blank_names() {
        let json = r#"[{"hoTen":"","chucVu":"x","phongBan":"y"},{"fullName":"Minh"}]"#;
        let people: Vec<DirectorySpeaker> = serde_json::from_str(json).unwrap();
        let out = normalize_people(people);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].full_name, "Minh");
        assert!(!out[0].id.is_empty());
    }

    #[test]
    fn bundled_nguoi_noi_json_keeps_named_rows_and_drops_blank_templates() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(DEFAULTS_SUBDIR)
            .join(DEFAULTS_FILE);
        let people = read_directory_file(&path);
        assert_eq!(people.len(), 5);
        assert_eq!(people[0].full_name, "Phạm Tuấn Anh");
        assert_eq!(people[0].title, "Tổng Giám đốc");
        assert_eq!(people[0].department, "Ban Điều hành");
        assert_eq!(people[4].full_name, "Phạm Văn Kiên");
        assert_eq!(people[4].title, "Trưởng phòng KHCT");
        assert!(people.iter().all(|p| !p.id.is_empty()));
    }
}
