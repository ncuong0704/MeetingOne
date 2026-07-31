use serde::{Deserialize, Serialize};

use super::prompts;

/// Custom LLM prompts configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptConfig {
    pub system_prompt_final_template: String,
}

impl PromptConfig {
    pub fn defaults() -> Self {
        Self {
            system_prompt_final_template: prompts::SYSTEM_PROMPT_FINAL_TEMPLATE.to_string(),
        }
    }

    /// Nâng cấp prompt đã lưu khi thiếu placeholder mới (ví dụ `{template_markdown}`).
    pub fn migrate_to_current_defaults(&self) -> Option<Self> {
        if self.system_prompt_final_template.contains("{template_markdown}") {
            return None;
        }

        let defaults = Self::defaults();
        let default_text = &defaults.system_prompt_final_template;
        let marker = "**MẪU MARKDOWN";
        let suffix = if let Some(idx) = default_text.find(marker) {
            default_text[idx..].to_string()
        } else {
            "\n\n**MẪU MARKDOWN (điền nội dung vào khung bên dưới, giữ nguyên cấu trúc tiêu đề):**\n\n{template_markdown}\n"
                .to_string()
        };

        let trimmed = self.system_prompt_final_template.trim_end();
        let migrated = Self {
            system_prompt_final_template: format!("{}\n\n{}", trimmed, suffix.trim_start()),
        };
        Some(migrated)
    }

    /// Trả prompt hiệu lực: mặc định nếu chưa lưu, hoặc bản đã lưu sau migrate.
    pub fn resolve_with_defaults(custom: Option<Self>) -> Self {
        let base = custom.unwrap_or_else(Self::defaults);
        base.migrate_to_current_defaults().unwrap_or(base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_adds_template_markdown_when_missing() {
        let old = PromptConfig {
            system_prompt_final_template: "Hello {section_instructions}".to_string(),
        };
        let migrated = old.migrate_to_current_defaults().expect("should migrate");
        assert!(migrated.system_prompt_final_template.contains("{template_markdown}"));
        assert!(migrated.system_prompt_final_template.starts_with("Hello {section_instructions}"));
    }

    #[test]
    fn migrate_skips_when_placeholder_present() {
        let current = PromptConfig {
            system_prompt_final_template: "Already has {template_markdown}".to_string(),
        };
        assert!(current.migrate_to_current_defaults().is_none());
    }
}
