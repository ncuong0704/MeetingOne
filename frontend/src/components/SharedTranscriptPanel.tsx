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
    <div className="space-y-4 mt-6 pt-6 border-t border-gray-100 dark:border-gray-700">
      <div>
        <h4 className="text-sm font-semibold text-gray-900 dark:text-white">Cấu hình chung</h4>
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-1">
          Hotwords dùng cho cả ghi âm trực tiếp và nhập file. Với ghi âm trực tiếp, dấu câu/viết hoa
          chỉ áp dụng sau khi kết thúc cuộc họp.
        </p>
      </div>

      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          Từ khóa ưu tiên (tên riêng, thuật ngữ chuyên ngành)
        </label>
        <p className="text-xs text-gray-500 dark:text-gray-400">
          Mỗi dòng một cụm từ. Có thể thêm trọng số bằng cú pháp{' '}
          <code className="px-1 rounded bg-gray-100 dark:bg-gray-700">CỤM TỪ :2.5</code>. Dòng bắt
          đầu bằng <code className="px-1 rounded bg-gray-100 dark:bg-gray-700">#</code> là ghi chú.
        </p>
        <textarea
          value={hotwords}
          onChange={(e) => setHotwords(e.target.value)}
          disabled={disabled}
          rows={6}
          placeholder={'ỦY BAN NHÂN DÂN :2.5\nCHUYỂN ĐỔI SỐ\n# Tên riêng\nANH MINH'}
          className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white font-mono focus:outline-none focus:ring-2 focus-visible:ring-ring disabled:opacity-50"
        />
      </div>

      <div className="flex items-center gap-3 pt-1">
        <button
          onClick={handleSave}
          disabled={isSaving || disabled}
          className="px-4 py-2 text-sm rounded-md bg-primary hover:bg-primary-hover disabled:opacity-50 text-primary-foreground font-medium transition-colors"
        >
          {isSaving ? 'Đang lưu...' : 'Lưu cấu hình chung'}
        </button>
        {saveMessage && (
          <span
            className={`text-xs ${
              saveMessage.startsWith('Lỗi')
                ? 'text-red-500'
                : 'text-green-600 dark:text-green-400'
            }`}
          >
            {saveMessage}
          </span>
        )}
      </div>
    </div>
  );
}
