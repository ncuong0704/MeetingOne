/**
 * Model defaults — keep STT + summary provider constants in one place.
 * STT: ASR engine (ZipFormer 30M default, Gipformer 65M optional).
 */

import type { ModelConfig } from '@/components/ModelSettingsModal';

/** HuggingFace display name for the default STT model. */
export const ZIPFORMER_MODEL_DISPLAY_NAME = 'hynt/Zipformer-30M-RNNT-6000h';

/** Internal model id stored in DB (matches Rust ZIPFORMER_MODEL_NAME). */
export const ZIPFORMER_MODEL_ID = 'zipformer-vi-30m';

/** Internal model id for Gipformer 65M (matches Rust GIPFORMER_MODEL_NAME). */
export const GIPFORMER_MODEL_ID = 'gipformer-65m-rnnt';

/** Internal model id for Sherpa-ONNX Zipformer VI 2025 (matches Rust SHERPA_VI_2025_MODEL_NAME). */
export const SHERPA_VI_2025_MODEL_ID = 'sherpa-onnx-zipformer-vi-2025-04-20';

/** Internal model id for NghiMe/NghiASR (matches Rust NGHI_ASR_MODEL_NAME). */
export const NGHI_ASR_MODEL_ID = 'nghi-asr';

/** Default Custom OpenAI-compatible endpoint (Google Gemini OpenAI API). */
export const DEFAULT_CUSTOM_OPENAI_ENDPOINT =
  'https://generativelanguage.googleapis.com/v1beta/openai';

/** Default model for Custom OpenAI summary provider. */
export const DEFAULT_CUSTOM_OPENAI_MODEL = 'gemini-3.6-flash';

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
    provider: 'asr' as const,
    model: ZIPFORMER_MODEL_ID,
    apiKey: null as string | null,
  };
}
