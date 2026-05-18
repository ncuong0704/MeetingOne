-- Add fallbackModels column to store per-provider fallback model list as JSON
-- Format: {"groq": ["model-b", "model-c"], "openai": ["gpt-4o-mini"]}
ALTER TABLE settings ADD COLUMN fallbackModels TEXT;
