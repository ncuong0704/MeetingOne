//! Live speaker assignment via number keys 1–9 (mirrors test ASR, without
//! injecting `__SPK_SEP__` tokens into ASR text).

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter, Manager, Runtime};

const HOTKEYS_FILE: &str = "speaker_hotkeys.json";

pub const SPEAKER_COLORS: [&str; 8] = [
    "#2563EB", "#DC2626", "#16A34A", "#CA8A04", "#9333EA", "#DB2777", "#0891B2",
    "#EA580C",
];

pub fn color_for_name(name: &str) -> String {
    let mut h: u32 = 0;
    for b in name.as_bytes() {
        h = h.wrapping_mul(31).wrapping_add(*b as u32);
    }
    SPEAKER_COLORS[(h as usize) % SPEAKER_COLORS.len()].to_string()
}

#[derive(Debug, Clone, Default)]
pub struct LiveSpeakerTracker {
    current: Option<String>,
    pending: Option<String>,
}

impl LiveSpeakerTracker {
    pub const fn new() -> Self {
        Self {
            current: None,
            pending: None,
        }
    }

    pub fn reset(&mut self) {
        self.current = None;
        self.pending = None;
    }

    /// Queue `name` for the next utterance. Empty / whitespace-only is ignored.
    /// Pressing the same name again still queues so the current utterance is cut.
    pub fn queue(&mut self, name: String) -> bool {
        let name = name.trim().to_string();
        if name.is_empty() {
            return false;
        }
        self.pending = Some(name);
        true
    }

    pub fn pending(&self) -> Option<&str> {
        self.pending.as_deref()
    }

    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    pub fn should_force_endpoint(&self) -> bool {
        self.pending.is_some()
    }

    /// Speaker that owns the utterance currently being decoded (not the pending one).
    pub fn stamp(&self) -> Option<String> {
        self.current.clone()
    }

    /// After the current utterance is finalized, promote pending so later speech
    /// belongs to the new speaker.
    pub fn apply_pending(&mut self) -> Option<String> {
        if let Some(name) = self.pending.take() {
            self.current = Some(name.clone());
            Some(name)
        } else {
            None
        }
    }
}

fn global_tracker() -> &'static Mutex<LiveSpeakerTracker> {
    static TRACKER: OnceLock<Mutex<LiveSpeakerTracker>> = OnceLock::new();
    TRACKER.get_or_init(|| Mutex::new(LiveSpeakerTracker::new()))
}

pub fn reset_session() {
    global_tracker().lock().reset();
}

pub fn queue_speaker(name: String) -> bool {
    global_tracker().lock().queue(name)
}

pub fn should_force_endpoint() -> bool {
    global_tracker().lock().should_force_endpoint()
}

pub fn stamp() -> Option<String> {
    global_tracker().lock().stamp()
}

pub fn apply_pending() -> Option<String> {
    global_tracker().lock().apply_pending()
}

pub fn default_hotkeys() -> BTreeMap<String, String> {
    (1..=9).map(|i| (i.to_string(), String::new())).collect()
}

fn normalize_hotkeys(raw: BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut out = default_hotkeys();
    for i in 1..=9 {
        let k = i.to_string();
        if let Some(v) = raw.get(&k) {
            out.insert(k, v.trim().to_string());
        }
    }
    out
}

fn hotkeys_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(dir.join(HOTKEYS_FILE))
}

