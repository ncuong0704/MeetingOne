'use client';

import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useState } from 'react';
import { SpeakerHotkeyDialog } from '@/components/SpeakerHotkeyDialog';
import {
  ASR_MODELS,
  AsrAPI,
  AsrModelFamily,
  DecodingMethod,
  LiveAsrConfig,
  ModelVariant,
  TranscriptConfigAPI,
  VariantStatus,
} from '@/lib/asr';
import {
  DEFAULT_DECODING,
  DEFAULT_LIVE_FAMILY,
  DEFAULT_MAX_SEGMENT_SECONDS,
  DEFAULT_PATHS,
  DEFAULT_VARIANT,
  MAX_MAX_SEGMENT_SECONDS,
  MIN_MAX_SEGMENT_SECONDS,
  parseAsrFamily,
  resolveVariantForFamily,
  VARIANT_OPTIONS,
} from './asrSettingsConstants';
import { AsrVariantNotice } from './AsrVariantNotice';

interface LiveAsrPanelProps {
  config?: LiveAsrConfig | null;
  disabled?: boolean;
  onSaved?: () => void;
}

interface DownloadState {
  downloading: boolean;
  progress: number;
  error: string | null;
}

export default function LiveAsrPanel({ config, disabled = false, onSaved }: LiveAsrPanelProps) {
  const [selectedFamily, setSelectedFamily] = useState<AsrModelFamily>(DEFAULT_LIVE_FAMILY);
  const [selectedVariant, setSelectedVariant] = useState<ModelVariant>(DEFAULT_VARIANT);
  const [decodingMethod, setDecodingMethod] = useState<DecodingMethod>(DEFAULT_DECODING);
  const [numActivePaths, setNumActivePaths] = useState(DEFAULT_PATHS);
  const [maxSegmentSeconds, setMaxSegmentSeconds] = useState(DEFAULT_MAX_SEGMENT_SECONDS);
  const [variantStatuses, setVariantStatuses] = useState<Record<ModelVariant, VariantStatus>>({
    int8: { hasFiles: false, isLoaded: false },
    full: { hasFiles: false, isLoaded: false },
  });
  const [downloadState, setDownloadState] = useState<DownloadState>({
    downloading: false,
    progress: 0,
    error: null,
  });
  const [isSaving, setIsSaving] = useState(false);
  const [saveMessage, setSaveMessage] = useState<string | null>(null);
  const [hotkeyOpen, setHotkeyOpen] = useState(false);

  const selectedModelInfo = ASR_MODELS.find((m) => m.id === selectedFamily);
  const effectiveVariant = resolveVariantForFamily(selectedFamily, selectedVariant);
  const availableVariantOptions = VARIANT_OPTIONS.filter((v) =>
    selectedModelInfo ? selectedModelInfo.availableVariants.includes(v.id) : true
  );

  const refreshAllVariantStatuses = useCallback(async (family: AsrModelFamily) => {
    const modelInfo = ASR_MODELS.find((m) => m.id === family);
    const results: Record<ModelVariant, VariantStatus> = {
      int8: { hasFiles: false, isLoaded: false },
      full: { hasFiles: false, isLoaded: false },
    };
    for (const variant of VARIANT_OPTIONS) {
      if (modelInfo && !modelInfo.availableVariants.includes(variant.id)) continue;
      try {
        results[variant.id] = await AsrAPI.getVariantStatus(family, variant.id);
      } catch {
        // keep defaults
      }
    }
    setVariantStatuses(results);
  }, []);

  useEffect(() => {
    if (!config) return;
    const family = parseAsrFamily(config.model);
    setSelectedFamily(family);
    setSelectedVariant(resolveVariantForFamily(family, config.asrVariant ?? DEFAULT_VARIANT));
    if (config.decodingMethod === 'greedy_search' || config.decodingMethod === 'modified_beam_search') {
      setDecodingMethod(config.decodingMethod);
    }
    if (typeof config.numActivePaths === 'number') setNumActivePaths(config.numActivePaths);
    if (typeof config.maxSegmentSeconds === 'number') {
      setMaxSegmentSeconds(
        Math.min(
          MAX_MAX_SEGMENT_SECONDS,
          Math.max(MIN_MAX_SEGMENT_SECONDS, config.maxSegmentSeconds)
        )
      );
    }
  }, [config]);

  useEffect(() => {
    AsrAPI.init().catch(console.error);
    refreshAllVariantStatuses(selectedFamily);
  }, [selectedFamily, refreshAllVariantStatuses]);

  useEffect(() => {
    const unlistenProgress = listen<{ progress: number }>('asr-model-download-progress', (event) => {
      setDownloadState((prev) => ({ ...prev, progress: event.payload.progress }));
    });
    const unlistenComplete = listen('asr-model-download-complete', () => {
      setDownloadState({ downloading: false, progress: 100, error: null });
      refreshAllVariantStatuses(selectedFamily);
    });
    const unlistenError = listen<{ error: string }>('asr-model-download-error', (event) => {
      setDownloadState((prev) => ({ ...prev, downloading: false, error: event.payload.error }));
    });
    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenComplete.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, [selectedFamily, refreshAllVariantStatuses]);

  const handleDownload = async () => {
    setDownloadState({ downloading: true, progress: 0, error: null });
    try {
      await AsrAPI.downloadModel(selectedFamily, effectiveVariant);
    } catch (e) {
      setDownloadState((prev) => ({ ...prev, downloading: false, error: String(e) }));
    }
  };

  const handleSave = async () => {
    setIsSaving(true);
    setSaveMessage(null);
    const payload: LiveAsrConfig = {
      model: selectedFamily,
      asrVariant: effectiveVariant,
      decodingMethod,
      numActivePaths,
      maxSegmentSeconds,
    };
    try {
      await TranscriptConfigAPI.saveLive(payload);
      const freshStatus = await AsrAPI.getVariantStatus(selectedFamily, effectiveVariant);
      setVariantStatuses((prev) => ({ ...prev, [effectiveVariant]: freshStatus }));
      if (freshStatus.hasFiles) {
        await refreshAllVariantStatuses(selectedFamily);
        setSaveMessage('Đã lưu cấu hình ghi âm trực tiếp');
      } else {
        setSaveMessage('Đã lưu. Tải model trước khi ghi âm.');
      }
      onSaved?.();
      setTimeout(() => setSaveMessage(null), 3000);
    } catch (e) {
      setSaveMessage(`Lỗi: ${String(e)}`);
    } finally {
      setIsSaving(false);
    }
  };

  const currentStatus = variantStatuses[effectiveVariant];
  const { downloading, progress, error } = downloadState;

  return (
    <div className="space-y-4">
      <p className="text-xs text-gray-500 dark:text-gray-400">
        Dấu câu/viết hoa (CAPU) chỉ áp dụng sau khi kết thúc cuộc họp.
      </p>
      <div>
        <button
          type="button"
          onClick={() => setHotkeyOpen(true)}
          disabled={disabled}
          className="px-3 py-1.5 text-xs rounded-md border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-200 hover:bg-gray-50 dark:hover:bg-gray-800 disabled:opacity-50"
        >
          Cấu hình hotkey người nói
        </button>
        <p className="mt-1 text-xs text-gray-400">
          Lúc ghi âm, bấm phím 1–9 (cửa sổ app đang focus) để gán người đang nói.
        </p>
      </div>
      <SpeakerHotkeyDialog open={hotkeyOpen} onOpenChange={setHotkeyOpen} />

      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          Model ASR (ghi âm trực tiếp)
        </label>
        <select
          value={selectedFamily}
          onChange={(e) => setSelectedFamily(e.target.value as AsrModelFamily)}
          disabled={disabled}
          className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
        >
          {ASR_MODELS.map((m) => (
            <option key={m.id} value={m.id}>{m.label}</option>
          ))}
        </select>
      </div>

      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">Biến thể</label>
        <select
          value={effectiveVariant}
          onChange={(e) => setSelectedVariant(e.target.value as ModelVariant)}
          disabled={disabled || availableVariantOptions.length <= 1}
          className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
        >
          {availableVariantOptions.map((v) => {
            const size = v.id === 'int8' ? selectedModelInfo?.int8Size : selectedModelInfo?.fullSize;
            return (
              <option key={v.id} value={v.id}>{v.label} ({size})</option>
            );
          })}
        </select>
        <AsrVariantNotice family={selectedFamily} variant={effectiveVariant} path="live" />
        <div className="flex items-center justify-between p-3 rounded-lg border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800">
          <div className="flex items-center gap-2">
            <span className="text-base">🇻🇳</span>
            <div>
              {currentStatus.isLoaded && (
                <span className="text-xs text-green-600 dark:text-green-400 font-medium">✓ Đang dùng</span>
              )}
              {currentStatus.hasFiles && !currentStatus.isLoaded && (
                <span className="text-xs text-yellow-600 dark:text-yellow-400">Đã tải, chưa load</span>
              )}
              {!currentStatus.hasFiles && !downloading && (
                <span className="text-xs text-gray-400">Chưa tải</span>
              )}
            </div>
          </div>
          {!currentStatus.hasFiles && !downloading && (
            <button
              onClick={handleDownload}
              disabled={disabled}
              className="px-3 py-1.5 text-xs rounded-md bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white font-medium"
            >
              Tải xuống
            </button>
          )}
        </div>
        {downloading && (
          <div className="space-y-1">
            <div className="flex justify-between text-xs text-gray-500">
              <span>Đang tải xuống...</span>
              <span>{progress}%</span>
            </div>
            <div className="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-1.5">
              <div className="bg-blue-500 h-1.5 rounded-full transition-all" style={{ width: `${progress}%` }} />
            </div>
          </div>
        )}
        {error && <p className="text-xs text-red-500">{error}</p>}
      </div>

      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          Phương pháp giải mã
        </label>
        <div className="flex gap-2">
          {(['greedy_search', 'modified_beam_search'] as DecodingMethod[]).map((m) => (
            <button
              key={m}
              onClick={() => setDecodingMethod(m)}
              disabled={disabled}
              className={`px-3 py-1.5 text-sm rounded-md border disabled:opacity-50 ${
                decodingMethod === m
                  ? 'border-orange-500 bg-orange-50 dark:bg-orange-900/20 text-orange-700'
                  : 'border-gray-300 dark:border-gray-600 text-gray-600'
              }`}
            >
              {m}
            </button>
          ))}
        </div>
      </div>

      {decodingMethod === 'modified_beam_search' && (
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
              Số đường giải mã
            </label>
            <span className="text-sm font-mono">{numActivePaths}</span>
          </div>
          <input
            type="range"
            min={1}
            max={100}
            value={numActivePaths}
            onChange={(e) => setNumActivePaths(Number(e.target.value))}
            disabled={disabled}
            className="w-full accent-orange-500 disabled:opacity-50"
          />
        </div>
      )}

      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            Độ dài tối đa mỗi đoạn
          </label>
          <span className="text-sm font-mono">{maxSegmentSeconds}s</span>
        </div>
        <input
          type="range"
          min={MIN_MAX_SEGMENT_SECONDS}
          max={MAX_MAX_SEGMENT_SECONDS}
          value={maxSegmentSeconds}
          onChange={(e) => setMaxSegmentSeconds(Number(e.target.value))}
          disabled={disabled}
          className="w-full accent-blue-500 disabled:opacity-50"
        />
      </div>

      <div className="flex items-center gap-3 pt-1">
        <button
          onClick={handleSave}
          disabled={isSaving || disabled}
          className="px-4 py-2 text-sm rounded-md bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white font-medium"
        >
          {isSaving ? 'Đang lưu...' : 'Lưu cấu hình ghi trực tiếp'}
        </button>
        {saveMessage && (
          <span className={`text-xs ${saveMessage.startsWith('Lỗi') ? 'text-red-500' : 'text-green-600'}`}>
            {saveMessage}
          </span>
        )}
      </div>
    </div>
  );
}
