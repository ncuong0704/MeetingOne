import { invoke } from '@tauri-apps/api/core';

export type ModelStatus =
  | { type: 'NotLoaded' }
  | { type: 'Downloading'; value: number }
  | { type: 'Ready' }
  | { type: 'Error'; value: string };

export type AsrModelFamily =
  | 'zipformer-vi-30m'
  | 'gipformer-65m-rnnt'
  | 'sherpa-onnx-zipformer-vi-2025-04-20'
  | 'zipformer-vi-30m-streaming'
  | 'nghi-asr';
export type ModelVariant = 'int8' | 'full';
export type DecodingMethod = 'greedy_search' | 'modified_beam_search';
export type AsrPathKind = 'live' | 'file';
export type SttProvider = 'asr' | 'gemini';

export type AsrNotice = {
  description: string;
  warning?: string;
};

export interface AsrModelInfo {
  id: AsrModelFamily;
  label: string;
  hfRepo: string;
  int8Size: string;
  fullSize: string;
  /** Short family blurb (status cards / fallback). */
  description: string;
  availableVariants: ModelVariant[];
  variants: Partial<Record<ModelVariant, AsrNotice>>;
  liveVariants?: Partial<Record<ModelVariant, AsrNotice>>;
  liveOnly?: boolean;
}

export interface LiveAsrConfig {
  model: AsrModelFamily;
  asrVariant: ModelVariant;
  decodingMethod: DecodingMethod;
  numActivePaths: number;
  maxSegmentSeconds: number;
  provider?: SttProvider;
}

export interface FileAsrConfig {
  model: AsrModelFamily;
  asrVariant: ModelVariant;
  decodingMethod: DecodingMethod;
  numActivePaths: number;
  maxSegmentSeconds: number;
  roverEnabled: boolean;
  roverFamilyB?: AsrModelFamily | null;
  roverVariantB?: ModelVariant | null;
  provider?: SttProvider;
}

export interface SharedTranscriptConfig {
  hotwords?: string | null;
  capuCpuThreads?: number | null;
  capuPunctuationLevel: number;
  capuCaseLevel: number;
  diarizationEnabled?: boolean;
  diarizationNumSpeakers?: number | null;
}

export interface TranscriptConfigBundle {
  live: LiveAsrConfig;
  file: FileAsrConfig;
  shared: SharedTranscriptConfig;
}

