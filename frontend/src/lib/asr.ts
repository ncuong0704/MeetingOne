import { invoke } from '@tauri-apps/api/core';

export type ModelStatus =
  | { type: 'NotLoaded' }
  | { type: 'Downloading'; value: number }
  | { type: 'Ready' }
  | { type: 'Error'; value: string };

export type AsrModelFamily =
  | 'zipformer-vi-30m'
  | 'gipformer-65m-rnnt'
  | 'sherpa-onnx-zipformer-vi-2025-04-20';
export type ModelVariant = 'int8' | 'full';
export type DecodingMethod = 'greedy_search' | 'modified_beam_search';

export interface AsrModelInfo {
  id: AsrModelFamily;
  label: string;
  hfRepo: string;
  int8Size: string;
  fullSize: string;
  description: string;
  /** Which variants this family actually ships. Must match Rust `ModelFamily::available_variants()`. */
  availableVariants: ModelVariant[];
}

export const ASR_MODELS: AsrModelInfo[] = [
  {
    id: 'zipformer-vi-30m',
    label: 'ZipFormer 30M',
    hfRepo: 'hynt/Zipformer-30M-RNNT-6000h',
    int8Size: '~32 MB',
    fullSize: '~100 MB',
    description: 'Nhỏ gọn, tốc độ cao — mặc định',
    availableVariants: ['int8', 'full'],
  },
  {
    id: 'gipformer-65m-rnnt',
    label: 'Gipformer 65M',
    hfRepo: 'g-group-ai-lab/gipformer-65M-rnnt',
    int8Size: '~75 MB',
    fullSize: '~335 MB',
    description: 'Chính xác hơn, cần máy mạnh hơn',
    availableVariants: ['int8', 'full'],
  },
  {
    id: 'sherpa-onnx-zipformer-vi-2025-04-20',
    label: 'Sherpa-ONNX Zipformer VI (2025)',
    hfRepo: 'csukuangfj/sherpa-onnx-zipformer-vi-2025-04-20',
    int8Size: 'Không có',
    fullSize: '~270 MB',
    description: 'Model cộng đồng, chỉ có bản full precision',
    availableVariants: ['full'],
  },
];

export interface VariantStatus {
  hasFiles: boolean;
  isLoaded: boolean;
}

export const AsrAPI = {
  init: (): Promise<void> => invoke('asr_init'),
  getModelStatus: (): Promise<ModelStatus> => invoke('asr_get_model_status'),
  isModelLoaded: (): Promise<boolean> => invoke('asr_is_model_loaded'),
  getModelsDirectory: (): Promise<string> => invoke('asr_get_models_directory'),
  downloadModel: (family: AsrModelFamily, variant: ModelVariant): Promise<void> =>
    invoke('asr_download_model', { family, variant }),
  loadModel: (
    family: AsrModelFamily,
    variant: ModelVariant,
    decodingMethod: DecodingMethod,
    numActivePaths: number
  ): Promise<void> =>
    invoke('asr_load_model', { family, variant, decodingMethod, numActivePaths }),
  getVariantStatus: (family: AsrModelFamily, variant: ModelVariant): Promise<VariantStatus> =>
    invoke('asr_get_variant_status', { family, variant }),
  validateModelReady: (
    family?: AsrModelFamily,
    variant?: ModelVariant,
    decodingMethod?: DecodingMethod,
    numActivePaths?: number
  ): Promise<string> =>
    invoke('asr_validate_model_ready', { family, variant, decodingMethod, numActivePaths }),
};

export const RoverAPI = {
  isModelLoaded: (): Promise<boolean> => invoke('rover_is_model_loaded'),
  getCurrentConfig: (): Promise<{
    isLoaded: boolean;
    familyA?: AsrModelFamily;
    variantA?: ModelVariant;
    familyB?: AsrModelFamily;
    variantB?: ModelVariant;
  }> => invoke('rover_get_current_config'),
  validateModelReady: (): Promise<string> => invoke('rover_validate_model_ready'),
};

export interface CpuTopology {
  physicalCores: number;
  logicalThreads: number;
}

export const CapuAPI = {
  getCpuTopology: (): Promise<CpuTopology> => invoke('capu_get_cpu_topology'),
};
