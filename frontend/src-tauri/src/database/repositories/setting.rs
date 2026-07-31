use crate::database::models::{Setting, TranscriptSetting};
use crate::summary::CustomOpenAIConfig;
use sqlx::SqlitePool;

#[derive(serde::Deserialize, Debug)]
pub struct SaveModelConfigRequest {
    pub provider: String,
    pub model: String,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
pub struct SaveTranscriptConfigRequest {
    pub provider: String,
    pub model: String,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
    #[serde(rename = "zipformerVariant")]
    pub zipformer_variant: Option<String>,
    #[serde(rename = "decodingMethod")]
    pub decoding_method: Option<String>,
    #[serde(rename = "numActivePaths")]
    pub num_active_paths: Option<i32>,
}

pub struct SettingsRepository;

// Transcript providers: zipformer only
// Summary providers: openai, claude, openrouter, custom-openai
// NOTE: Handle data exclusion in the higher layer as this is database abstraction layer(using SELECT *)

impl SettingsRepository {
    pub async fn get_model_config(
        pool: &SqlitePool,
    ) -> std::result::Result<Option<Setting>, sqlx::Error> {
        let setting = sqlx::query_as::<_, Setting>("SELECT * FROM settings LIMIT 1")
            .fetch_optional(pool)
            .await?;
        Ok(setting)
    }

    pub async fn save_model_config(
        pool: &SqlitePool,
        provider: &str,
        model: &str,
        fallback_models_json: Option<&str>,
    ) -> std::result::Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO settings (id, provider, model, fallbackModels)
            VALUES ('1', $1, $2, $3)
            ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                model = excluded.model,
                fallbackModels = COALESCE(excluded.fallbackModels, settings.fallbackModels)
            "#,
        )
        .bind(provider)
        .bind(model)
        .bind(fallback_models_json)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Returns the list of fallback model names for the given provider.
    /// Reads from the `fallbackModels` JSON map column.
    pub async fn get_fallback_models(
        pool: &SqlitePool,
        provider: &str,
    ) -> std::result::Result<Vec<String>, sqlx::Error> {
        let setting = Self::get_model_config(pool).await?;
        let map_str = setting
            .and_then(|s| s.fallback_models)
            .unwrap_or_default();
        if map_str.is_empty() {
            return Ok(vec![]);
        }
        let map: serde_json::Value = serde_json::from_str(&map_str).unwrap_or_default();
        let models = map[provider]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Ok(models)
    }

    pub async fn save_api_key(
        pool: &SqlitePool,
        provider: &str,
        api_key: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        // Custom OpenAI uses JSON config (customOpenAIConfig) instead of a separate API key column
        if provider == "custom-openai" {
            return Err(sqlx::Error::Protocol(
                "custom-openai provider should use save_custom_openai_config() instead of save_api_key()".into(),
            ));
        }

        let api_key_column = match provider {
            "openai" => "openaiApiKey",
            "claude" => "anthropicApiKey",
            "openrouter" => "openRouterApiKey",
            _ => {
                return Err(sqlx::Error::Protocol(
                    format!("Invalid provider: {}", provider).into(),
                ))
            }
        };

        let query = format!(
            r#"
            INSERT INTO settings (id, provider, model, "{}")
            VALUES ('1', 'openai', 'gpt-4o-2024-11-20', $1)
            ON CONFLICT(id) DO UPDATE SET
                "{}" = $1
            "#,
            api_key_column, api_key_column
        );
        sqlx::query(&query).bind(api_key).execute(pool).await?;

        Ok(())
    }

    pub async fn get_api_key(
        pool: &SqlitePool,
        provider: &str,
    ) -> std::result::Result<Option<String>, sqlx::Error> {
        // Custom OpenAI uses JSON config - extract API key from there
        if provider == "custom-openai" {
            let config = Self::get_custom_openai_config(pool).await?;
            return Ok(config.and_then(|c| c.api_key));
        }

        let api_key_column = match provider {
            "openai" => "openaiApiKey",
            "claude" => "anthropicApiKey",
            "openrouter" => "openRouterApiKey",
            _ => {
                return Err(sqlx::Error::Protocol(
                    format!("Invalid provider: {}", provider).into(),
                ))
            }
        };

        let query = format!(
            "SELECT {} FROM settings WHERE id = '1' LIMIT 1",
            api_key_column
        );
        let api_key = sqlx::query_scalar(&query).fetch_optional(pool).await?;
        Ok(api_key)
    }

