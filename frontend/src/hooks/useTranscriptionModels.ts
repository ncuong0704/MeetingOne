import { useState, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface RawModelInfo {
  name: string;
  size_mb: number;
  status: 'Available' | 'Missing' | { Downloading: number } | { Error: string };
}

export interface ModelOption {
  provider: 'zipformer';
  name: string;
  displayName: string;
  size_mb: number;
}

interface TranscriptModelConfig {
  provider?: string;
  model?: string;
}

export function useTranscriptionModels(transcriptModelConfig: TranscriptModelConfig | undefined) {
  const [availableModels, setAvailableModels] = useState<ModelOption[]>([]);
  const [selectedModelKey, setSelectedModelKey] = useState<string>('zipformer:zipformer-vi-30m');
  const [loadingModels, setLoadingModels] = useState(false);
  const userSelectedRef = useRef(false);

  const setSelectedModelKeyWithTracking = useCallback((key: string) => {
    userSelectedRef.current = true;
    setSelectedModelKey(key);
  }, []);

  const fetchModels = useCallback(async () => {
    setLoadingModels(true);

    try {
      await invoke('zipformer_init');
      const isLoaded = await invoke<boolean>('zipformer_is_model_loaded');

      const zipformerModel: ModelOption = {
        provider: 'zipformer',
        name: 'zipformer-vi-30m',
        displayName: '🇻🇳 ZipFormer Vietnamese ASR (~30 MB)',
        size_mb: 30,
      };

      setAvailableModels(isLoaded ? [zipformerModel] : []);

      if (!userSelectedRef.current && isLoaded) {
        setSelectedModelKey('zipformer:zipformer-vi-30m');
      }
    } catch (err) {
      console.error('Failed to check ZipFormer status:', err);
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
