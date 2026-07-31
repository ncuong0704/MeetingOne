import { invoke } from '@tauri-apps/api/core';
import { isTauriRuntime } from '@/lib/tauriRuntime';

export interface PromptConfig {
  systemPromptFinalTemplate: string;
}

const DEV_PROMPT_SETTINGS_KEY = 'dev_prompt_settings';

const TEMPLATE_MARKDOWN_MARKER = '**MẪU MARKDOWN';

let cachedDefaults: PromptConfig | null = null;

function migratePromptConfig(config: PromptConfig, defaults: PromptConfig): PromptConfig {
  if (config.systemPromptFinalTemplate.includes('{template_markdown}')) {
    return config;
  }

  const defaultText = defaults.systemPromptFinalTemplate;
  const markerIndex = defaultText.indexOf(TEMPLATE_MARKDOWN_MARKER);
  const suffix =
    markerIndex >= 0
      ? defaultText.slice(markerIndex)
      : '\n\n**MẪU MARKDOWN (điền nội dung vào khung bên dưới, giữ nguyên cấu trúc tiêu đề):**\n\n{template_markdown}\n';

  const trimmed = config.systemPromptFinalTemplate.trimEnd();
  return {
    ...config,
    systemPromptFinalTemplate: `${trimmed}\n\n${suffix.trimStart()}`,
  };
}

async function fetchDefaultPromptConfig(): Promise<PromptConfig> {
  const res = await fetch('/dev-prompt-defaults.json', { cache: 'no-store' });
  if (!res.ok) {
    throw new Error('Không tải được prompt mặc định');
  }

  cachedDefaults = await res.json() as PromptConfig;
  return cachedDefaults;
}

function getDevPromptSettings(): PromptConfig | null {
  if (typeof window === 'undefined') return null;
  try {
    const raw = localStorage.getItem(DEV_PROMPT_SETTINGS_KEY);
    return raw ? JSON.parse(raw) as PromptConfig : null;
  } catch {
    return null;
  }
}

function setDevPromptSettings(settings: PromptConfig) {
  localStorage.setItem(DEV_PROMPT_SETTINGS_KEY, JSON.stringify(settings));
}

function clearDevPromptSettings() {
  localStorage.removeItem(DEV_PROMPT_SETTINGS_KEY);
}

/** Built-in defaults (không đọc bản đã lưu của user). */
export async function getDefaultPromptSettings(): Promise<PromptConfig> {
  return fetchDefaultPromptConfig();
}

export async function getPromptSettings(): Promise<PromptConfig> {
  if (isTauriRuntime()) {
    return invoke<PromptConfig>('api_get_prompt_settings');
  }

  const defaults = await fetchDefaultPromptConfig();
  const custom = getDevPromptSettings();
  const base = custom ?? defaults;
  const migrated = migratePromptConfig(base, defaults);

  if (migrated.systemPromptFinalTemplate !== base.systemPromptFinalTemplate) {
    setDevPromptSettings(migrated);
  }

  return migrated;
}

export async function savePromptSettings(settings: PromptConfig): Promise<void> {
  if (isTauriRuntime()) {
    await invoke('api_save_prompt_settings', { settings });
    return;
  }

  setDevPromptSettings(settings);
}

export async function resetPromptSettings(): Promise<PromptConfig> {
  if (isTauriRuntime()) {
    return invoke<PromptConfig>('api_reset_prompt_settings');
  }

  clearDevPromptSettings();
  cachedDefaults = null;
  return fetchDefaultPromptConfig();
}
