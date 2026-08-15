'use client';

import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useState } from 'react';
import {
  ASR_MODELS,
  AsrAPI,
  AsrModelFamily,
  DecodingMethod,
  FileAsrConfig,
  ModelVariant,
  RoverAPI,
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
    };
    try {
      await TranscriptConfigAPI.saveFile(payload);
      if (roverEnabled) {
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

  return (
    <div className="space-y-4">
      <p className="text-xs text-gray-500 dark:text-gray-400">
        Cấu hình này dùng khi nhập file audio hoặc nhận dạng lại. ROVER chỉ áp dụng cho luồng file.
      </p>

      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          Model ASR (nhập file)
        </label>
        <select
          value={selectedFamily}
          onChange={(e) => setSelectedFamily(e.target.value as AsrModelFamily)}
          disabled={disabled}
          className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
        >
          {ASR_MODELS.filter((m) => !m.liveOnly).map((m) => (
            <option key={m.id} value={m.id}>{m.label}</option>
          ))}
        </select>
        {selectedModelInfo?.description && (
          <p className="text-xs text-gray-500 dark:text-gray-400">{selectedModelInfo.description}</p>
        )}
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
        <div className="flex items-center justify-between p-3 rounded-lg border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800">
          <div>
            {currentStatus.isLoaded && (
              <span className="text-xs text-green-600 font-medium">✓ Đang dùng</span>
            )}
            {currentStatus.hasFiles && !currentStatus.isLoaded && (
              <span className="text-xs text-yellow-600">Đã tải, chưa load</span>
            )}
            {!currentStatus.hasFiles && !downloading && (
              <span className="text-xs text-gray-400">Chưa tải</span>
            )}
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
        {downloading && <p className="text-xs text-gray-500">Đang tải... {progress}%</p>}
        {error && <p className="text-xs text-red-500">{error}</p>}
      </div>

      <div className="flex items-center justify-between p-3 rounded-lg border border-gray-200 dark:border-gray-700">
        <div>
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            Bật ROVER (kết hợp 2 model)
          </label>
          <p className="text-xs text-gray-500">ROVER dùng gấp đôi RAM/CPU — khuyến nghị int8 cho cả 2 phía.</p>
        </div>
        <input
          type="checkbox"
          checked={roverEnabled}
          onChange={(e) => setRoverEnabled(e.target.checked)}
          disabled={disabled}
          className="w-5 h-5 accent-blue-500 disabled:opacity-50"
        />
      </div>

      {roverEnabled && (
        <div className="space-y-2 p-3 rounded-lg border border-blue-200 dark:border-blue-800">
          <p className="text-xs font-medium text-gray-700 dark:text-gray-300">
            Model B (phụ) — Model A là model chọn ở trên
          </p>
          <select
            value={roverFamilyB}
            onChange={(e) => setRoverFamilyB(e.target.value as AsrModelFamily)}
            disabled={disabled}
            className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800"
          >
            {ASR_MODELS.filter((m) => !m.liveOnly && m.id !== selectedFamily).map((m) => (
              <option key={m.id} value={m.id}>{m.label}</option>
            ))}
          </select>
          <select
            value={roverEffectiveVariantB}
            onChange={(e) => setRoverVariantB(e.target.value as ModelVariant)}
            disabled={disabled || roverAvailableVariantsB.length <= 1}
            className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800"
          >
            {roverAvailableVariantsB.map((v) => (
              <option key={v.id} value={v.id}>{v.label}</option>
            ))}
          </select>
          <div className="flex items-center justify-between p-2 rounded-md bg-gray-50 dark:bg-gray-800">
            <span className="text-xs text-gray-600">
              {roverVariantBStatus.hasFiles ? '✓ Đã tải' : 'Chưa tải'}
            </span>
            {!roverVariantBStatus.hasFiles && !roverDownloadState.downloading && (
              <button
                onClick={handleDownloadRoverB}
                disabled={disabled}
                className="px-3 py-1.5 text-xs rounded-md bg-blue-600 text-white"
              >
                Tải xuống
              </button>
            )}
          </div>
          {roverDownloadState.error && (
            <p className="text-xs text-red-500">{roverDownloadState.error}</p>
          )}
        </div>
      )}

      {!roverEnabled && (
        <>
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
                      ? 'border-orange-500 bg-orange-50 text-orange-700'
                      : 'border-gray-300 text-gray-600'
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
        </>
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
          {isSaving ? 'Đang lưu...' : 'Lưu cấu hình nhập file'}
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
