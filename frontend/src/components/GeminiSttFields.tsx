'use client';

import { useEffect, useState } from 'react';
import { SttProvider, TranscriptConfigAPI } from '@/lib/asr';

interface GeminiSttFieldsProps {
  provider: SttProvider;
  onProviderChange: (provider: SttProvider) => void;
  disabled?: boolean;
}

export default function GeminiSttFields({
  provider,
  onProviderChange,
  disabled = false,
}: GeminiSttFieldsProps) {
  const [apiKey, setApiKey] = useState('');
  const [hasOverride, setHasOverride] = useState(false);
  const [isBusy, setIsBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    TranscriptConfigAPI.getTranscriptApiKey('gemini')
      .then((key) => setHasOverride(!!key.trim()))
      .catch(() => setHasOverride(false));
  }, []);

  const saveClass =
    'inline-flex h-8 items-center rounded-md bg-primary px-2.5 text-xs font-medium text-primary-foreground hover:bg-primary-hover disabled:opacity-50';
  const outlineClass =
    'inline-flex h-8 items-center rounded-md border border-rule bg-paper-2 px-2.5 text-xs font-medium text-ink hover:bg-secondary disabled:opacity-50';
  const inputClass =
    'w-full h-9 px-3 text-sm rounded-md border border-rule bg-paper-2 text-ink focus:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50';

  const handleSaveKey = async () => {
    if (!apiKey.trim()) {
      setMessage('Để trống = dùng key LLM. Không ghi đè.');
      return;
    }
    setIsBusy(true);
    setMessage(null);
    try {
      await TranscriptConfigAPI.saveTranscriptApiKey('gemini', apiKey.trim());
      setHasOverride(true);
      setApiKey('');
      setMessage('Đã lưu key riêng');
    } catch (e) {
      setMessage(`Lỗi: ${String(e)}`);
    } finally {
      setIsBusy(false);
    }
  };

  const handleDeleteKey = async () => {
    setIsBusy(true);
    setMessage(null);
    try {
      await TranscriptConfigAPI.deleteTranscriptApiKey('gemini');
      setHasOverride(false);
      setApiKey('');
      setMessage('Đã xóa key riêng');
    } catch (e) {
      setMessage(`Lỗi: ${String(e)}`);
    } finally {
      setIsBusy(false);
    }
  };

  return (
    <div className="space-y-3">
      <div className="space-y-1.5">
        <label className="block text-sm font-medium text-ink">Nguồn nhận dạng</label>
        <div className="flex gap-1.5">
          {([
            { id: 'asr' as const, label: 'Local' },
            { id: 'gemini' as const, label: 'Gemini' },
          ]).map((opt) => (
            <button
              key={opt.id}
              type="button"
              onClick={() => onProviderChange(opt.id)}
              disabled={disabled}
              className={`h-8 px-2.5 text-xs rounded-md border disabled:opacity-50 ${
                provider === opt.id
                  ? 'border-primary bg-paper text-ink'
                  : 'border-rule text-ink-2 hover:bg-secondary'
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>

      {provider === 'gemini' && (
        <div className="space-y-3">
          <p className="text-xs text-ink-2">
            Cảnh báo: âm thanh sẽ được gửi tới Google để nhận dạng.
          </p>
          <div className="space-y-1.5">
            <label className="block text-sm font-medium text-ink">Key riêng (tuỳ chọn)</label>
            <input
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              disabled={disabled || isBusy}
              placeholder="Để trống = dùng key LLM"
              className={inputClass}
              autoComplete="off"
            />
            {hasOverride && (
              <p className="text-xs text-ink-2">Đang dùng key riêng cho Gemini STT.</p>
            )}
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={handleSaveKey}
              disabled={disabled || isBusy}
              className={saveClass}
            >
              Lưu key riêng
            </button>
            {hasOverride && (
              <button
                type="button"
                onClick={handleDeleteKey}
                disabled={disabled || isBusy}
                className={outlineClass}
              >
                Xóa key riêng
              </button>
            )}
          </div>
          {message && (
            <p className={`text-xs ${message.startsWith('Lỗi') ? 'text-destructive' : 'text-ink-2'}`}>
              {message}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
