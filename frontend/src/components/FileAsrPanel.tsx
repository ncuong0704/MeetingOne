'use client';

import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useRef, useState } from 'react';
import GeminiSttFields, { GeminiSttFieldsHandle } from '@/components/GeminiSttFields';
import {
  ASR_MODELS,
  AsrAPI,
  AsrModelFamily,
  DecodingMethod,
  FileAsrConfig,
  ModelVariant,
  RoverAPI,
  SttProvider,
  TranscriptConfigAPI,
  VariantStatus,
} from '@/lib/asr';
import {
  DEFAULT_DECODING,
  DEFAULT_FAMILY,
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
import { Switch } from '@/components/ui/switch';

interface FileAsrPanelProps {
  config?: FileAsrConfig | null;
  disabled?: boolean;
  onSaved?: () => void;
}

interface DownloadState {
  downloading: boolean;
  progress: number;
  error: string | null;
}

export default function FileAsrPanel({ config, disabled = false, onSaved }: FileAsrPanelProps) {
  const [selectedFamily, setSelectedFamily] = useState<AsrModelFamily>(DEFAULT_FAMILY);
  const [selectedVariant, setSelectedVariant] = useState<ModelVariant>(DEFAULT_VARIANT);
  const [decodingMethod, setDecodingMethod] = useState<DecodingMethod>(DEFAULT_DECODING);
  const [numActivePaths, setNumActivePaths] = useState(DEFAULT_PATHS);
  const [maxSegmentSeconds, setMaxSegmentSeconds] = useState(DEFAULT_MAX_SEGMENT_SECONDS);
  const [provider, setProvider] = useState<SttProvider>('asr');
  const [roverEnabled, setRoverEnabled] = useState(false);
  const [roverFamilyB, setRoverFamilyB] = useState<AsrModelFamily>('gipformer-65m-rnnt');
  const [roverVariantB, setRoverVariantB] = useState<ModelVariant>('int8');
  const [roverVariantBStatus, setRoverVariantBStatus] = useState<VariantStatus>({
    hasFiles: false,
    isLoaded: false,
  });
  const [roverDownloadState, setRoverDownloadState] = useState<DownloadState>({
    downloading: false,
    progress: 0,
    error: null,
  });
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
  const geminiFieldsRef = useRef<GeminiSttFieldsHandle>(null);

  const selectedModelInfo = ASR_MODELS.find((m) => m.id === selectedFamily);
  const roverModelBInfo = ASR_MODELS.find((m) => m.id === roverFamilyB);
  const effectiveVariant = resolveVariantForFamily(selectedFamily, selectedVariant);
  const roverEffectiveVariantB = resolveVariantForFamily(roverFamilyB, roverVariantB);
  const availableVariantOptions = VARIANT_OPTIONS.filter((v) =>
    selectedModelInfo ? selectedModelInfo.availableVariants.includes(v.id) : true
  );
  const roverAvailableVariantsB = VARIANT_OPTIONS.filter((v) =>
    roverModelBInfo ? roverModelBInfo.availableVariants.includes(v.id) : true
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

  const refreshRoverVariantBStatus = useCallback(async () => {
    try {
      setRoverVariantBStatus(
        await AsrAPI.getVariantStatus(roverFamilyB, roverEffectiveVariantB)
      );
    } catch {
      setRoverVariantBStatus({ hasFiles: false, isLoaded: false });
    }
  }, [roverFamilyB, roverEffectiveVariantB]);

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
    if (typeof config.roverEnabled === 'boolean') setRoverEnabled(config.roverEnabled);
    if (config.roverFamilyB) setRoverFamilyB(parseAsrFamily(config.roverFamilyB));
    if (config.roverVariantB === 'int8' || config.roverVariantB === 'full') {
      setRoverVariantB(
        resolveVariantForFamily(
          parseAsrFamily(config.roverFamilyB),
          config.roverVariantB as ModelVariant
        )
      );
    }
    setProvider(config.provider === 'gemini' ? 'gemini' : 'asr');
  }, [config]);

  useEffect(() => {
    refreshAllVariantStatuses(selectedFamily);
  }, [selectedFamily, refreshAllVariantStatuses]);

  useEffect(() => {
    if (roverEnabled) refreshRoverVariantBStatus();
  }, [roverEnabled, refreshRoverVariantBStatus]);

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

  const handleDownloadRoverB = async () => {
    setRoverDownloadState({ downloading: true, progress: 0, error: null });
    try {
      await AsrAPI.downloadModel(roverFamilyB, roverEffectiveVariantB);
      await refreshRoverVariantBStatus();
      setRoverDownloadState({ downloading: false, progress: 100, error: null });
    } catch (e) {
      setRoverDownloadState((prev) => ({ ...prev, downloading: false, error: String(e) }));
    }
  };

  const handleSave = async () => {
    setIsSaving(true);
    setSaveMessage(null);
    const payload: FileAsrConfig = {
      model: selectedFamily,
      asrVariant: effectiveVariant,
      decodingMethod: roverEnabled ? DEFAULT_DECODING : decodingMethod,
      numActivePaths: roverEnabled ? DEFAULT_PATHS : numActivePaths,
      maxSegmentSeconds,
      roverEnabled,
      roverFamilyB: roverEnabled ? roverFamilyB : null,
      roverVariantB: roverEnabled ? roverEffectiveVariantB : null,
      provider,
    };
    try {
      if (provider === 'gemini') {
        await geminiFieldsRef.current?.saveKeyIfPresent();
      }
      await TranscriptConfigAPI.saveFile(payload);
      if (provider === 'gemini') {
        setSaveMessage('Đã lưu Gemini cho nhập file');
      } else if (roverEnabled) {
        const freshA = await AsrAPI.getVariantStatus(selectedFamily, effectiveVariant);
        const freshB = await AsrAPI.getVariantStatus(roverFamilyB, roverEffectiveVariantB);
        setVariantStatuses((prev) => ({ ...prev, [effectiveVariant]: freshA }));
        setRoverVariantBStatus(freshB);
        if (freshA.hasFiles && freshB.hasFiles) {
          await RoverAPI.validateModelReady();
          setSaveMessage('Đã lưu cấu hình ROVER (nhập file)');
        } else {
          setSaveMessage('Đã lưu. Tải đủ model A và B trước khi nhập file.');
        }
      } else {
        const freshStatus = await AsrAPI.getVariantStatus(selectedFamily, effectiveVariant);
        setVariantStatuses((prev) => ({ ...prev, [effectiveVariant]: freshStatus }));
        setSaveMessage(
          freshStatus.hasFiles
            ? 'Đã lưu cấu hình nhập file'
            : 'Đã lưu. Tải model trước khi nhập file.'
        );
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

  return (
    <div className="space-y-3">
      <p className="text-xs text-ink-2">
        Dùng khi nhập file hoặc nhận dạng lại.
        {provider === 'asr' && ' ROVER chỉ áp dụng cho luồng file.'}
      </p>

      <GeminiSttFields
        ref={geminiFieldsRef}
        provider={provider}
        onProviderChange={setProvider}
        disabled={disabled}
      />

      {provider === 'asr' && (
      <>
      <div className="space-y-1.5">
        <label className="block text-sm font-medium text-ink">Model ASR</label>
        <select
          value={selectedFamily}
          onChange={(e) => setSelectedFamily(e.target.value as AsrModelFamily)}
          disabled={disabled}
          className={selectClass}
        >
          {ASR_MODELS.filter((m) => !m.liveOnly).map((m) => (
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
        <AsrVariantNotice family={selectedFamily} variant={effectiveVariant} path="file" />
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
        {downloading && <p className="text-xs text-ink-2 font-mono">Đang tải {progress}%</p>}
        {error && <p className="text-xs text-destructive">{error}</p>}
      </div>

      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0">
          <p className="text-sm font-medium text-ink">ROVER</p>
          <p className="mt-0.5 text-xs text-ink-2 leading-snug">
            Kết hợp 2 model. Tốn RAM/CPU, nên dùng int8 cho cả hai.
          </p>
        </div>
        <Switch
          checked={roverEnabled}
          onCheckedChange={setRoverEnabled}
          disabled={disabled}
        />
      </div>

      {roverEnabled && (
        <div className="space-y-3 border-t border-rule pt-3">
          <p className="text-xs text-ink-2">Model B (phụ). Model A là lựa chọn ở trên.</p>
          <div className="space-y-1.5">
            <label className="block text-sm font-medium text-ink">Model B</label>
            <select
              value={roverFamilyB}
              onChange={(e) => setRoverFamilyB(e.target.value as AsrModelFamily)}
              disabled={disabled}
              className={selectClass}
            >
              {ASR_MODELS.filter((m) => !m.liveOnly && m.id !== selectedFamily).map((m) => (
                <option key={m.id} value={m.id}>{m.label}</option>
              ))}
            </select>
          </div>
          <div className="space-y-1.5">
            <label className="block text-sm font-medium text-ink">Biến thể B</label>
            <select
              value={roverEffectiveVariantB}
              onChange={(e) => setRoverVariantB(e.target.value as ModelVariant)}
              disabled={disabled || roverAvailableVariantsB.length <= 1}
              className={selectClass}
            >
              {roverAvailableVariantsB.map((v) => (
                <option key={v.id} value={v.id}>{v.label}</option>
              ))}
            </select>
          </div>
          <div className="flex items-center justify-between gap-3">
            <span className="text-xs text-ink-2">
              {roverVariantBStatus.hasFiles ? 'Đã tải' : 'Chưa tải'}
            </span>
            {!roverVariantBStatus.hasFiles && !roverDownloadState.downloading && (
              <button type="button" onClick={handleDownloadRoverB} disabled={disabled} className={saveClass}>
                Tải xuống
              </button>
            )}
          </div>
          {roverDownloadState.error && (
            <p className="text-xs text-destructive">{roverDownloadState.error}</p>
          )}
        </div>
      )}

      {!roverEnabled && (
        <>
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
        </>
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
      </>
      )}

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
