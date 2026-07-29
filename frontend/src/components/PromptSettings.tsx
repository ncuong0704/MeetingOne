'use client';

import React, { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { RotateCcw, Save, ChevronDown, ChevronUp } from 'lucide-react';

interface PromptConfig {
  systemPromptFinalTemplate: string;
}

interface PromptField {
  key: keyof PromptConfig;
  label: string;
  description: string;
  badge?: string;
  placeholders?: string[];
}

const PROMPT_FIELDS: PromptField[] = [
  {
    key: 'systemPromptFinalTemplate',
    label: 'System Prompt — Tạo báo cáo cuối cùng',
    description: 'Hướng dẫn tổng thể để tạo báo cáo theo mẫu. Áp dụng cho tất cả các nhà cung cấp AI.',
    badge: 'Tất cả provider',
    placeholders: ['{section_instructions}', '{template_markdown}', '{meeting_datetime}', '{current_datetime}'],
  },
];

export function PromptSettings() {
  const [config, setConfig] = useState<PromptConfig | null>(null);
  const [original, setOriginal] = useState<PromptConfig | null>(null);
  const [defaults, setDefaults] = useState<PromptConfig | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [isResetting, setIsResetting] = useState(false);
  const [expanded, setExpanded] = useState<Record<keyof PromptConfig, boolean>>({
    systemPromptFinalTemplate: true,
  });

  const loadConfig = useCallback(async () => {
    setIsLoading(true);
    try {
      const data = await invoke<PromptConfig>('api_get_prompt_settings');
      setConfig(data);
      setOriginal(data);
      if (!defaults) setDefaults(data);
    } catch (err) {
      toast.error('Không thể tải cài đặt prompt: ' + String(err));
    } finally {
      setIsLoading(false);
    }
  }, [defaults]);

  useEffect(() => {
    loadConfig();
  }, []);

  const handleFieldChange = (key: keyof PromptConfig, value: string) => {
    setConfig(prev => prev ? { ...prev, [key]: value } : prev);
  };

  const handleResetField = (key: keyof PromptConfig) => {
    if (!defaults) return;
    setConfig(prev => prev ? { ...prev, [key]: defaults[key] } : prev);
  };

  const handleSave = async () => {
    if (!config) return;
    setIsSaving(true);
    try {
      await invoke('api_save_prompt_settings', { settings: config });
      setOriginal(config);
      toast.success('Đã lưu cài đặt prompt');
    } catch (err) {
      toast.error('Lưu thất bại: ' + String(err));
    } finally {
      setIsSaving(false);
    }
  };

  const handleResetAll = async () => {
    setIsResetting(true);
    try {
      const data = await invoke<PromptConfig>('api_reset_prompt_settings');
      setConfig(data);
      setOriginal(data);
      setDefaults(data);
      toast.success('Đã khôi phục tất cả prompt về mặc định');
    } catch (err) {
      toast.error('Khôi phục thất bại: ' + String(err));
    } finally {
      setIsResetting(false);
    }
  };

  const isDirty = config && original && JSON.stringify(config) !== JSON.stringify(original);

  const toggleExpand = (key: keyof PromptConfig) => {
    setExpanded(prev => ({ ...prev, [key]: !prev[key] }));
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-16 text-gray-400 text-sm">
        Đang tải cài đặt prompt…
      </div>
    );
  }

  if (!config) return null;

  return (
    <div className="space-y-4">
      <div className="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
        <div className="px-5 py-4 border-b border-gray-50 flex items-start justify-between gap-4">
          <div>
            <h3 className="text-base font-semibold text-gray-900">Prompt AI</h3>
            <p className="text-sm text-gray-500 mt-1">
              Tùy chỉnh prompt hệ thống khi AI tạo báo cáo cuối cùng theo mẫu.
              Các token placeholder (ví dụ: <code className="bg-gray-100 px-1 rounded text-xs">{'{template_markdown}'}</code>) phải được giữ nguyên.
            </p>
          </div>
          <div className="flex items-center gap-2 shrink-0">
            <button
              onClick={handleResetAll}
              disabled={isResetting || isSaving}
              className="flex items-center gap-1.5 px-3 py-1.5 text-sm text-gray-600 border border-gray-200 rounded-lg hover:bg-gray-50 disabled:opacity-50 transition-colors"
            >
              <RotateCcw className="w-3.5 h-3.5" />
              {isResetting ? 'Đang khôi phục…' : 'Khôi phục mặc định'}
            </button>
            <button
              onClick={handleSave}
              disabled={!isDirty || isSaving || isResetting}
              className="flex items-center gap-1.5 px-3 py-1.5 text-sm text-white bg-[#16478e] rounded-lg hover:bg-[#123d7a] disabled:opacity-40 transition-colors"
            >
              <Save className="w-3.5 h-3.5" />
              {isSaving ? 'Đang lưu…' : 'Lưu tất cả'}
            </button>
          </div>
        </div>

        <div className="divide-y divide-gray-50">
          {PROMPT_FIELDS.map((field) => {
            const isExpanded = expanded[field.key];
            const isFieldDirty = defaults && config[field.key] !== defaults[field.key];

            return (
              <div key={field.key} className="px-5 py-4">
                <div className="flex items-start justify-between gap-3 mb-2">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="text-sm font-medium text-gray-800">{field.label}</span>
                      {field.badge && (
                        <span className="text-xs px-1.5 py-0.5 rounded-full bg-blue-50 text-blue-600 border border-blue-100">
                          {field.badge}
                        </span>
                      )}
                      {isFieldDirty && (
                        <span className="text-xs px-1.5 py-0.5 rounded-full bg-amber-50 text-amber-600 border border-amber-100">
                          Đã sửa
                        </span>
                      )}
                    </div>
                    <p className="text-xs text-gray-500 mt-0.5">{field.description}</p>
                    {field.placeholders && (
                      <p className="text-xs text-gray-400 mt-0.5">
                        Placeholder bắt buộc:{' '}
                        {field.placeholders.map(p => (
                          <code key={p} className="bg-gray-100 px-1 rounded mr-1">{p}</code>
                        ))}
                      </p>
                    )}
                  </div>
                  <div className="flex items-center gap-1.5 shrink-0">
                    {isFieldDirty && (
                      <button
                        onClick={() => handleResetField(field.key)}
                        className="text-xs text-gray-500 hover:text-gray-700 px-2 py-1 rounded border border-gray-200 hover:bg-gray-50 transition-colors"
                        title="Khôi phục về mặc định"
                      >
                        <RotateCcw className="w-3 h-3" />
                      </button>
                    )}
                    <button
                      onClick={() => toggleExpand(field.key)}
                      className="text-gray-400 hover:text-gray-600 p-1 rounded transition-colors"
                    >
                      {isExpanded ? <ChevronUp className="w-4 h-4" /> : <ChevronDown className="w-4 h-4" />}
                    </button>
                  </div>
                </div>

                {isExpanded ? (
                  <textarea
                    value={config[field.key]}
                    onChange={e => handleFieldChange(field.key, e.target.value)}
                    className="w-full min-h-[200px] p-3 text-xs font-mono bg-gray-50 border border-gray-200 rounded-lg resize-y focus:outline-none focus:ring-2 focus:ring-blue-200 focus:border-blue-300 transition-colors"
                    spellCheck={false}
                  />
                ) : (
                  <div
                    onClick={() => toggleExpand(field.key)}
                    className="cursor-pointer p-3 bg-gray-50 border border-gray-100 rounded-lg"
                  >
                    <p className="text-xs font-mono text-gray-500 line-clamp-2 whitespace-pre-wrap">
                      {config[field.key].trim()}
                    </p>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
