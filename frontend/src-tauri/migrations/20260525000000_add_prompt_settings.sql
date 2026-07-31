-- Add promptSettings column to store custom LLM prompts as JSON
-- Format: {"system_prompt_chunk": "...", "user_prompt_template_chunk": "...", ...}
-- NULL means use built-in defaults from prompts.rs
ALTER TABLE settings ADD COLUMN promptSettings TEXT;