    pub async fn get_transcript_config(
        pool: &SqlitePool,
    ) -> std::result::Result<Option<TranscriptSetting>, sqlx::Error> {
        let setting =
            sqlx::query_as::<_, TranscriptSetting>("SELECT * FROM transcript_settings LIMIT 1")
                .fetch_optional(pool)
                .await?;
        Ok(setting)

    }

    pub async fn save_transcript_config(
        pool: &SqlitePool,
        provider: &str,
        model: &str,
        zipformer_variant: &str,
        decoding_method: &str,
        num_active_paths: i32,
    ) -> std::result::Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO transcript_settings
                (id, provider, model, zipformerVariant, decodingMethod, numActivePaths)
            VALUES ('1', $1, $2, $3, $4, $5)
            ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                model = excluded.model,
                zipformerVariant = excluded.zipformerVariant,
                decodingMethod = excluded.decodingMethod,
                numActivePaths = excluded.numActivePaths
            "#,
        )
        .bind(provider)
        .bind(model)
        .bind(zipformer_variant)
        .bind(decoding_method)
        .bind(num_active_paths)
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn save_transcript_api_key(
        _pool: &SqlitePool,
        provider: &str,
        _api_key: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        if provider != "zipformer" {
            return Err(sqlx::Error::Protocol(
                format!("Unsupported transcript provider: {}. Only zipformer is supported.", provider).into(),
            ));
        }
        Ok(())
    }

    pub async fn get_transcript_api_key(
        _pool: &SqlitePool,
        provider: &str,
    ) -> std::result::Result<Option<String>, sqlx::Error> {
        if provider != "zipformer" {
            return Err(sqlx::Error::Protocol(
                format!("Unsupported transcript provider: {}. Only zipformer is supported.", provider).into(),
            ));
        }
        Ok(None)
    }

    pub async fn delete_api_key(
        pool: &SqlitePool,
        provider: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        // Custom OpenAI uses JSON config - clear the entire config
        if provider == "custom-openai" {
            sqlx::query("UPDATE settings SET customOpenAIConfig = NULL WHERE id = '1'")
                .execute(pool)
                .await?;
            return Ok(());
        }

        let api_key_column = match provider {
            "openai" => "openaiApiKey",
            "claude" => "anthropicApiKey",
            "openrouter" => "openRouterApiKey",
            _ => {
                return Err(sqlx::Error::Protocol(
                    format!("Invalid provider: {}", provider).into(),
                ))
            }
        };

        let query = format!(
            "UPDATE settings SET {} = NULL WHERE id = '1'",
            api_key_column
        );
        sqlx::query(&query).execute(pool).await?;

        Ok(())
    }

    // ===== CUSTOM OPENAI CONFIG METHODS =====

    /// Gets the custom OpenAI configuration from JSON
    ///
    /// # Returns
    /// * `Ok(Some(CustomOpenAIConfig))` - Config exists and is valid JSON
    /// * `Ok(None)` - No config stored
    /// * `Err(sqlx::Error)` - Database error
    pub async fn get_custom_openai_config(
        pool: &SqlitePool,
    ) -> std::result::Result<Option<CustomOpenAIConfig>, sqlx::Error> {
        use sqlx::Row;

        let row = sqlx::query(
            r#"
            SELECT customOpenAIConfig
            FROM settings
            WHERE id = '1'
            LIMIT 1
            "#
        )
        .fetch_optional(pool)
        .await?;

        match row {
            Some(record) => {
                let config_json: Option<String> = record.get("customOpenAIConfig");

                if let Some(json) = config_json {
                    // Parse JSON into CustomOpenAIConfig
                    let config: CustomOpenAIConfig = serde_json::from_str(&json)
                        .map_err(|e| sqlx::Error::Protocol(
                            format!("Invalid JSON in customOpenAIConfig: {}", e).into()
                        ))?;

                    Ok(Some(config))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Saves the custom OpenAI configuration as JSON
    ///
    /// # Arguments
    /// * `pool` - Database connection pool
    /// * `config` - CustomOpenAIConfig to save (includes endpoint, apiKey, model, maxTokens, temperature, topP)
    ///
    /// # Returns
    /// * `Ok(())` - Config saved successfully
    /// * `Err(sqlx::Error)` - Database or JSON serialization error
    pub async fn save_custom_openai_config(
        pool: &SqlitePool,
        config: &CustomOpenAIConfig,
    ) -> std::result::Result<(), sqlx::Error> {
        // Serialize config to JSON
        let config_json = serde_json::to_string(config)
            .map_err(|e| sqlx::Error::Protocol(
                format!("Failed to serialize config to JSON: {}", e).into()
            ))?;

        // Upsert into settings table
        sqlx::query(
            r#"
            INSERT INTO settings (id, provider, model, customOpenAIConfig)
            VALUES ('1', 'custom-openai', $1, $2)
            ON CONFLICT(id) DO UPDATE SET
                customOpenAIConfig = excluded.customOpenAIConfig
            "#,
        )
        .bind(&config.model)
        .bind(config_json)
        .execute(pool)
        .await?;

        Ok(())
    }

    // ===== PROMPT SETTINGS METHODS =====

    /// Retrieves custom prompt settings from the database.
    /// Returns `Ok(None)` when no custom prompts have been saved yet (use built-in defaults).
    pub async fn get_prompt_settings(
        pool: &SqlitePool,
    ) -> std::result::Result<Option<crate::summary::PromptConfig>, sqlx::Error> {
        use sqlx::Row;

        let row = sqlx::query("SELECT promptSettings FROM settings WHERE id = '1' LIMIT 1")
            .fetch_optional(pool)
            .await?;

        match row {
            Some(record) => {
                let json: Option<String> = record.get("promptSettings");
                match json {
                    Some(j) => {
                        let config: crate::summary::PromptConfig = serde_json::from_str(&j)
                            .map_err(|e| sqlx::Error::Protocol(
                                format!("Invalid JSON in promptSettings: {}", e).into()
                            ))?;
                        Ok(Some(config))
                    }
                    None => Ok(None),
                }
            }
            None => Ok(None),
        }
    }

    /// Saves custom prompt settings as JSON.
    pub async fn save_prompt_settings(
        pool: &SqlitePool,
        config: &crate::summary::PromptConfig,
    ) -> std::result::Result<(), sqlx::Error> {
        let json = serde_json::to_string(config)
            .map_err(|e| sqlx::Error::Protocol(
                format!("Failed to serialize prompt settings: {}", e).into()
            ))?;

        sqlx::query(
            r#"
            INSERT INTO settings (id, provider, model, promptSettings)
            VALUES ('1', 'openai', 'gpt-4o-2024-11-20', $1)
            ON CONFLICT(id) DO UPDATE SET
                promptSettings = excluded.promptSettings
            "#,
        )
        .bind(json)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Resets custom prompt settings to built-in defaults by clearing the stored JSON.
    pub async fn reset_prompt_settings(
        pool: &SqlitePool,
    ) -> std::result::Result<(), sqlx::Error> {
        sqlx::query("UPDATE settings SET promptSettings = NULL WHERE id = '1'")
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Retrieves the ID of the user's chosen default template. Returns None when not set.
    pub async fn get_default_template(
        pool: &SqlitePool,
    ) -> std::result::Result<Option<String>, sqlx::Error> {
        use sqlx::Row;
        let row = sqlx::query("SELECT defaultTemplate FROM settings WHERE id = '1' LIMIT 1")
            .fetch_optional(pool)
            .await?;
        Ok(row.and_then(|r| r.get("defaultTemplate")))
    }

    /// Persists the user's chosen default template ID.
    pub async fn save_default_template(
        pool: &SqlitePool,
        template_id: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO settings (id, provider, model, defaultTemplate)
            VALUES ('1', 'openai', 'gpt-4o-2024-11-20', $1)
            ON CONFLICT(id) DO UPDATE SET defaultTemplate = excluded.defaultTemplate
            "#,
        )
        .bind(template_id)
        .execute(pool)
        .await?;
        Ok(())
    }
}
