use crate::database::repositories::setting::SettingsRepository;
use crate::state::AppState;
use crate::summary::templates;
use serde::{Deserialize, Serialize};
use tauri::Runtime;
use tracing::{info, warn};

/// Template metadata for UI display
#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateInfo {
    /// Template identifier (e.g., "daily_standup", "standard_meeting")
    pub id: String,

    /// Display name for the template
    pub name: String,

    /// Brief description of the template's purpose
    pub description: String,

    /// True nếu template chỉ nằm trong custom_dir (không phải built-in)
    pub is_custom: bool,

    /// True nếu là built-in nhưng bị ghi đè bởi file trong custom_dir
    pub has_custom_override: bool,
}

/// Detailed template structure for preview/debugging
#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateDetails {
    /// Template identifier
    pub id: String,

    /// Display name
    pub name: String,

    /// Description
    pub description: String,

    /// List of section titles in order
    pub sections: Vec<String>,
}

/// Lists all available templates
///
/// Returns templates from both built-in (embedded) and custom (user data directory) sources.
/// Templates are automatically discovered - no code changes needed to add new templates.
///
/// # Returns
/// Vector of TemplateInfo with id, name, and description for each template
#[tauri::command]
pub async fn api_list_templates<R: Runtime>(
    _app: tauri::AppHandle<R>,
) -> Result<Vec<TemplateInfo>, String> {
    info!("api_list_templates called");

    let templates = templates::list_templates_with_source();

    let template_infos: Vec<TemplateInfo> = templates
        .into_iter()
        .map(|(id, name, description, is_custom, has_custom_override)| TemplateInfo {
            id,
            name,
            description,
            is_custom,
            has_custom_override,
        })
        .collect();

    info!("Found {} available templates", template_infos.len());

    Ok(template_infos)
}

/// Gets detailed information about a specific template
///
/// # Arguments
/// * `template_id` - Template identifier (e.g., "daily_standup")
///
/// # Returns
/// TemplateDetails with full template structure
#[tauri::command]
pub async fn api_get_template_details<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: String,
) -> Result<TemplateDetails, String> {
    info!("api_get_template_details called for template_id: {}", template_id);

    let template = templates::get_template(&template_id)?;

    let section_titles: Vec<String> = template
        .sections
        .iter()
        .map(|section| section.title.clone())
        .collect();

    let details = TemplateDetails {
        id: template_id,
        name: template.name,
        description: template.description,
        sections: section_titles,
    };

    info!("Retrieved template details for '{}'", details.name);

    Ok(details)
}

/// Validates a custom template JSON string
///
/// Useful for template editor UI or validation before saving custom templates
///
/// # Arguments
/// * `template_json` - Raw JSON string of the template
///
/// # Returns
/// Ok(template_name) if valid, Err(error_message) if invalid
#[tauri::command]
pub async fn api_validate_template<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_json: String,
) -> Result<String, String> {
    info!("api_validate_template called");

    match templates::validate_and_parse_template(&template_json) {
        Ok(template) => {
            info!("Template '{}' validated successfully", template.name);
            Ok(template.name)
        }
        Err(e) => {
            warn!("Template validation failed: {}", e);
            Err(e)
        }
    }
}

/// Trả về raw JSON string của một template (để hiển thị/chỉnh sửa trong UI)
///
/// Ưu tiên: custom → bundled → built-in
///
/// # Arguments
/// * `template_id` - Template identifier
///
/// # Returns
/// Ok(json_string) nếu tìm thấy, Err(error_message) nếu không
#[tauri::command]
pub async fn api_get_template_json<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: String,
) -> Result<String, String> {
    info!("api_get_template_json called for template_id: {}", template_id);

    if let Some(content) = templates::get_custom_template_json(&template_id) {
        return Ok(content);
    }
    if let Some(content) = templates::get_bundled_template_json(&template_id) {
        return Ok(content);
    }
    if let Some(content) = templates::get_builtin_template(&template_id) {
        return Ok(content.to_string());
    }

    Err(format!("Template '{}' not found", template_id))
}

