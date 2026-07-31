import { invoke } from '@tauri-apps/api/core';

export type ModelStatus =
  | { type: 'NotLoaded' }
  | { type: 'Downloading'; value: number }
  | { type: 'Ready' }
  | { type: 'Error'; value: string };

export type ModelVariant = 'int8' | 'full';
export type DecodingMethod = 'greedy_search' | 'modified_beam_search';

export interface ZipFormerModelInfo {
  id: ModelVariant;
  label: string;
  size: string;
  description: string;
}

export const ZIPFORMER_MODELS: ZipFormerModelInfo[] = [
  {
    id: 'int8',
    label: 'hynt/Zipformer-30M-RNNT-6000h (int8)',
    size: '~32 MB',
    description: 'Int8 quantized — nhỏ gọn, tốc độ cao',
  },
  {
    id: 'full',
    label: 'hynt/Zipformer-30M-RNNT-6000h (full)',
    size: '~100 MB',
    description: 'Full precision — độ chính xác cao nhất',
  },
];

export interface VariantStatus {
  hasFiles: boolean;
  isLoaded: boolean;
}

export const ZipFormerAPI = {
  init: (): Promise<void> =>
    invoke('zipformer_init'),

  getModelStatus: (): Promise<ModelStatus> =>
    invoke('zipformer_get_model_status'),

  isModelLoaded: (): Promise<boolean> =>
    invoke('zipformer_is_model_loaded'),

  getModelsDirectory: (): Promise<string> =>
    invoke('zipformer_get_models_directory'),

  downloadModel: (variant: ModelVariant): Promise<void> =>
    invoke('zipformer_download_model', { variant }),

  loadModel: (
    variant: ModelVariant,
    decodingMethod: DecodingMethod,
    numActivePaths: number
  ): Promise<void> =>
    invoke('zipformer_load_model', { variant, decodingMethod, numActivePaths }),

  transcribeAudio: (audioData: number[]): Promise<string> =>
    invoke('zipformer_transcribe_audio', { audioData }),

  validateModelReady: (
    variant?: ModelVariant,
    decodingMethod?: DecodingMethod,
    numActivePaths?: number
  ): Promise<string> =>
    invoke('zipformer_validate_model_ready', { variant, decodingMethod, numActivePaths }),

  getVariantStatus: (variant: ModelVariant): Promise<VariantStatus> =>
    invoke('zipformer_get_variant_status', { variant }),
};
