/**
 * Model defaults — keep STT + summary provider constants in one place.
 * STT: ZipFormer only (hynt/Zipformer-30M-RNNT-6000h).
 */

import type { ModelConfig } from '@/components/ModelSettingsModal';

/** HuggingFace display name for the only supported STT model. */
export const ZIPFORMER_MODEL_DISPLAY_NAME = 'hynt/Zipformer-30M-RNNT-6000h';

/** Internal model id stored in DB (matches Rust ZIPFORMER_MODEL_NAME). */
export const ZIPFORMER_MODEL_ID = 'zipformer-vi-30m';

/** Default Custom OpenAI-compatible endpoint (Google Gemini OpenAI API). */
export const DEFAULT_CUSTOM_OPENAI_ENDPOINT =
  'https://generativelanguage.googleapis.com/v1beta/openai';

/** Default model for Custom OpenAI summary provider. */
export const DEFAULT_CUSTOM_OPENAI_MODEL = 'gemini-3.1-flash-lite';

export function createDefaultSummaryModelConfig(): ModelConfig {
  return {
    provider: 'custom-openai',
    model: DEFAULT_CUSTOM_OPENAI_MODEL,
    apiKey: null,
    customOpenAIEndpoint: DEFAULT_CUSTOM_OPENAI_ENDPOINT,
    customOpenAIModel: DEFAULT_CUSTOM_OPENAI_MODEL,
    customOpenAIApiKey: null,
  };
}

export function createDefaultTranscriptModelConfig() {
  return {
    provider: 'zipformer' as const,
    model: ZIPFORMER_MODEL_ID,
    apiKey: null as string | null,
  };
}
