import { invoke } from '@tauri-apps/api/core';
import { isTauriRuntime } from '@/lib/tauriRuntime';
import type { TemplateInfo } from '@/components/TemplateSettings/types';

const BUILTIN_TEMPLATE_IDS = [
  'daily_standup',
  'standard_meeting',
  'theo_mau_act',
  'theo_mau_act_no_table',
  'project_sync',
  'retrospective',
  'sales_marketing_client_call',
] as const;

const DEV_CUSTOM_TEMPLATES_KEY = 'dev_custom_templates';
const DEV_DEFAULT_TEMPLATE_KEY = 'dev_default_template';
const HIDDEN_TEMPLATES_KEY = 'hidden_templates';

function isBuiltinTemplateId(templateId: string): boolean {
  return (BUILTIN_TEMPLATE_IDS as readonly string[]).includes(templateId);
}

function getHiddenTemplateIds(): Set<string> {
  if (typeof window === 'undefined') return new Set();
  try {
    const ids = JSON.parse(localStorage.getItem(HIDDEN_TEMPLATES_KEY) || '[]') as string[];
    return new Set(ids);
  } catch {
    return new Set();
  }
}

function hideTemplate(templateId: string) {
  const hidden = getHiddenTemplateIds();
  hidden.add(templateId);
  localStorage.setItem(HIDDEN_TEMPLATES_KEY, JSON.stringify([...hidden]));
}

function unhideTemplate(templateId: string) {
  const hidden = getHiddenTemplateIds();
  if (!hidden.has(templateId)) return;
  hidden.delete(templateId);
  localStorage.setItem(HIDDEN_TEMPLATES_KEY, JSON.stringify([...hidden]));
}

/** Gỡ ID mẫu tùy chỉnh đã xóa khỏi hidden — tránh chặn tạo lại cùng ID. */
function pruneStaleHiddenCustomTemplates(existingIds: Set<string>) {
  const hidden = getHiddenTemplateIds();
  if (hidden.size === 0) return;

  let changed = false;
  for (const id of hidden) {
    if (!isBuiltinTemplateId(id) && !existingIds.has(id)) {
      hidden.delete(id);
      changed = true;
    }
  }
  if (changed) {
    localStorage.setItem(HIDDEN_TEMPLATES_KEY, JSON.stringify([...hidden]));
  }
}

function filterHiddenTemplates(templates: TemplateInfo[]): TemplateInfo[] {
  const hidden = getHiddenTemplateIds();
  if (hidden.size === 0) return templates;
  return templates.filter(t => !hidden.has(t.id));
}

function getDevCustomTemplates(): Record<string, string> {
  if (typeof window === 'undefined') return {};
  try {
    return JSON.parse(localStorage.getItem(DEV_CUSTOM_TEMPLATES_KEY) || '{}');
  } catch {
    return {};
  }
}

function setDevCustomTemplates(templates: Record<string, string>) {
  localStorage.setItem(DEV_CUSTOM_TEMPLATES_KEY, JSON.stringify(templates));
}

async function fetchBuiltinTemplateJson(id: string): Promise<string> {
  const res = await fetch(`/dev-templates/${id}.json`);
  if (!res.ok) {
    throw new Error(`Template '${id}' not found`);
  }
  return res.text();
}

async function listBrowserTemplates(): Promise<TemplateInfo[]> {
  const custom = getDevCustomTemplates();
  const customIds = new Set(Object.keys(custom));
  const results: TemplateInfo[] = [];

  for (const id of BUILTIN_TEMPLATE_IDS) {
    const hasCustomOverride = customIds.has(id);
    const jsonStr = hasCustomOverride ? custom[id] : await fetchBuiltinTemplateJson(id);
    const parsed = JSON.parse(jsonStr) as { name: string; description: string };
    results.push({
      id,
      name: parsed.name,
      description: parsed.description,
      is_custom: false,
      has_custom_override: hasCustomOverride,
    });
  }

  for (const id of customIds) {
    if ((BUILTIN_TEMPLATE_IDS as readonly string[]).includes(id)) continue;
    const parsed = JSON.parse(custom[id]) as { name: string; description: string };
    results.push({
      id,
      name: parsed.name,
      description: parsed.description,
      is_custom: true,
      has_custom_override: false,
    });
  }

  return filterHiddenTemplates(results);
}

function collectBrowserTemplateIds(): Set<string> {
  const custom = getDevCustomTemplates();
  return new Set([...BUILTIN_TEMPLATE_IDS, ...Object.keys(custom)]);
}

export async function listTemplates(): Promise<TemplateInfo[]> {
  if (isTauriRuntime()) {
    const list = await invoke<TemplateInfo[]>('api_list_templates');
    pruneStaleHiddenCustomTemplates(new Set(list.map((t) => t.id)));
    return filterHiddenTemplates(list);
  }
  pruneStaleHiddenCustomTemplates(collectBrowserTemplateIds());
  return listBrowserTemplates();
}

export async function getTemplateJson(templateId: string): Promise<string> {
  if (isTauriRuntime()) {
    return invoke<string>('api_get_template_json', { templateId });
  }

  const custom = getDevCustomTemplates();
  if (custom[templateId]) return custom[templateId];
  return fetchBuiltinTemplateJson(templateId);
}

export async function getDefaultTemplate(): Promise<string> {
  if (isTauriRuntime()) {
    return invoke<string>('api_get_default_template');
  }
  return localStorage.getItem(DEV_DEFAULT_TEMPLATE_KEY) || 'theo_mau_act_no_table';
}

export async function setDefaultTemplate(templateId: string): Promise<void> {
  if (isTauriRuntime()) {
    await invoke('api_set_default_template', { templateId });
    return;
  }
  localStorage.setItem(DEV_DEFAULT_TEMPLATE_KEY, templateId);
}

export async function saveCustomTemplate(templateId: string, templateJson: string): Promise<void> {
  if (isTauriRuntime()) {
    await invoke('api_save_custom_template', { templateId, templateJson });
    unhideTemplate(templateId);
    return;
  }

  const custom = getDevCustomTemplates();
  custom[templateId] = templateJson;
  setDevCustomTemplates(custom);
  unhideTemplate(templateId);
}

export async function deleteCustomTemplate(templateId: string): Promise<void> {
  if (isTauriRuntime()) {
    try {
      await invoke('api_delete_custom_template', { templateId });
      return;
    } catch (error) {
      const message = String(error);
      const isBuiltinHideCase =
        isBuiltinTemplateId(templateId) &&
        (message.includes('built-in') || message.includes('mặc định') || message.includes('not found'));
      if (isBuiltinHideCase) {
        hideTemplate(templateId);
        return;
      }
      throw error;
    }
  }

  const custom = getDevCustomTemplates();
  if (custom[templateId]) {
    delete custom[templateId];
    setDevCustomTemplates(custom);
    return;
  }

  if (isBuiltinTemplateId(templateId)) {
    hideTemplate(templateId);
  }
}
