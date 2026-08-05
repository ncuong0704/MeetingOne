use super::engine::{ItnEngine, ITN_ENGINE};
use crate::config::{ITN_CLASSIFY_FAR, ITN_RESOURCE_SUBDIR};
use log::{error, info};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, Runtime};

pub fn resolve_itn_dir<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    if let Ok(resource) = app.path().resource_dir() {
        let bundled = resource.join(ITN_RESOURCE_SUBDIR);
        if bundled.join(ITN_CLASSIFY_FAR).exists() {
            return bundled;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(ITN_RESOURCE_SUBDIR)
}

pub fn init_on_startup<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let path = resolve_itn_dir(&app);
        match ItnEngine::load_from_dir(&path) {
            Ok(engine) => {
                *ITN_ENGINE.lock().unwrap() = Some(engine);
                info!("ITN engine initialized from {:?}", path);
            }
            Err(e) => error!("ITN engine failed to load (ITN disabled): {}", e),
        }
    });
}

#[tauri::command]
pub async fn itn_is_ready() -> Result<bool, String> {
    Ok(ITN_ENGINE.lock().unwrap().is_some())
}