/// Lưu template tùy chỉnh vào thư mục dữ liệu người dùng
///
/// Validate JSON trước khi ghi. Template ID chỉ được chứa [a-zA-Z0-9_-].
///
/// # Arguments
/// * `template_id` - Tên file (không có .json extension)
/// * `template_json` - Nội dung JSON của template
///
/// # Returns
/// Ok(()) nếu thành công, Err(error_message) nếu thất bại
#[tauri::command]
pub async fn api_save_custom_template<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: String,
    template_json: String,
) -> Result<(), String> {
    info!("api_save_custom_template called for template_id: {}", template_id);

    if template_id.is_empty() {
        return Err("Template ID không được để trống".to_string());
    }
    if !template_id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err("Template ID chỉ được chứa chữ cái, số, dấu gạch dưới hoặc dấu gạch ngang".to_string());
    }

    templates::validate_and_parse_template(&template_json)
        .map_err(|e| format!("Template không hợp lệ: {}", e))?;

    let custom_dir = templates::get_custom_templates_dir_pub()
        .ok_or_else(|| "Không xác định được thư mục lưu template".to_string())?;

    std::fs::create_dir_all(&custom_dir)
        .map_err(|e| format!("Không tạo được thư mục template: {}", e))?;

    let file_path = custom_dir.join(format!("{}.json", template_id));
    std::fs::write(&file_path, template_json.as_bytes())
        .map_err(|e| format!("Không ghi được file template: {}", e))?;

    info!("Custom template '{}' saved to {:?}", template_id, file_path);
    Ok(())
}

/// Xóa template tùy chỉnh khỏi thư mục dữ liệu người dùng
///
/// Không thể xóa template built-in (nhúng trong binary).
///
/// # Arguments
/// * `template_id` - Template identifier cần xóa
///
/// # Returns
/// Ok(()) nếu thành công, Err(error_message) nếu thất bại
#[tauri::command]
pub async fn api_delete_custom_template<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: String,
) -> Result<(), String> {
    info!("api_delete_custom_template called for template_id: {}", template_id);

    let custom_dir = templates::get_custom_templates_dir_pub()
        .ok_or_else(|| "Không xác định được thư mục template".to_string())?;

    let file_path = custom_dir.join(format!("{}.json", template_id));

    if !file_path.exists() {
        if templates::get_builtin_template(&template_id).is_some() {
            return Err("Không thể xóa template mặc định (built-in)".to_string());
        }
        return Err(format!("Template tùy chỉnh '{}' không tìm thấy", template_id));
    }

    std::fs::remove_file(&file_path)
        .map_err(|e| format!("Không xóa được file template: {}", e))?;

    info!("Custom template '{}' deleted from {:?}", template_id, file_path);
    Ok(())
}

/// Lấy ID template mặc định đã được người dùng chọn.
/// Trả về "theo_mau_act_no_table" nếu chưa có setting.
#[tauri::command]
pub async fn api_get_default_template<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    info!("api_get_default_template called");
    let pool = state.db_manager.pool();
    match SettingsRepository::get_default_template(pool).await {
        Ok(Some(id)) => Ok(id),
        Ok(None) => Ok("theo_mau_act_no_table".to_string()),
        Err(e) => Err(format!("Không lấy được template mặc định: {}", e)),
    }
}

/// Lưu ID template mặc định do người dùng chọn.
#[tauri::command]
pub async fn api_set_default_template<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    template_id: String,
) -> Result<(), String> {
    info!("api_set_default_template called with id: {}", template_id);
    if template_id.is_empty() {
        return Err("Template ID không được để trống".to_string());
    }
    let pool = state.db_manager.pool();
    SettingsRepository::save_default_template(pool, &template_id)
        .await
        .map_err(|e| format!("Không lưu được template mặc định: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_list_templates() {
        // This test requires the templates to be embedded/available
        // In a real test environment, you might want to mock the templates module

        // For now, just verify the function compiles and runs
        // You can expand this with more specific assertions
    }

    #[tokio::test]
    async fn test_validate_template_valid() {
        let valid_json = r#"
        {
            "name": "Test Template",
            "description": "A test template",
            "sections": [
                {
                    "title": "Summary",
                    "instruction": "Provide a summary",
                    "format": "paragraph"
                }
            ]
        }"#;

        // Mock app handle would be needed for actual testing
        // For now, test the validation logic directly
        let result = templates::validate_and_parse_template(valid_json);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_template_invalid() {
        let invalid_json = "invalid json";

        let result = templates::validate_and_parse_template(invalid_json);
        assert!(result.is_err());
    }
}
