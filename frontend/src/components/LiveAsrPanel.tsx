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

  const selectClass =
    'w-full h-9 px-3 text-sm rounded-md border border-rule bg-paper-2 text-ink focus:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50';
  const saveClass =
    'inline-flex h-8 items-center rounded-md bg-primary px-2.5 text-xs font-medium text-primary-foreground hover:bg-primary-hover disabled:opacity-50';
  const outlineClass =
    'inline-flex h-8 items-center rounded-md border border-rule bg-paper-2 px-2.5 text-xs font-medium text-ink hover:bg-secondary disabled:opacity-50';

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="text-sm font-medium text-ink">Hotkey người nói</p>
          <p className="mt-0.5 text-xs text-ink-2">Phím 1-9 khi cửa sổ app đang focus.</p>
        </div>
        <button
          type="button"
          onClick={() => setHotkeyOpen(true)}
          disabled={disabled}
          className={outlineClass}
        >
          Cấu hình
        </button>
      </div>
      <SpeakerHotkeyDialog open={hotkeyOpen} onOpenChange={setHotkeyOpen} />

      <div className="space-y-1.5">
        <label className="block text-sm font-medium text-ink">Model ASR</label>
        <select
          value={selectedFamily}
          onChange={(e) => setSelectedFamily(e.target.value as AsrModelFamily)}
          disabled={disabled}
          className={selectClass}
        >
          {ASR_MODELS.map((m) => (
            <option key={m.id} value={m.id}>{m.label}</option>
          ))}
        </select>
      </div>

      <div className="space-y-1.5">
        <label className="block text-sm font-medium text-ink">Biến thể</label>
        <select
          value={effectiveVariant}
          onChange={(e) => setSelectedVariant(e.target.value as ModelVariant)}
          disabled={disabled || availableVariantOptions.length <= 1}
          className={selectClass}
        >
          {availableVariantOptions.map((v) => {
            const size = v.id === 'int8' ? selectedModelInfo?.int8Size : selectedModelInfo?.fullSize;
            return (
              <option key={v.id} value={v.id}>{v.label} ({size})</option>
            );
          })}
        </select>
        <AsrVariantNotice family={selectedFamily} variant={effectiveVariant} path="live" />
        <div className="flex items-center justify-between gap-3">
          <span className="text-xs text-ink-2">
            {currentStatus.isLoaded && 'Đang dùng'}
            {currentStatus.hasFiles && !currentStatus.isLoaded && 'Đã tải, chưa load'}
            {!currentStatus.hasFiles && !downloading && 'Chưa tải'}
          </span>
          {!currentStatus.hasFiles && !downloading && (
            <button type="button" onClick={handleDownload} disabled={disabled} className={saveClass}>
              Tải xuống
            </button>
          )}
        </div>
        {downloading && (
          <div className="space-y-1">
            <div className="flex justify-between text-xs text-ink-2">
              <span>Đang tải</span>
              <span className="font-mono">{progress}%</span>
            </div>
            <div className="h-1.5 w-full rounded-full bg-paper-3">
              <div className="h-1.5 rounded-full bg-primary" style={{ width: `${progress}%` }} />
            </div>
          </div>
        )}
        {error && <p className="text-xs text-destructive">{error}</p>}
      </div>

      <div className="space-y-1.5">
        <label className="block text-sm font-medium text-ink">Phương pháp giải mã</label>
        <div className="flex gap-1.5">
          {(['greedy_search', 'modified_beam_search'] as DecodingMethod[]).map((m) => (
            <button
              key={m}
              type="button"
              onClick={() => setDecodingMethod(m)}
              disabled={disabled}
              className={`h-8 px-2.5 text-xs rounded-md border disabled:opacity-50 ${
                decodingMethod === m
                  ? 'border-primary bg-paper text-ink'
                  : 'border-rule text-ink-2 hover:bg-secondary'
              }`}
            >
              {m}
            </button>
          ))}
        </div>
      </div>

      {decodingMethod === 'modified_beam_search' && (
        <div className="space-y-1.5">
          <div className="flex items-center justify-between">
            <label className="text-sm font-medium text-ink">Số đường giải mã</label>
            <span className="text-xs font-mono text-ink-2">{numActivePaths}</span>
          </div>
          <input
            type="range"
            min={1}
            max={100}
            value={numActivePaths}
            onChange={(e) => setNumActivePaths(Number(e.target.value))}
            disabled={disabled}
            className="w-full accent-primary disabled:opacity-50"
          />
        </div>
      )}

      <div className="space-y-1.5">
        <div className="flex items-center justify-between">
          <label className="text-sm font-medium text-ink">Độ dài tối đa mỗi đoạn</label>
          <span className="text-xs font-mono text-ink-2">{maxSegmentSeconds}s</span>
        </div>
        <input
          type="range"
          min={MIN_MAX_SEGMENT_SECONDS}
          max={MAX_MAX_SEGMENT_SECONDS}
          value={maxSegmentSeconds}
          onChange={(e) => setMaxSegmentSeconds(Number(e.target.value))}
          disabled={disabled}
          className="w-full accent-primary disabled:opacity-50"
        />
      </div>

      <div className="flex items-center gap-3 pt-0.5">
        <button type="button" onClick={handleSave} disabled={isSaving || disabled} className={saveClass}>
          {isSaving ? 'Đang lưu...' : 'Lưu'}
        </button>
        {saveMessage && (
          <span className={`text-xs ${saveMessage.startsWith('Lỗi') ? 'text-destructive' : 'text-ink-2'}`}>
            {saveMessage}
          </span>
        )}
      </div>
    </div>
  );
}
