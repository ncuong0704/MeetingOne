use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;
use log::{info, warn, error};
use anyhow::Result;

use crate::state::AppState;
use crate::database::repositories::setting::SettingsRepository;

/// A single model option shown to the user during onboarding
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelOption {
    pub name: String,
    pub family: String,
    pub size_gb: f32,
    pub description: String,
    pub is_pulled: bool,
    pub is_recommended: bool,
}

/// Returns a list of Ollama models compatible with the system RAM,
/// each marked with whether it is already pulled locally and whether it is recommended.
#[tauri::command]
pub async fn get_ollama_model_options(
    endpoint: Option<String>,
) -> Result<Vec<ModelOption>, String> {
    // Read total system RAM in GB
    let ram_gb = tokio::task::spawn_blocking(|| {
        use sysinfo::System;
        let mut sys = System::new();
        sys.refresh_memory();
        sys.total_memory() / (1024 * 1024 * 1024)
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {}", e))?;

    // Curated catalogue: (name, family, size_gb, min_ram_gb, description)
    // Ordered from smallest to largest. Only sub-4B models with good Vietnamese support.
    let catalogue: &[(&str, &str, f32, u64, &str)] = &[
        ("qwen3.5:0.8b", "Qwen", 1.0, 4, "Qwen 3.5 0.8B (Feb 2026) — siêu nhẹ, 201 ngôn ngữ, tiếng Việt tốt"),
        ("qwen3.5:2b",   "Qwen", 2.7, 4, "Qwen 3.5 2B (Feb 2026) — tốt nhất dưới 4B, tóm tắt tiếng Việt xuất sắc"),
    ];

    // Recommended tag must appear exactly in `catalogue` above.
    let recommended = if ram_gb >= 4 {
        "qwen3.5:2b"
    } else {
        "qwen3.5:0.8b"
    };

    // Fetch locally pulled models (ignore errors — Ollama may not be running yet)
    let pulled_names: Vec<String> = crate::ollama::ollama::get_ollama_models(endpoint)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| m.name.to_lowercase())
        .collect();

    let options: Vec<ModelOption> = catalogue
        .iter()
        .filter(|(_, _, _, min_ram, _)| ram_gb >= *min_ram)
        .map(|(name, family, size_gb, _, desc)| {
            let name_lc = name.to_lowercase();
            // Exact tag or same base with extra suffix (e.g. `qwen3:4b-q4_K_M`), not `qwen3:8b`.
            let is_pulled = pulled_names.iter().any(|p| {
                p == &name_lc
                    || (p.starts_with(&name_lc)
                        && p.len() > name_lc.len()
                        && matches!(
                            p.as_bytes().get(name_lc.len()),
                            Some(b':' | b'-')
                        ))
            });
            ModelOption {
                name: name.to_string(),
                family: family.to_string(),
                size_gb: *size_gb,
                description: desc.to_string(),
                is_pulled,
                is_recommended: *name == recommended,
            }
        })
        .collect();

    info!("Returning {} model options for RAM={}GB", options.len(), ram_gb);
    Ok(options)
}

/// Recommended Ollama model for meeting summarization based on system RAM.
/// Logic: match model size tier to available RAM.
#[tauri::command]
pub async fn get_ollama_model_recommendation() -> Result<OllamaModelRecommendation, String> {
    // Run blocking sysinfo call on a dedicated thread to avoid blocking the async runtime
    let ram_gb = tokio::task::spawn_blocking(|| {
        use sysinfo::System;
        let mut sys = System::new();
        sys.refresh_memory();
        sys.total_memory() / (1024 * 1024 * 1024)
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {}", e))?;

    // Footprint ≈ quantized weights; leave headroom for OS + app + context.
    let (model, size_gb, description) = if ram_gb >= 4 {
        (
            "qwen3.5:2b",
            2.7,
            "Qwen 3.5 2B (Feb 2026) — tốt nhất dưới 4B, tóm tắt tiếng Việt xuất sắc",
        )
    } else {
        (
            "qwen3.5:0.8b",
            1.0,
            "Qwen 3.5 0.8B (Feb 2026) — siêu nhẹ, 201 ngôn ngữ, tiếng Việt tốt",
        )
    };

    info!("Ollama model recommendation: {} (RAM={}GB)", model, ram_gb);

    Ok(OllamaModelRecommendation {
        model: model.to_string(),
        size_gb,
        description: description.to_string(),
        ram_gb,
        pull_command: format!("ollama pull {}", model),
    })
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OllamaModelRecommendation {
    pub model: String,
    pub size_gb: f32,
    pub description: String,
    pub ram_gb: u64,
    pub pull_command: String,
}


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OnboardingStatus {
    pub version: String,
    pub completed: bool,
    pub current_step: u8,
    pub model_status: ModelStatus,
    pub last_updated: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ModelStatus {
    #[serde(alias = "parakeet", default = "default_not_downloaded")]
    pub zipformer: String,  // "downloaded" | "not_downloaded" | "downloading"
    #[serde(default = "default_not_downloaded")]
    pub summary: String,
}

fn default_not_downloaded() -> String {
    "not_downloaded".to_string()
}

impl Default for OnboardingStatus {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            completed: false,
            current_step: 1,
            model_status: ModelStatus {
                zipformer: "not_downloaded".to_string(),
                summary: "not_downloaded".to_string(),
            },
            last_updated: chrono::Utc::now().to_rfc3339(),
        }
    }
}


/// Load onboarding status from store
pub async fn load_onboarding_status<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<OnboardingStatus> {
    // Try to load from Tauri store
    let store = match app.store("onboarding-status.json") {
        Ok(store) => store,
        Err(e) => {
            warn!("Failed to access onboarding store: {}, using defaults", e);
            return Ok(OnboardingStatus::default());
        }
    };

    // Try to get the status from store
    let status = if let Some(value) = store.get("status") {
        match serde_json::from_value::<OnboardingStatus>(value.clone()) {
            Ok(s) => {
                info!("Loaded onboarding status from store - Step: {}, Completed: {}",
                      s.current_step, s.completed);
                s
            }
            Err(e) => {
                warn!("Failed to deserialize onboarding status: {}, using defaults", e);
                OnboardingStatus::default()
            }
        }
    } else {
        info!("No stored onboarding status found, using defaults");
        OnboardingStatus::default()
    };

    Ok(status)
}

/// Save onboarding status to store
pub async fn save_onboarding_status<R: Runtime>(
    app: &AppHandle<R>,
    status: &OnboardingStatus,
) -> Result<()> {
    info!("Saving onboarding status: step={}, completed={}",
          status.current_step, status.completed);

    // Get or create store
    let store = app.store("onboarding-status.json")
        .map_err(|e| anyhow::anyhow!("Failed to access onboarding store: {}", e))?;

    // Update last_updated timestamp
    let mut status = status.clone();
    status.last_updated = chrono::Utc::now().to_rfc3339();

    // Serialize status to JSON value
    let status_value = serde_json::to_value(&status)
        .map_err(|e| anyhow::anyhow!("Failed to serialize onboarding status: {}", e))?;

    // Save to store
    store.set("status", status_value);

    // Persist to disk
    store.save()
        .map_err(|e| anyhow::anyhow!("Failed to save onboarding store to disk: {}", e))?;

    info!("Successfully persisted onboarding status to disk");
    Ok(())
}

/// Reset onboarding status (delete from store)
pub async fn reset_onboarding_status<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<()> {
    info!("Resetting onboarding status");

    let store = app.store("onboarding-status.json")
        .map_err(|e| anyhow::anyhow!("Failed to access onboarding store: {}", e))?;

    // Clear the status key
    store.delete("status");

    // Persist deletion to disk
    store.save()
        .map_err(|e| anyhow::anyhow!("Failed to save onboarding store after reset: {}", e))?;

    info!("Successfully reset onboarding status");
    Ok(())
}

/// Tauri commands for onboarding status
#[tauri::command]
pub async fn get_onboarding_status<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<OnboardingStatus>, String> {
    let status = load_onboarding_status(&app)
        .await
        .map_err(|e| format!("Failed to load onboarding status: {}", e))?;

    // Return None if it's the default (never saved before)
    // Check if we have any saved data by seeing if the store has the key
    let store = app.store("onboarding-status.json")
        .map_err(|e| format!("Failed to access store: {}", e))?;

    if store.get("status").is_none() {
        Ok(None)
    } else {
        Ok(Some(status))
    }
}

#[tauri::command]
pub async fn save_onboarding_status_cmd<R: Runtime>(
    app: AppHandle<R>,
    status: OnboardingStatus,
) -> Result<(), String> {
    save_onboarding_status(&app, &status)
        .await
        .map_err(|e| format!("Failed to save onboarding status: {}", e))
}

#[tauri::command]
pub async fn reset_onboarding_status_cmd<R: Runtime>(
    app: AppHandle<R>,
) -> Result<(), String> {
    reset_onboarding_status(&app)
        .await
        .map_err(|e| format!("Failed to reset onboarding status: {}", e))
}

#[tauri::command]
pub async fn complete_onboarding<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    info!("Completing onboarding (summary model config saved by frontend)");

    let pool = state.db_manager.pool();

    // Save transcription model config (ZipFormer Vietnamese ASR)
    if let Err(e) = SettingsRepository::save_transcript_config(
        pool,
        "zipformer",
        crate::config::ZIPFORMER_MODEL_NAME,
    ).await {
        error!("Failed to save transcription model config: {}", e);
        return Err(format!("Failed to save transcription model config: {}", e));
    }
    info!("Saved transcription model config: provider=zipformer, model={}", crate::config::ZIPFORMER_MODEL_NAME);

    // Step 2: Only NOW mark onboarding as complete (after DB operations succeed)
    let mut status = load_onboarding_status(&app)
        .await
        .map_err(|e| format!("Failed to load onboarding status: {}", e))?;

    status.completed = true;
    status.current_step = 3;
    status.model_status.zipformer = "not_downloaded".to_string(); // user downloads separately
    status.model_status.summary = "not_downloaded".to_string();

    save_onboarding_status(&app, &status)
        .await
        .map_err(|e| format!("Failed to save completed onboarding status: {}", e))?;

    info!("Onboarding completed successfully");
    Ok(())
}
