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
      <div className="text-xs text-ink-2 px-3 py-2 border border-rule rounded-md min-h-[200px] bg-paper">
        Đang tải trình soạn thảo...
      </div>
    ),
  },
);

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
  }, []);

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

  const isDirty = config && original && JSON.stringify(config) !== JSON.stringify(original);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-16 text-sm text-ink-2">
        Đang tải cài đặt prompt…
      </div>
    );
  }

  if (!config) return null;

  return (
    <div className="overflow-hidden rounded-md border border-rule bg-paper-2">
      <div className="flex items-center justify-between gap-3 border-b border-rule px-5 h-12">
        <div className="flex items-center gap-2 min-w-0">
          <h3 className="text-sm font-semibold text-ink tracking-tight">Prompt hệ thống</h3>
          <span className="text-[11px] px-1.5 py-0.5 rounded-md bg-primary/10 text-primary border border-primary/20">
            Tất cả provider
          </span>
          {isDirty && (
            <span className="text-[11px] px-1.5 py-0.5 rounded-md bg-amber-50 text-amber-700 border border-amber-200">
              Đã sửa
            </span>
          )}
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <button
            onClick={handleResetAll}
            disabled={isResetting || isSaving}
            className="flex items-center gap-1.5 px-3 py-1.5 text-sm text-ink-2 border border-rule rounded-md hover:bg-secondary disabled:opacity-50 transition-colors"
          >
            <RotateCcw className="w-3.5 h-3.5" />
            {isResetting ? 'Đang khôi phục…' : 'Khôi phục mặc định'}
          </button>
          <button
            onClick={handleSave}
            disabled={!isDirty || isSaving || isResetting}
            className="flex items-center gap-1.5 px-3 py-1.5 text-sm text-primary-foreground bg-primary rounded-md hover:bg-primary-hover disabled:opacity-40 transition-colors"
          >
            <Save className="w-3.5 h-3.5" />
            {isSaving ? 'Đang lưu…' : 'Lưu'}
          </button>
        </div>
      </div>

      <div className="px-5 py-2.5 border-b border-rule">
        <p className="text-xs text-ink-2 mb-1.5">
          Có thể chỉnh prompt, nhưng bắt buộc giữ nguyên các biến hệ thống:
        </p>
        <div className="flex flex-col gap-1">
          {PROMPT_PLACEHOLDER_INFO.map(({ token, description }) => (
            <div key={token} className="flex items-baseline gap-2 min-w-0">
              <code className="font-mono text-[11px] text-ink shrink-0">{token}</code>
              <span className="text-xs text-ink-2 leading-snug">{description}</span>
            </div>
          ))}
        </div>
      </div>

      <div className="px-5 py-4 min-h-[200px]">
        <PromptBlockNoteEditor
          key={`systemPromptFinalTemplate-${editorVersion}`}
          value={config.systemPromptFinalTemplate}
          onChange={value => setConfig(prev => prev ? { ...prev, systemPromptFinalTemplate: value } : prev)}
        />
      </div>
    </div>
  );
}
