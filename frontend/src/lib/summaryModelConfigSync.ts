import { invoke } from '@tauri-apps/api/core';
import type { ModelConfig } from '@/components/ModelSettingsModal';

/**
 * Persist summary model config to DB and broadcast to all listeners via Tauri event.
 * Single source of truth for onboarding, settings, and popup saves.
 */
export async function persistSummaryModelConfig(config: ModelConfig): Promise<void> {
  if (
    config.provider === 'custom-openai' &&
    config.customOpenAIEndpoint?.trim() &&
    config.customOpenAIModel?.trim()
  ) {
    await invoke('api_save_custom_openai_config', {
      endpoint: config.customOpenAIEndpoint.trim(),
      apiKey: config.customOpenAIApiKey?.trim() || null,
      model: config.customOpenAIModel.trim(),
      maxTokens: config.maxTokens ?? null,
      temperature: config.temperature ?? null,
      topP: config.topP ?? null,
    });
  }

  await invoke('api_save_model_config', {
    provider: config.provider,
    model: config.model,
    whisperModel: config.whisperModel,
    apiKey: config.apiKey ?? null,
    ollamaEndpoint: config.ollamaEndpoint ?? null,
    fallbackModelsJson: config.fallbackModels ? JSON.stringify(config.fallbackModels) : null,
  });

  if (config.model) {
    const map = JSON.parse(localStorage.getItem('providerModelMap') || '{}');
    map[config.provider] = config.model;
    localStorage.setItem('providerModelMap', JSON.stringify(map));
  }

  const { emit } = await import('@tauri-apps/api/event');
  await emit('model-config-updated', config);
}
