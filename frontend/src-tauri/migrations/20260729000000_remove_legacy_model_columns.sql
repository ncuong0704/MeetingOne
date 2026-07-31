-- Remove legacy STT/LLM provider columns (Whisper, Groq, Ollama, Deepgram, etc.)
-- Keeps only ZipFormer transcript config and 4 cloud summary providers.

PRAGMA foreign_keys=off;

-- Recreate transcript_settings without cloud STT API key columns
CREATE TABLE transcript_settings_new (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    zipformerVariant TEXT NOT NULL DEFAULT 'int8',
    decodingMethod TEXT NOT NULL DEFAULT 'modified_beam_search',
    numActivePaths INTEGER NOT NULL DEFAULT 15
);

INSERT INTO transcript_settings_new (id, provider, model, zipformerVariant, decodingMethod, numActivePaths)
SELECT
    id,
    CASE
        WHEN provider IN ('parakeet', 'localWhisper', 'whisper', '') OR provider IS NULL THEN 'zipformer'
        ELSE provider
    END,
    CASE
        WHEN model IS NULL OR model = '' OR model LIKE '%parakeet%' OR model LIKE '%ggml%' OR model = 'large-v3' THEN 'zipformer-vi-30m'
        ELSE model
    END,
    COALESCE(zipformerVariant, 'int8'),
    COALESCE(decodingMethod, 'modified_beam_search'),
    COALESCE(numActivePaths, 15)
FROM transcript_settings;

DROP TABLE transcript_settings;
ALTER TABLE transcript_settings_new RENAME TO transcript_settings;

-- Recreate settings without whisperModel / ollama / groq columns
CREATE TABLE settings_new (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    openaiApiKey TEXT,
    anthropicApiKey TEXT,
    openRouterApiKey TEXT,
    customOpenAIConfig TEXT,
    fallbackModels TEXT,
    promptSettings TEXT,
    defaultTemplate TEXT
);

INSERT INTO settings_new (
    id, provider, model,
    openaiApiKey, anthropicApiKey, openRouterApiKey,
    customOpenAIConfig, fallbackModels, promptSettings, defaultTemplate
)
SELECT
    id,
    CASE WHEN provider IN ('ollama', 'groq') THEN 'custom-openai' ELSE provider END,
    CASE WHEN provider IN ('ollama', 'groq') THEN 'gemini-3.1-flash-lite' ELSE model END,
    openaiApiKey,
    anthropicApiKey,
    openRouterApiKey,
    customOpenAIConfig,
    fallbackModels,
    promptSettings,
    defaultTemplate
FROM settings;

DROP TABLE settings;
ALTER TABLE settings_new RENAME TO settings;

PRAGMA foreign_keys=on;
