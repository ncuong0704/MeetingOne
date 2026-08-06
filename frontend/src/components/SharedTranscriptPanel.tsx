'use client';

import { useEffect, useState } from 'react';
import { CapuAPI, SharedTranscriptConfig, TranscriptConfigAPI } from '@/lib/asr';
import {
  DEFAULT_CAPU_CASE_LEVEL,
  DEFAULT_CAPU_PUNCTUATION_LEVEL,
  FALLBACK_PHYSICAL_CORES,
  levelLabel,
} from './asrSettingsConstants';
import GpuSetupGuidance from './GpuSetupGuidance';

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
  const [physicalCores, setPhysicalCores] = useState(FALLBACK_PHYSICAL_CORES);
  const [capuThreads, setCapuThreads] = useState(FALLBACK_PHYSICAL_CORES);
  const [capuPunctuationLevel, setCapuPunctuationLevel] = useState(DEFAULT_CAPU_PUNCTUATION_LEVEL);
  const [capuCaseLevel, setCapuCaseLevel] = useState(DEFAULT_CAPU_CASE_LEVEL);
  const [isSaving, setIsSaving] = useState(false);
  const [saveMessage, setSaveMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!config) return;
    if (typeof config.hotwords === 'string') setHotwords(config.hotwords);
    if (typeof config.capuCpuThreads === 'number') setCapuThreads(config.capuCpuThreads);
    if (typeof config.capuPunctuationLevel === 'number') {
      setCapuPunctuationLevel(config.capuPunctuationLevel);
    }
    if (typeof config.capuCaseLevel === 'number') setCapuCaseLevel(config.capuCaseLevel);
  }, [config]);

  useEffect(() => {
    CapuAPI.getCpuTopology()
      .then(({ physicalCores: cores }) => {
        setPhysicalCores(cores);
        setCapuThreads((prev) => Math.min(prev, cores));
      })
      .catch(() => undefined);
  }, []);

  const handleSave = async () => {
    setIsSaving(true);
    setSaveMessage(null);
    try {
      await TranscriptConfigAPI.saveShared({
        hotwords,
        capuCpuThreads: capuThreads,
        capuPunctuationLevel,
        capuCaseLevel,
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
          Hotwords và CAPU dùng cho cả ghi âm trực tiếp và nhập file. Với ghi âm trực tiếp, dấu
          câu/viết hoa chỉ áp dụng sau khi kết thúc cuộc họp.
        </p>
      </div>

      <GpuSetupGuidance />

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
          className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white font-mono focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
        />
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            Số luồng CPU (thêm dấu câu)
          </label>
          <span className="text-sm font-mono text-gray-900 dark:text-white w-8 text-right">
            {capuThreads}
          </span>
        </div>
        <div className="flex items-center gap-3">
          <span className="text-xs text-gray-400">1</span>
          <input
            type="range"
            min={1}
            max={physicalCores}
            step={1}
            value={capuThreads}
            onChange={(e) => setCapuThreads(Number(e.target.value))}
            disabled={disabled}
            className="flex-1 accent-blue-500 disabled:opacity-50"
          />
          <span className="text-xs text-gray-400">{physicalCores}</span>
        </div>
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            Mức độ thêm dấu
          </label>
          <span className="text-sm font-mono text-gray-900 dark:text-white w-16 text-right">
            {levelLabel(capuPunctuationLevel)}
          </span>
        </div>
        <div className="flex items-center gap-3">
          <span className="text-xs text-gray-400">1</span>
          <input
            type="range"
            min={1}
            max={10}
            step={1}
            value={capuPunctuationLevel}
            onChange={(e) => setCapuPunctuationLevel(Number(e.target.value))}
            disabled={disabled}
            className="flex-1 accent-blue-500 disabled:opacity-50"
          />
          <span className="text-xs text-gray-400">10</span>
        </div>
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            Mức độ tự viết hoa
          </label>
          <span className="text-sm font-mono text-gray-900 dark:text-white w-16 text-right">
            {levelLabel(capuCaseLevel)}
          </span>
        </div>
        <div className="flex items-center gap-3">
          <span className="text-xs text-gray-400">1</span>
          <input
            type="range"
            min={1}
            max={10}
            step={1}
            value={capuCaseLevel}
            onChange={(e) => setCapuCaseLevel(Number(e.target.value))}
            disabled={disabled}
            className="flex-1 accent-blue-500 disabled:opacity-50"
          />
          <span className="text-xs text-gray-400">10</span>
        </div>
      </div>

      <div className="flex items-center gap-3 pt-1">
        <button
          onClick={handleSave}
          disabled={isSaving || disabled}
          className="px-4 py-2 text-sm rounded-md bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white font-medium transition-colors"
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
