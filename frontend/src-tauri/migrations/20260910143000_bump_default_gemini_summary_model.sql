-- Bump shipped Gemini summary default on install/update.
-- Only replaces the previous shipped default; leave custom model names alone.
UPDATE settings
SET model = 'gemini-3.6-flash'
WHERE provider = 'custom-openai'
  AND model = 'gemini-3.1-flash-lite';

UPDATE settings
SET customOpenAIConfig = json_set(customOpenAIConfig, '$.model', 'gemini-3.6-flash')
WHERE customOpenAIConfig IS NOT NULL
  AND json_valid(customOpenAIConfig)
  AND json_extract(customOpenAIConfig, '$.model') = 'gemini-3.1-flash-lite';
