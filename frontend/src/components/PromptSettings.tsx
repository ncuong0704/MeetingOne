'use client';

import React, { useState, useEffect, useCallback } from 'react';
import dynamic from 'next/dynamic';
import { toast } from 'sonner';
import { RotateCcw, Save } from 'lucide-react';
import {
  getPromptSettings,
  resetPromptSettings,
  savePromptSettings,
  type PromptConfig,
} from '@/services/promptService';

const PROMPT_PLACEHOLDER_INFO = [
  {
    token: '{section_instructions}',
    description: 'Hướng dẫn chi tiết từng mục trong mẫu báo cáo.',
  },
  {
    token: '{template_markdown}',
    description: 'Khung cấu trúc rỗng của mẫu báo cáo.',
  },
  {
    token: '{meeting_datetime}',
    description: 'Thời điểm cuộc họp diễn ra (giờ địa phương).',
  },
  {
    token: '{current_datetime}',
    description: 'Thời điểm tạo báo cáo (giờ địa phương).',
  },
] as const;

const PromptBlockNoteEditor = dynamic(
  () => import('@/components/TemplateSettings/SectionInstructionEditor'),
  {
    ssr: false,
    loading: () => (
      <div className="text-xs text-ink-2 px-3 py-2 border border-rule rounded-md min-h-[80px]">
        Đang tải trình soạn thảo...
      </div>
    ),
  },
);

const btnPrimary =
  'inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-2.5 text-xs font-medium text-primary-foreground hover:bg-primary-hover disabled:opacity-50';
const btnOutline =
  'inline-flex h-8 items-center gap-1.5 rounded-md border border-rule bg-paper-2 px-2.5 text-xs font-medium text-ink-2 hover:bg-secondary hover:text-ink disabled:opacity-50';

export function PromptSettings() {
  const [config, setConfig] = useState<PromptConfig | null>(null);
  const [original, setOriginal] = useState<PromptConfig | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [isResetting, setIsResetting] = useState(false);
  const [editorVersion, setEditorVersion] = useState(0);

  const loadConfig = useCallback(async () => {
    setIsLoading(true);
    try {
      const data = await getPromptSettings();
      setConfig(data);
      setOriginal(data);
      setEditorVersion(v => v + 1);
    } catch (err) {
      toast.error('Không thể tải cài đặt prompt: ' + String(err));
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  const handleSave = async () => {
    if (!config) return;
    setIsSaving(true);
    try {
      await savePromptSettings(config);
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
      const data = await resetPromptSettings();
      setConfig(data);
      setOriginal(data);
      setEditorVersion(v => v + 1);
      toast.success('Đã khôi phục prompt về mặc định');
    } catch (err) {
      toast.error('Khôi phục thất bại: ' + String(err));
    } finally {
      setIsResetting(false);
    }
  };

  const isDirty = Boolean(config && original && JSON.stringify(config) !== JSON.stringify(original));

  if (isLoading) {
    return (
      <div className="max-w-xl space-y-6 animate-pulse">
        <div className="h-28 bg-paper-3 rounded-md" />
        <div className="h-40 bg-paper-3 rounded-md" />
      </div>
    );
  }

  if (!config) return null;

  return (
    <div className="max-w-xl space-y-6">
      <section>
        <div className="flex items-baseline justify-between gap-3">
          <h2 className="text-sm font-semibold text-ink tracking-tight">Prompt hệ thống</h2>
          {isDirty && <span className="text-xs text-amber-600">Đã sửa</span>}
        </div>
        <p className="text-xs text-ink-2 mt-0.5 mb-2">
          Dùng cho mọi nhà cung cấp. Bắt buộc giữ nguyên các biến bên dưới.
        </p>
        <div className="app-surface overflow-hidden">
          <ul className="divide-y divide-rule">
            {PROMPT_PLACEHOLDER_INFO.map(({ token, description }) => (
              <li key={token} className="px-4 py-2.5">
                <code className="font-mono text-xs text-ink">{token}</code>
                <p className="text-xs text-ink-2 mt-0.5 leading-snug">{description}</p>
              </li>
            ))}
          </ul>
        </div>
      </section>

      <section>
        <h2 className="text-sm font-semibold text-ink tracking-tight mb-2">Nội dung</h2>
        <div className="app-surface overflow-hidden px-4 py-3">
          <PromptBlockNoteEditor
            key={`systemPromptFinalTemplate-${editorVersion}`}
            value={config.systemPromptFinalTemplate}
            onChange={value => setConfig(prev => prev ? { ...prev, systemPromptFinalTemplate: value } : prev)}
          />
        </div>
      </section>

      <div className="flex items-center justify-end gap-1.5">
        <button
          type="button"
          onClick={() => void handleResetAll()}
          disabled={isResetting || isSaving}
          className={btnOutline}
        >
          <RotateCcw className="h-3.5 w-3.5" />
          {isResetting ? 'Đang khôi phục...' : 'Khôi phục'}
        </button>
        <button
          type="button"
          onClick={() => void handleSave()}
          disabled={!isDirty || isSaving || isResetting}
          className={btnPrimary}
        >
          <Save className="h-3.5 w-3.5" />
          {isSaving ? 'Đang lưu...' : 'Lưu'}
        </button>
      </div>
    </div>
  );
}
