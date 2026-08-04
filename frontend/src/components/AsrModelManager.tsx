'use client';

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useState } from 'react';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import {
  ASR_MODELS,
  AsrAPI,
  AsrModelFamily,
  CapuAPI,
  DecodingMethod,
  ModelVariant,
  RoverAPI,
  VariantStatus,
} from '../lib/asr';

const DEFAULT_FAMILY: AsrModelFamily = 'zipformer-vi-30m';
const DEFAULT_VARIANT: ModelVariant = 'int8';
const DEFAULT_DECODING: DecodingMethod = 'modified_beam_search';
const DEFAULT_PATHS = 15;
const DEFAULT_MAX_SEGMENT_SECONDS = 25;
const MIN_MAX_SEGMENT_SECONDS = 5;
const MAX_MAX_SEGMENT_SECONDS = 30;
const DEFAULT_CAPU_PUNCTUATION_LEVEL = 7;
const DEFAULT_CAPU_CASE_LEVEL = 3;
const FALLBACK_PHYSICAL_CORES = 4;

// Exact-value lookup, no interpolation between mid-points — matches the reference app's
// own `labels.get(value, str(value))` (values without a label just show the raw number).
const LEVEL_LABELS: Record<number, string> = {
  1: 'Rất ít',
  3: 'Ít',
  5: 'Vừa',
  7: 'Nhiều',
  10: 'Rất nhiều',
};
const levelLabel = (v: number) => LEVEL_LABELS[v] ?? String(v);

const VARIANT_OPTIONS: { id: ModelVariant; label: string }[] = [
  { id: 'int8', label: 'int8 (quantized)' },
  { id: 'full', label: 'full (precision)' },
];

function resolveVariantForFamily(family: AsrModelFamily, variant: ModelVariant): ModelVariant {
  const info = ASR_MODELS.find((m) => m.id === family);
  if (!info) return variant;
  return info.availableVariants.includes(variant) ? variant : info.availableVariants[0];
}

interface DownloadState {
  downloading: boolean;
  progress: number;
  error: string | null;
}

