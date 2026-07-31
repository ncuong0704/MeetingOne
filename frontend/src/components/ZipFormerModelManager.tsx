'use client';

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import {
  DecodingMethod,
  ModelVariant,
  VariantStatus,
  ZIPFORMER_MODELS,
  ZipFormerAPI,
} from '../lib/zipformer';

const DEFAULT_VARIANT: ModelVariant = 'int8';
const DEFAULT_DECODING: DecodingMethod = 'modified_beam_search';
const DEFAULT_PATHS = 15;

interface DownloadState {
  downloading: boolean;
  progress: number;
  error: string | null;
}

export default function ZipFormerModelManager() {
  const [selectedVariant, setSelectedVariant] = useState<ModelVariant>(DEFAULT_VARIANT);
  const [decodingMethod, setDecodingMethod] = useState<DecodingMethod>(DEFAULT_DECODING);
  const [numActivePaths, setNumActivePaths] = useState<number>(DEFAULT_PATHS);

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

  useEffect(() => {
    ZipFormerAPI.init().catch(console.error);
    loadSavedConfig();
    refreshAllVariantStatuses();

    const unlistenProgress = listen<{ progress: number }>(
      'zipformer-model-download-progress',
      (event) => {
        setDownloadState((prev) => ({ ...prev, progress: event.payload.progress }));
      }
    );

    const unlistenComplete = listen('zipformer-model-download-complete', () => {
      setDownloadState({ downloading: false, progress: 100, error: null });
      refreshAllVariantStatuses();
    });

    const unlistenError = listen<{ error: string }>(
      'zipformer-model-download-error',
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
  }, []);

  const loadSavedConfig = async () => {
    try {
      const config = await invoke<{
        zipformerVariant?: string;
        decodingMethod?: string;
        numActivePaths?: number;
      } | null>('api_get_transcript_config');
      if (config) {
        if (config.zipformerVariant === 'int8' || config.zipformerVariant === 'full') {
          setSelectedVariant(config.zipformerVariant);
        }
        if (
          config.decodingMethod === 'greedy_search' ||
          config.decodingMethod === 'modified_beam_search'
        ) {
          setDecodingMethod(config.decodingMethod);
        }
        if (typeof config.numActivePaths === 'number') {
          setNumActivePaths(config.numActivePaths);
        }
      }
    } catch (e) {
      console.error('Failed to load ZipFormer config:', e);
    }
  };

  const refreshAllVariantStatuses = async () => {
    const results: Record<ModelVariant, VariantStatus> = {
      int8: { hasFiles: false, isLoaded: false },
      full: { hasFiles: false, isLoaded: false },
    };
    for (const m of ZIPFORMER_MODELS) {
      try {
        results[m.id] = await ZipFormerAPI.getVariantStatus(m.id);
      } catch {
        // keep defaults
      }
    }
    setVariantStatuses(results);
  };

  const handleDownload = async () => {
    setDownloadState({ downloading: true, progress: 0, error: null });
    try {
      await ZipFormerAPI.downloadModel(selectedVariant);
    } catch (e) {
      setDownloadState((prev) => ({
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
        provider: 'zipformer',
        model: 'zipformer-vi',
        apiKey: null,
        zipformerVariant: selectedVariant,
        decodingMethod,
        numActivePaths,
      });

      // Reload model with new config if files are present
      const status = variantStatuses[selectedVariant];
      if (status.hasFiles) {
        await ZipFormerAPI.loadModel(selectedVariant, decodingMethod, numActivePaths);
        await refreshAllVariantStatuses();
      }

      setSaveMessage('Đã lưu cấu hình thành công');
      setTimeout(() => setSaveMessage(null), 3000);
    } catch (e) {
      setSaveMessage(`Lỗi: ${String(e)}`);
    } finally {
      setIsSaving(false);
    }
  };

  const currentStatus = variantStatuses[selectedVariant];
  const { downloading, progress, error } = downloadState;

  return (
    <div className="space-y-4">
      {/* Model selector */}
      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          Chọn phiên bản 
        </label>
        <select
          value={selectedVariant}
          onChange={(e) => setSelectedVariant(e.target.value as ModelVariant)}
          className="w-full px-3 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500"
        >
          {ZIPFORMER_MODELS.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label} ({m.size})
            </option>
          ))}
        </select>

        {/* Status + download row */}
        <div className="flex items-center justify-between p-3 rounded-lg border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800">
          <div className="flex items-center gap-2">
            <span className="text-base">🇻🇳</span>
            <div>
              <p className="text-xs font-medium text-gray-800 dark:text-gray-200">
                {ZIPFORMER_MODELS.find((m) => m.id === selectedVariant)?.description}
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
              className="px-3 py-1.5 text-xs rounded-md bg-blue-600 hover:bg-blue-700 text-white font-medium transition-colors"
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

      {/* Decoding method */}
      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          Phương pháp giải mã
        </label>
        <div className="flex gap-2">
          {(['greedy_search', 'modified_beam_search'] as DecodingMethod[]).map((m) => (
            <button
              key={m}
              onClick={() => setDecodingMethod(m)}
              className={`flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md border transition-colors ${
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

      {/* Num active paths — only for beam search */}
      {decodingMethod === 'modified_beam_search' && (
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
              className="flex-1 accent-orange-500"
            />
            <span className="text-xs text-gray-400">100</span>
          </div>
        </div>
      )}

      {/* Save button */}
      <div className="flex items-center gap-3 pt-1">
        <button
          onClick={handleSave}
          disabled={isSaving}
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
