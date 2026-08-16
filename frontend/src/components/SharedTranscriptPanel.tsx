'use client';

import { useEffect, useState } from 'react';
import { SharedTranscriptConfig, TranscriptConfigAPI } from '@/lib/asr';
import {
  DEFAULT_CAPU_CASE_LEVEL,
  DEFAULT_CAPU_PUNCTUATION_LEVEL,
  FIXED_CAPU_CPU_THREADS,
} from './asrSettingsConstants';

interface SharedTranscriptPanelProps {
  config?: SharedTranscriptConfig | null;
  disabled?: boolean;
  onSaved?: () => void;
}

export default function SharedTranscriptPanel({
  config,
  disabled = false,
  onSaved,
}: SharedTranscriptPanelProps) {
  const [hotwords, setHotwords] = useState('');
  const [isSaving, setIsSaving] = useState(false);
  const [saveMessage, setSaveMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!config) return;
    if (typeof config.hotwords === 'string') setHotwords(config.hotwords);
  }, [config]);

  const handleSave = async () => {
    setIsSaving(true);
    setSaveMessage(null);
    try {
      await TranscriptConfigAPI.saveShared({
        hotwords,
        capuCpuThreads: FIXED_CAPU_CPU_THREADS,
        capuPunctuationLevel: DEFAULT_CAPU_PUNCTUATION_LEVEL,
        capuCaseLevel: DEFAULT_CAPU_CASE_LEVEL,
        diarizationEnabled: config?.diarizationEnabled ?? false,
        diarizationNumSpeakers: config?.diarizationNumSpeakers ?? null,
      });
      setSaveMessage('Đã lưu cấu hình chung');
      onSaved?.();
      setTimeout(() => setSaveMessage(null), 3000);
    } catch (e) {
      setSaveMessage(`Lỗi: ${String(e)}`);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <section>
      <h2 className="text-sm font-semibold text-ink tracking-tight">Từ khóa</h2>
      <p className="text-xs text-ink-2 mt-0.5 mb-2">
        Dùng cho cả ghi trực tiếp và nhập file. Mỗi dòng một cụm; có thể thêm trọng số
        <span className="font-mono"> CỤM TỪ :2.5</span>. Dòng bắt đầu bằng
        <span className="font-mono"> #</span> là ghi chú.
      </p>
      <div className="app-surface overflow-hidden px-4 py-3 space-y-3">
        <textarea
          value={hotwords}
          onChange={(e) => setHotwords(e.target.value)}
          disabled={disabled}
          rows={5}
          placeholder={'ỦY BAN NHÂN DÂN :2.5\nCHUYỂN ĐỔI SỐ\n# Tên riêng\nANH MINH'}
          className="w-full px-3 py-2 text-sm font-mono rounded-md border border-rule bg-paper-2 text-ink placeholder:text-ink-2 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
        />
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={handleSave}
            disabled={isSaving || disabled}
            className="inline-flex h-8 items-center rounded-md bg-primary px-2.5 text-xs font-medium text-primary-foreground hover:bg-primary-hover disabled:opacity-50"
          >
            {isSaving ? 'Đang lưu...' : 'Lưu'}
          </button>
          {saveMessage && (
            <span
              className={`text-xs ${
                saveMessage.startsWith('Lỗi') ? 'text-destructive' : 'text-ink-2'
              }`}
            >
              {saveMessage}
            </span>
          )}
        </div>
      </div>
    </section>
  );
}