export default function AsrModelManager() {
  const { isRecording } = useRecordingState();
  const [selectedFamily, setSelectedFamily] = useState<AsrModelFamily>(DEFAULT_FAMILY);
  const [selectedVariant, setSelectedVariant] = useState<ModelVariant>(DEFAULT_VARIANT);
  const [decodingMethod, setDecodingMethod] = useState<DecodingMethod>(DEFAULT_DECODING);
  const [numActivePaths, setNumActivePaths] = useState<number>(DEFAULT_PATHS);
  const [maxSegmentSeconds, setMaxSegmentSeconds] = useState<number>(DEFAULT_MAX_SEGMENT_SECONDS);

  const [roverEnabled, setRoverEnabled] = useState(false);
  const [hotwords, setHotwords] = useState('');
  const [physicalCores, setPhysicalCores] = useState(FALLBACK_PHYSICAL_CORES);
  const [capuThreads, setCapuThreads] = useState(FALLBACK_PHYSICAL_CORES);
  const [capuPunctuationLevel, setCapuPunctuationLevel] = useState(DEFAULT_CAPU_PUNCTUATION_LEVEL);
  const [capuCaseLevel, setCapuCaseLevel] = useState(DEFAULT_CAPU_CASE_LEVEL);
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

  const selectedModelInfo = ASR_MODELS.find((m) => m.id === selectedFamily);

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

  const roverModelBInfo = ASR_MODELS.find((m) => m.id === roverFamilyB);
  const roverEffectiveVariantB = resolveVariantForFamily(roverFamilyB, roverVariantB);
  const roverAvailableVariantsB = VARIANT_OPTIONS.filter((v) =>
    roverModelBInfo ? roverModelBInfo.availableVariants.includes(v.id) : true
  );
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
      if (modelInfo && !modelInfo.availableVariants.includes(variant.id)) {
        continue;
      }
      try {
        results[variant.id] = await AsrAPI.getVariantStatus(family, variant.id);
      } catch {
        // keep defaults
      }
    }
    setVariantStatuses(results);
  }, []);

  useEffect(() => {
    AsrAPI.init().catch(console.error);
    loadSavedConfig();
    CapuAPI.getCpuTopology()
      .then(({ physicalCores: cores }) => {
        setPhysicalCores(cores);
        setCapuThreads((prev) => Math.min(prev, cores));
      })
      .catch(() => {
        // Fall back to FALLBACK_PHYSICAL_CORES already set as initial state — don't block
        // the rest of the settings panel on this.
      });
  }, []);

  useEffect(() => {
    setVariantStatuses({
      int8: { hasFiles: false, isLoaded: false },
      full: { hasFiles: false, isLoaded: false },
    });
    refreshAllVariantStatuses(selectedFamily);
  }, [selectedFamily, refreshAllVariantStatuses]);

  useEffect(() => {
    if (!selectedModelInfo) return;
    const resolved = resolveVariantForFamily(selectedFamily, selectedVariant);
    if (resolved !== selectedVariant) {
      setSelectedVariant(resolved);
    }
  }, [selectedFamily, selectedModelInfo, selectedVariant]);

  useEffect(() => {
    if (!roverModelBInfo) return;
    const resolved = resolveVariantForFamily(roverFamilyB, roverVariantB);
    if (resolved !== roverVariantB) {
      setRoverVariantB(resolved);
    }
  }, [roverFamilyB, roverModelBInfo, roverVariantB]);

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
    if (roverEnabled) {
      refreshRoverVariantBStatus();
    }
  }, [roverEnabled, refreshRoverVariantBStatus]);

  useEffect(() => {
    const unlistenProgress = listen<{ progress: number }>(
      'asr-model-download-progress',
      (event) => {
        setDownloadState((prev) => ({ ...prev, progress: event.payload.progress }));
      }
    );

    const unlistenComplete = listen('asr-model-download-complete', () => {
      setDownloadState({ downloading: false, progress: 100, error: null });
      refreshAllVariantStatuses(selectedFamily);
    });

    const unlistenError = listen<{ error: string }>(
      'asr-model-download-error',
      (event) => {
        setDownloadState((prev) => ({
          ...prev,
          downloading: false,
          error: event.payload.error,
        }));
      }
    );

    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenComplete.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, [selectedFamily, refreshAllVariantStatuses]);

  const loadSavedConfig = async () => {
    try {
      const config = await invoke<{
        model?: string;
        asrVariant?: string;
        decodingMethod?: string;
        numActivePaths?: number;
        maxSegmentSeconds?: number;
        roverEnabled?: boolean;
        roverFamilyB?: string;
        roverVariantB?: string;
        hotwords?: string;
        capuCpuThreads?: number | null;
        capuPunctuationLevel?: number;
        capuCaseLevel?: number;
      } | null>('api_get_transcript_config');
      if (config) {
        const loadedFamily = (
          config.model === 'zipformer-vi-30m' ||
          config.model === 'gipformer-65m-rnnt' ||
          config.model === 'sherpa-onnx-zipformer-vi-2025-04-20'
            ? config.model
            : DEFAULT_FAMILY
        ) as AsrModelFamily;
        setSelectedFamily(loadedFamily);
        if (typeof config.roverEnabled === 'boolean') {
          setRoverEnabled(config.roverEnabled);
        }
        if (
          config.roverFamilyB === 'zipformer-vi-30m' ||
          config.roverFamilyB === 'gipformer-65m-rnnt' ||
          config.roverFamilyB === 'sherpa-onnx-zipformer-vi-2025-04-20'
        ) {
          setRoverFamilyB(config.roverFamilyB as AsrModelFamily);
        }
        if (config.roverVariantB === 'int8' || config.roverVariantB === 'full') {
          setRoverVariantB(
            resolveVariantForFamily(
              config.roverFamilyB === 'zipformer-vi-30m' ||
                config.roverFamilyB === 'gipformer-65m-rnnt' ||
                config.roverFamilyB === 'sherpa-onnx-zipformer-vi-2025-04-20'
                ? (config.roverFamilyB as AsrModelFamily)
                : roverFamilyB,
              config.roverVariantB as ModelVariant
            )
          );
        }
        const loadedVariant =
          config.asrVariant === 'int8' || config.asrVariant === 'full'
            ? (config.asrVariant as ModelVariant)
            : DEFAULT_VARIANT;
        setSelectedVariant(resolveVariantForFamily(loadedFamily, loadedVariant));
        if (
          config.decodingMethod === 'greedy_search' ||
          config.decodingMethod === 'modified_beam_search'
        ) {
          setDecodingMethod(config.decodingMethod);
        }
        if (typeof config.numActivePaths === 'number') {
          setNumActivePaths(config.numActivePaths);
        }
        if (typeof config.maxSegmentSeconds === 'number') {
          setMaxSegmentSeconds(
            Math.min(
              MAX_MAX_SEGMENT_SECONDS,
              Math.max(MIN_MAX_SEGMENT_SECONDS, config.maxSegmentSeconds)
            )
          );
        }
        if (typeof config.hotwords === 'string') {
          setHotwords(config.hotwords);
        }
        if (typeof config.capuCpuThreads === 'number') {
          setCapuThreads(config.capuCpuThreads);
        }
        if (typeof config.capuPunctuationLevel === 'number') {
          setCapuPunctuationLevel(config.capuPunctuationLevel);
        }
        if (typeof config.capuCaseLevel === 'number') {
          setCapuCaseLevel(config.capuCaseLevel);
        }
      }
    } catch (e) {
      console.error('Failed to load ASR config:', e);
    }
  };

  const handleDownload = async () => {
    setDownloadState({ downloading: true, progress: 0, error: null });
    try {
      await AsrAPI.downloadModel(selectedFamily, effectiveVariant);
    } catch (e) {
      setDownloadState((prev) => ({
        ...prev,
        downloading: false,
        error: String(e),
      }));
    }
  };

  const handleDownloadRoverB = async () => {
    setRoverDownloadState({ downloading: true, progress: 0, error: null });
    try {
      await AsrAPI.downloadModel(roverFamilyB, roverEffectiveVariantB);
      await refreshRoverVariantBStatus();
      setRoverDownloadState({ downloading: false, progress: 100, error: null });
    } catch (e) {
      setRoverDownloadState((prev) => ({
        ...prev,
        downloading: false,
        error: String(e),
      }));
    }
  };

  const handleSave = async () => {
    setIsSaving(true);
    setSaveMessage(null);
    try {
      await invoke('api_save_transcript_config', {
        provider: 'asr',
        model: selectedFamily,
        apiKey: null,
        asrVariant: effectiveVariant,
        decodingMethod,
        numActivePaths,
        maxSegmentSeconds,
        roverEnabled,
        roverFamilyB: roverEnabled ? roverFamilyB : null,
        roverVariantB: roverEnabled ? roverEffectiveVariantB : null,
        hotwords,
        capuCpuThreads: capuThreads,
        capuPunctuationLevel,
        capuCaseLevel,
      });

      if (roverEnabled) {
        const freshA = await AsrAPI.getVariantStatus(selectedFamily, effectiveVariant);
        const freshB = await AsrAPI.getVariantStatus(roverFamilyB, roverEffectiveVariantB);
        setVariantStatuses((prev) => ({ ...prev, [effectiveVariant]: freshA }));
        setRoverVariantBStatus(freshB);
        if (freshA.hasFiles && freshB.hasFiles) {
          await RoverAPI.validateModelReady();
          await refreshRoverVariantBStatus();
          setSaveMessage('Đã lưu cấu hình ROVER thành công');
        } else {
          setSaveMessage('Đã lưu cấu hình. Tải đủ cả model A và model B trước khi nhận dạng.');
        }
      } else {
        const freshStatus = await AsrAPI.getVariantStatus(selectedFamily, effectiveVariant);
        setVariantStatuses((prev) => ({ ...prev, [effectiveVariant]: freshStatus }));

        if (freshStatus.hasFiles) {
          await AsrAPI.loadModel(
            selectedFamily,
            effectiveVariant,
            decodingMethod,
            numActivePaths
          );
          await refreshAllVariantStatuses(selectedFamily);
          setSaveMessage('Đã lưu cấu hình thành công');
        } else {
          setSaveMessage('Đã lưu cấu hình. Bấm "Tải xuống" để tải model trước khi nhận dạng.');
        }
      }
      setTimeout(() => setSaveMessage(null), 3000);
    } catch (e) {
      setSaveMessage(`Lỗi: ${String(e)}`);
    } finally {
      setIsSaving(false);
    }
  };

  const currentStatus = variantStatuses[effectiveVariant];
  const { downloading, progress, error } = downloadState;
  const disabled = isRecording;

  return (
    <div className="space-y-4">
      {isRecording && (
        <p className="text-xs text-amber-600 dark:text-amber-400">
          Không thể thay đổi mô hình khi đang ghi âm.
        </p>
      )}

      {/* Model family selector */}
      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          Model ASR
        </label>
        <select
          value={selectedFamily}
          onChange={(e) => setSelectedFamily(e.target.value as AsrModelFamily)}
          disabled={disabled}
          className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
        >
          {ASR_MODELS.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
        </select>
      </div>

      {/* Variant selector */}
      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          Biến thể
        </label>
        <select
          value={effectiveVariant}
          onChange={(e) => setSelectedVariant(e.target.value as ModelVariant)}
          disabled={disabled || availableVariantOptions.length <= 1}
          className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
        >
          {availableVariantOptions.map((v) => {
            const size = v.id === 'int8' ? selectedModelInfo?.int8Size : selectedModelInfo?.fullSize;
            return (
              <option key={v.id} value={v.id}>
                {v.label} ({size})
              </option>
            );
          })}
        </select>

        {/* Status + download row */}
        <div className="flex items-center justify-between p-3 rounded-lg border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800">
          <div className="flex items-center gap-2">
            <span className="text-base">🇻🇳</span>
            <div>
              <p className="text-xs font-medium text-gray-800 dark:text-gray-200">
                {selectedModelInfo?.description}
              </p>
              {currentStatus.isLoaded && (
                <span className="text-xs text-green-600 dark:text-green-400 font-medium">
                  ✓ Đang dùng
                </span>
              )}
              {currentStatus.hasFiles && !currentStatus.isLoaded && (
                <span className="text-xs text-yellow-600 dark:text-yellow-400">
                  Đã tải, chưa load
                </span>
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
              className="px-3 py-1.5 text-xs rounded-md bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white font-medium transition-colors"
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
              <div
                className="bg-blue-500 h-1.5 rounded-full transition-all duration-300"
                style={{ width: `${progress}%` }}
              />
            </div>
          </div>
        )}

        {error && (
          <p className="text-xs text-red-500 dark:text-red-400">{error}</p>
        )}
      </div>

      {/* ROVER toggle */}
      <div className="flex items-center justify-between p-3 rounded-lg border border-gray-200 dark:border-gray-700">
        <div>
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            Bật ROVER (kết hợp 2 model)
          </label>
          <p className="text-xs text-gray-500 dark:text-gray-400">
            ROVER dùng gấp đôi RAM/CPU so với 1 model — khuyến nghị dùng biến thể int8 cho cả 2 phía.
          </p>
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
            Model B (phụ) — Model A (chính) là model chọn ở trên
          </p>
          <select
            value={roverFamilyB}
            onChange={(e) => setRoverFamilyB(e.target.value as AsrModelFamily)}
            disabled={disabled}
            className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
          >
            {ASR_MODELS.filter((m) => m.id !== selectedFamily).map((m) => (
              <option key={m.id} value={m.id}>
                {m.label}
              </option>
            ))}
          </select>
          <select
            value={roverEffectiveVariantB}
            onChange={(e) => setRoverVariantB(e.target.value as ModelVariant)}
            disabled={disabled || roverAvailableVariantsB.length <= 1}
            className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
          >
            {roverAvailableVariantsB.map((v) => {
              const size =
                v.id === 'int8' ? roverModelBInfo?.int8Size : roverModelBInfo?.fullSize;
              return (
                <option key={v.id} value={v.id}>
                  {v.label} ({size})
                </option>
              );
            })}
          </select>
          <div className="flex items-center justify-between p-2 rounded-md bg-gray-50 dark:bg-gray-800">
            <span className="text-xs text-gray-600 dark:text-gray-300">
              {roverVariantBStatus.hasFiles ? '✓ Đã tải' : 'Chưa tải'}
            </span>
            {!roverVariantBStatus.hasFiles && !roverDownloadState.downloading && (
              <button
                onClick={handleDownloadRoverB}
                disabled={disabled}
                className="px-3 py-1.5 text-xs rounded-md bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white font-medium transition-colors"
              >
                Tải xuống
              </button>
            )}
          </div>
          {roverDownloadState.downloading && (
            <p className="text-xs text-gray-500">Đang tải model B...</p>
          )}
          {roverDownloadState.error && (
            <p className="text-xs text-red-500 dark:text-red-400">{roverDownloadState.error}</p>
          )}
        </div>
      )}

      {/* Decoding method — sherpa-onnx-specific, not applicable to ROVER's custom decoder */}
      {!roverEnabled && (
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
              className={`flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md border transition-colors disabled:opacity-50 ${
                decodingMethod === m
                  ? 'border-orange-500 bg-orange-50 dark:bg-orange-900/20 text-orange-700 dark:text-orange-300'
                  : 'border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-400 hover:border-gray-400'
              }`}
            >
              <span
                className={`w-3 h-3 rounded-full border-2 flex-shrink-0 ${
                  decodingMethod === m
                    ? 'border-orange-500 bg-orange-500'
                    : 'border-gray-400'
                }`}
              />
              {m}
            </button>
          ))}
        </div>
      </div>
      )}

      {/* Num active paths — only for beam search, and not applicable to ROVER */}
      {!roverEnabled && decodingMethod === 'modified_beam_search' && (
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
              Số đường giải mã (num_active_paths)
            </label>
            <span className="text-sm font-mono text-gray-900 dark:text-white w-8 text-right">
              {numActivePaths}
            </span>
          </div>
          <div className="flex items-center gap-3">
            <span className="text-xs text-gray-400">1</span>
            <input
              type="range"
              min={1}
              max={100}
              step={1}
              value={numActivePaths}
              onChange={(e) => setNumActivePaths(Number(e.target.value))}
              disabled={disabled}
              className="flex-1 accent-orange-500 disabled:opacity-50"
            />
            <span className="text-xs text-gray-400">100</span>
          </div>
        </div>
      )}

      {/* Max segment length */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            Độ dài tối đa mỗi đoạn
          </label>
          <span className="text-sm font-mono text-gray-900 dark:text-white w-10 text-right">
            {maxSegmentSeconds}s
          </span>
        </div>
        <p className="text-xs text-gray-500 dark:text-gray-400">
          Cắt đoạn nói dài tại chỗ im lặng trước khi gửi nhận dạng. Giá trị nhỏ hơn giúp đoạn thuyết trình dài ổn định hơn; giá trị lớn hơn giữ ngữ cảnh câu đầy đủ hơn.
        </p>
        <div className="flex items-center gap-3">
          <span className="text-xs text-gray-400">{MIN_MAX_SEGMENT_SECONDS}s</span>
          <input
            type="range"
            min={MIN_MAX_SEGMENT_SECONDS}
            max={MAX_MAX_SEGMENT_SECONDS}
            step={1}
            value={maxSegmentSeconds}
            onChange={(e) => setMaxSegmentSeconds(Number(e.target.value))}
            disabled={disabled}
            className="flex-1 accent-blue-500 disabled:opacity-50"
          />
          <span className="text-xs text-gray-400">{MAX_MAX_SEGMENT_SECONDS}s</span>
        </div>
      </div>

      {/* Hotwords */}
      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          Từ khóa ưu tiên (tên riêng, thuật ngữ chuyên ngành)
        </label>
        <p className="text-xs text-gray-500 dark:text-gray-400">
          Mỗi dòng một cụm từ. Có thể thêm trọng số bằng cú pháp{' '}
          <code className="px-1 rounded bg-gray-100 dark:bg-gray-700">CỤM TỪ :2.5</code> (mặc định
          1.5 nếu không ghi). Dòng bắt đầu bằng{' '}
          <code className="px-1 rounded bg-gray-100 dark:bg-gray-700">#</code> là ghi chú, sẽ bị bỏ
          qua. Có hiệu lực ngay từ lần nhận dạng tiếp theo sau khi lưu — không cần tải lại model.
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

      {/* CAPU: CPU threads */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            Số luồng CPU (thêm dấu câu)
          </label>
          <span className="text-sm font-mono text-gray-900 dark:text-white w-8 text-right">
            {capuThreads}
          </span>
        </div>
        <p className="text-xs text-gray-500 dark:text-gray-400">
          Số luồng CPU dành cho model thêm dấu câu/viết hoa. Đổi giá trị này sẽ tải lại model khi lưu
          (mất khoảng 1-2 giây).
        </p>
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

      {/* CAPU: punctuation level */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            Mức độ thêm dấu
          </label>
          <span className="text-sm font-mono text-gray-900 dark:text-white w-16 text-right">
            {levelLabel(capuPunctuationLevel)}
          </span>
        </div>
        <p className="text-xs text-gray-500 dark:text-gray-400">
          Mức 1 tắt hoàn toàn việc thêm dấu câu (giữ nguyên văn bản thô từ nhận dạng giọng nói).
        </p>
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

      {/* CAPU: case level */}
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

      {/* Save button */}
      <div className="flex items-center gap-3 pt-1">
        <button
          onClick={handleSave}
          disabled={isSaving || disabled}
          className="px-4 py-2 text-sm rounded-md bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white font-medium transition-colors"
        >
          {isSaving ? 'Đang lưu...' : 'Lưu cấu hình'}
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