export const ASR_MODELS: AsrModelInfo[] = [
  {
    id: 'zipformer-vi-30m-streaming',
    label: 'ZipFormer 30M Streaming',
    hfRepo: 'hynt/Zipformer-30M-RNNT-Streaming-6000h',
    int8Size: 'Không có',
    fullSize: '~51 MB',
    description: 'Transcript từng phần ngay khi nói — chỉ dùng khi ghi âm trực tiếp',
    availableVariants: ['full'],
    liveOnly: true,
    variants: {
      full: {
        description: 'Bản full (~51 MB): hiện chữ gần như tức thì khi đang nói.',
      },
    },
    liveVariants: {
      full: {
        description: 'Bản full (~51 MB): transcript từng phần ngay khi nói.',
        warning: 'Chỉ dùng khi ghi âm trực tiếp. Không dùng cho nhập file.',
      },
    },
  },
  {
    id: 'zipformer-vi-30m',
    label: 'ZipFormer 30M',
    hfRepo: 'hynt/Zipformer-30M-RNNT-6000h',
    int8Size: '~32 MB',
    fullSize: '~100 MB',
    description: 'Nhỏ gọn, tốc độ cao — mặc định',
    availableVariants: ['int8', 'full'],
    variants: {
      int8: {
        description: 'Bản int8 (~32 MB): nhỏ, nhanh — phù hợp máy vừa và mặc định nhập file.',
      },
      full: {
        description: 'Bản full (~100 MB): chính xác hơn int8, tốn RAM/CPU hơn.',
      },
    },
    liveVariants: {
      int8: {
        description: 'Bản int8 (~32 MB): nhỏ gọn, tốc độ cao — mặc định.',
        warning: 'Khuyến nghị cho ghi âm trực tiếp — nhanh, ít tốn CPU/RAM.',
      },
      full: {
        description: 'Bản full (~100 MB): chính xác hơn int8.',
        warning: 'Nặng hơn bản int8; cuộc họp dài có thể tốn CPU/RAM hơn.',
      },
    },
  },
  {
    id: 'gipformer-65m-rnnt',
    label: 'Gipformer 65M',
    hfRepo: 'g-group-ai-lab/gipformer-65M-rnnt',
    int8Size: '~75 MB',
    fullSize: '~335 MB',
    description: 'Chính xác hơn, cần máy mạnh hơn',
    availableVariants: ['int8', 'full'],
    variants: {
      int8: {
        description: 'Bản int8 (~75 MB): chính xác hơn ZipFormer 30M, cần máy mạnh hơn.',
      },
      full: {
        description: 'Bản full (~335 MB): chính xác nhất họ Gipformer.',
        warning: 'Rất nặng RAM/CPU — chỉ nên dùng khi máy đủ mạnh.',
      },
    },
    liveVariants: {
      int8: {
        description: 'Bản int8 (~75 MB): chính xác hơn ZipFormer 30M.',
        warning: 'Chậm hơn 30M; cuộc họp dài có thể tụt transcript real-time.',
      },
      full: {
        description: 'Bản full (~335 MB): chính xác nhất nhưng rất nặng.',
        warning: 'Không khuyến nghị khi ghi âm liên tục — dễ tụt transcript.',
      },
    },
  },
  {
    id: 'nghi-asr',
    label: 'NghiASR',
    hfRepo: 'NghiMe/NghiASR',
    int8Size: '~73 MB',
    fullSize: '~271 MB',
    description: 'Tiếng Việt đời thường và code-switch Anh–Việt',
    availableVariants: ['int8', 'full'],
    variants: {
      int8: {
        description: 'Bản int8 (~73 MB): Zipformer VietCasual, WER 8.22%.',
      },
      full: {
        description: 'Bản full (~271 MB): cùng checkpoint, chưa lượng tử.',
      },
    },
    liveVariants: {
      int8: {
        description: 'Bản int8 (~73 MB): Zipformer VietCasual, WER 8.22%.',
        warning: 'Nặng hơn 30M; cuộc họp dài có thể tụt transcript real-time.',
      },
      full: {
        description: 'Bản full (~271 MB): cùng checkpoint, chưa lượng tử.',
        warning: 'Không khuyến nghị khi ghi âm liên tục — dễ tụt transcript.',
      },
    },
  },
  {
    id: 'sherpa-onnx-zipformer-vi-2025-04-20',
    label: 'Sherpa-ONNX Zipformer VI (2025)',
    hfRepo: 'csukuangfj/sherpa-onnx-zipformer-vi-2025-04-20',
    int8Size: 'Không có',
    fullSize: '~270 MB',
    description: 'Model cộng đồng, chỉ có bản full precision',
    availableVariants: ['full'],
    variants: {
      full: {
        description: 'Chỉ có bản full (~270 MB): model cộng đồng, không có int8.',
      },
    },
    liveVariants: {
      full: {
        description: 'Chỉ có bản full (~270 MB): model cộng đồng.',
        warning: 'Không khuyến nghị khi ghi âm liên tục.',
      },
    },
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

export const TranscriptConfigAPI = {
  get: (): Promise<TranscriptConfigBundle> => invoke('api_get_transcript_config'),
  saveLive: (config: LiveAsrConfig): Promise<void> =>
    invoke('api_save_live_asr_config', {
      model: config.model,
      asrVariant: config.asrVariant,
      decodingMethod: config.decodingMethod,
      numActivePaths: config.numActivePaths,
      maxSegmentSeconds: config.maxSegmentSeconds,
      provider: config.provider ?? 'asr',
    }),
  saveFile: (config: FileAsrConfig): Promise<void> =>
    invoke('api_save_file_asr_config', {
      model: config.model,
      asrVariant: config.asrVariant,
      decodingMethod: config.decodingMethod,
      numActivePaths: config.numActivePaths,
      maxSegmentSeconds: config.maxSegmentSeconds,
      roverEnabled: config.roverEnabled,
      roverFamilyB: config.roverEnabled ? config.roverFamilyB : null,
      roverVariantB: config.roverEnabled ? config.roverVariantB : null,
      provider: config.provider ?? 'asr',
    }),
  getTranscriptApiKey: (provider: string): Promise<string> =>
    invoke('api_get_transcript_api_key', { provider }),
  saveTranscriptApiKey: (provider: string, apiKey: string): Promise<void> =>
    invoke('api_save_transcript_api_key', { provider, apiKey }),
  deleteTranscriptApiKey: (provider: string): Promise<void> =>
    invoke('api_delete_transcript_api_key', { provider }),
  saveShared: (config: SharedTranscriptConfig): Promise<void> =>
    invoke('api_save_shared_transcript_config', {
      hotwords: config.hotwords ?? null,
      capuCpuThreads: config.capuCpuThreads ?? null,
      capuPunctuationLevel: config.capuPunctuationLevel,
      capuCaseLevel: config.capuCaseLevel,
      diarizationEnabled: config.diarizationEnabled ?? false,
      diarizationNumSpeakers: config.diarizationNumSpeakers ?? null,
    }),
};

export const CapuAPI = {
  getCpuTopology: (): Promise<CpuTopology> => invoke('capu_get_cpu_topology'),
};

export type MeetingSpeaker = {
  id: string;
  meeting_id: string;
  cluster_index: number;
  display_name: string;
  color: string;
  preview_start: number | null;
};

export const DiarizationAPI = {
  renameSpeaker: (speakerId: string, displayName: string): Promise<void> =>
    invoke('rename_meeting_speaker', { speakerId, displayName }),
  mergeWithPrevious: (transcriptId: string): Promise<void> =>
    invoke('merge_speaker_segment', { transcriptId }),
  listSpeakers: (meetingId: string): Promise<MeetingSpeaker[]> =>
    invoke('list_meeting_speakers', { meetingId }),
  mergeSpeakers: (sourceSpeakerId: string, targetSpeakerId: string): Promise<void> =>
    invoke('merge_meeting_speakers', { sourceSpeakerId, targetSpeakerId }),
  isModelReady: (): Promise<boolean> => invoke('diarization_is_model_ready'),
};
