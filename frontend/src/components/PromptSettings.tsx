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
      <div className="text-xs text-gray-400 px-3 py-2 border border-gray-200 rounded-md min-h-[200px]">
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
      <div className="flex items-center justify-center py-16 text-gray-400 text-sm">
        Đang tải cài đặt prompt…
      </div>
    );
  }

  if (!config) return null;

  return (
    <div className="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
      <div className="px-5 py-4 flex items-start justify-between gap-4">
        <div className="flex-1 min-w-0 space-y-2">
          <div className="flex items-center gap-2 flex-wrap">
            <h3 className="text-base font-semibold text-gray-900">Prompt AI</h3>
            <span className="text-xs px-1.5 py-0.5 rounded-full bg-blue-50 text-blue-600 border border-blue-100">
              Tất cả provider
            </span>
            {isDirty && (
              <span className="text-xs px-1.5 py-0.5 rounded-full bg-amber-50 text-amber-600 border border-amber-100">
                Đã sửa
              </span>
            )}
          </div>
          <p className="text-sm text-gray-500">
            Bạn có thể chỉnh prompt hệ thống để AI tạo báo cáo cuối cùng theo yêu cầu của bạn, nhưng bạn không được xoá các biến bên dưới:
          </p>
          <ul className="text-sm text-gray-500 space-y-1">
            {PROMPT_PLACEHOLDER_INFO.map(({ token, description }) => (
              <li key={token} className="flex gap-1.5">
                <code className="bg-gray-100 px-1 rounded text-xs shrink-0">{token}</code>
                <span>— {description}</span>
              </li>
            ))}
          </ul>
          <p className="text-xs text-gray-400">
            Các biến trên là của hệ thống AI nên bắt buộc phải giữ nguyên trong prompt.
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
            {isSaving ? 'Đang lưu…' : 'Lưu'}
          </button>
        </div>
      </div>

      <div className="px-5 pb-5 min-h-[200px]">
        <PromptBlockNoteEditor
          key={`systemPromptFinalTemplate-${editorVersion}`}
          value={config.systemPromptFinalTemplate}
          onChange={value => setConfig(prev => prev ? { ...prev, systemPromptFinalTemplate: value } : prev)}
        />
      </div>
    </div>
  );
}