fn read_hotkeys_file(path: &PathBuf) -> BTreeMap<String, String> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return default_hotkeys();
    };
    match serde_json::from_str::<BTreeMap<String, String>>(&raw) {
        Ok(map) => normalize_hotkeys(map),
        Err(_) => default_hotkeys(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveSpeakerEvent {
    pub name: Option<String>,
    pub color: Option<String>,
}

#[tauri::command]
pub fn get_speaker_hotkeys<R: Runtime>(
    app: AppHandle<R>,
) -> Result<BTreeMap<String, String>, String> {
    let path = hotkeys_path(&app)?;
    Ok(read_hotkeys_file(&path))
}

#[tauri::command]
pub fn save_speaker_hotkeys<R: Runtime>(
    app: AppHandle<R>,
    hotkeys: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, String> {
    let path = hotkeys_path(&app)?;
    let normalized = normalize_hotkeys(hotkeys);
    let json = serde_json::to_string_pretty(&normalized).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(normalized)
}

#[tauri::command]
pub async fn insert_live_speaker<R: Runtime>(
    app: AppHandle<R>,
    name: String,
) -> Result<bool, String> {
    if !crate::audio::recording_commands::is_recording().await {
        return Ok(false);
    }
    let trimmed = name.trim().to_string();
    if !queue_speaker(trimmed.clone()) {
        return Ok(false);
    }
    let payload = LiveSpeakerEvent {
        name: Some(trimmed.clone()),
        color: Some(color_for_name(&trimmed)),
    };
    let _ = app.emit("live-speaker-pending", &payload);
    Ok(true)
}

pub fn emit_speaker_committed<R: Runtime>(app: &AppHandle<R>, name: &str) {
    let payload = LiveSpeakerEvent {
        name: Some(name.to_string()),
        color: Some(color_for_name(name)),
    };
    let _ = app.emit("live-speaker-changed", &payload);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_rejects_empty_and_whitespace() {
        let mut t = LiveSpeakerTracker::new();
        assert!(!t.queue("".into()));
        assert!(!t.queue("   ".into()));
        assert!(t.pending().is_none());
        assert!(!t.should_force_endpoint());
    }

    #[test]
    fn stamp_stays_old_until_apply_pending() {
        let mut t = LiveSpeakerTracker::new();
        assert!(t.stamp().is_none());
        assert!(t.queue("Lan".into()));
        assert_eq!(t.pending(), Some("Lan"));
        assert!(t.should_force_endpoint());
        assert!(t.stamp().is_none(), "current utterance still unassigned");

        let applied = t.apply_pending();
        assert_eq!(applied.as_deref(), Some("Lan"));
        assert_eq!(t.stamp().as_deref(), Some("Lan"));
        assert!(t.pending().is_none());
        assert!(!t.should_force_endpoint());
    }

    #[test]
    fn next_utterance_keeps_new_speaker_until_another_queue() {
        let mut t = LiveSpeakerTracker::new();
        t.queue("Lan".into());
        t.apply_pending();
        t.queue("Minh".into());
        assert_eq!(t.stamp().as_deref(), Some("Lan"));
        t.apply_pending();
        assert_eq!(t.stamp().as_deref(), Some("Minh"));
    }

    #[test]
    fn same_name_still_queues_to_cut_utterance() {
        let mut t = LiveSpeakerTracker::new();
        t.queue("Lan".into());
        t.apply_pending();
        assert!(t.queue("Lan".into()));
        assert!(t.should_force_endpoint());
        assert_eq!(t.stamp().as_deref(), Some("Lan"));
        t.apply_pending();
        assert_eq!(t.stamp().as_deref(), Some("Lan"));
    }

    #[test]
    fn later_queue_overwrites_pending() {
        let mut t = LiveSpeakerTracker::new();
        t.queue("Lan".into());
        t.queue("Minh".into());
        assert_eq!(t.pending(), Some("Minh"));
        assert_eq!(t.apply_pending().as_deref(), Some("Minh"));
    }

    #[test]
    fn reset_clears_current_and_pending() {
        let mut t = LiveSpeakerTracker::new();
        t.queue("Lan".into());
        t.apply_pending();
        t.queue("Minh".into());
        t.reset();
        assert!(t.stamp().is_none());
        assert!(t.pending().is_none());
    }

    #[test]
    fn apply_pending_noop_when_empty() {
        let mut t = LiveSpeakerTracker::new();
        assert!(t.apply_pending().is_none());
    }

    #[test]
    fn queue_trims_name() {
        let mut t = LiveSpeakerTracker::new();
        t.queue("  Lan  ".into());
        assert_eq!(t.pending(), Some("Lan"));
    }

    #[test]
    fn same_name_maps_to_stable_color() {
        assert_eq!(color_for_name("Lan"), color_for_name("Lan"));
        assert_ne!(color_for_name("Lan"), color_for_name("Minh"));
    }

    #[test]
    fn normalize_hotkeys_keeps_only_slots_1_to_9() {
        let mut raw = BTreeMap::new();
        raw.insert("1".into(), "  Lan ".into());
        raw.insert("99".into(), "ignored".into());
        raw.insert("x".into(), "nope".into());
        let out = normalize_hotkeys(raw);
        assert_eq!(out.get("1").map(String::as_str), Some("Lan"));
        assert_eq!(out.get("2").map(String::as_str), Some(""));
        assert!(!out.contains_key("99"));
        assert_eq!(out.len(), 9);
    }
}
