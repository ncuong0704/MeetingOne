import { useState, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  GIPFORMER_MODEL_ID,
  SHERPA_VI_2025_MODEL_ID,
  ZIPFORMER_MODEL_ID,
} from '@/constants/modelDefaults';

export interface RawModelInfo {
  name: string;
  size_mb: number;
  status: 'Available' | 'Missing' | { Downloading: number } | { Error: string };
}

export interface ModelOption {
  provider: 'asr';
  name: string;
  displayName: string;
  size_mb: number;
}

interface TranscriptModelConfig {
  provider?: string;
  model?: string;
}

const ASR_MODEL_OPTIONS: ModelOption[] = [
  {
    provider: 'asr',
    name: ZIPFORMER_MODEL_ID,
    displayName: '🇻🇳 ZipFormer 30M Vietnamese ASR (~30 MB)',
    size_mb: 30,
  },
  {
    provider: 'asr',
    name: GIPFORMER_MODEL_ID,
    displayName: '🇻🇳 Gipformer 65M Vietnamese ASR (~65 MB)',
    size_mb: 65,
  },
  {
    provider: 'asr',
    name: SHERPA_VI_2025_MODEL_ID,
    displayName: '🇻🇳 Sherpa-ONNX Zipformer VI 2025 (~270 MB)',
    size_mb: 270,
  },
];

export function useTranscriptionModels(transcriptModelConfig: TranscriptModelConfig | undefined) {
  const [availableModels, setAvailableModels] = useState<ModelOption[]>([]);
  const [selectedModelKey, setSelectedModelKey] = useState<string>(`asr:${ZIPFORMER_MODEL_ID}`);
  const [loadingModels, setLoadingModels] = useState(false);
  const userSelectedRef = useRef(false);

  const setSelectedModelKeyWithTracking = useCallback((key: string) => {
    userSelectedRef.current = true;
    setSelectedModelKey(key);
  }, []);

  const fetchModels = useCallback(async () => {
    setLoadingModels(true);

    try {
      await invoke('asr_init');
      const isLoaded = await invoke<boolean>('asr_is_model_loaded');

      setAvailableModels(isLoaded ? ASR_MODEL_OPTIONS : []);

      if (!userSelectedRef.current && isLoaded) {
        const defaultKey = transcriptModelConfig?.model
          ? `asr:${transcriptModelConfig.model}`
          : `asr:${ZIPFORMER_MODEL_ID}`;
        setSelectedModelKey(defaultKey);
      }
    } catch (err) {
      console.error('Failed to check ASR status:', err);
      setAvailableModels([]);
    }

    setLoadingModels(false);
  }, [transcriptModelConfig]);

  const resetSelection = useCallback(() => {
    userSelectedRef.current = false;
  }, []);

  return {
    availableModels,
    selectedModelKey,
    setSelectedModelKey: setSelectedModelKeyWithTracking,
    loadingModels,
    fetchModels,
    resetSelection,
  };
}
