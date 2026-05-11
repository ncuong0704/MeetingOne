import { invoke } from '@tauri-apps/api/core';

export type ModelStatus =
  | { type: 'NotLoaded' }
  | { type: 'Downloading'; value: number }
  | { type: 'Ready' }
  | { type: 'Error'; value: string };

export const ZipFormerAPI = {
  init: (): Promise<void> =>
    invoke('zipformer_init'),

  getModelStatus: (): Promise<ModelStatus> =>
    invoke('zipformer_get_model_status'),

  isModelLoaded: (): Promise<boolean> =>
    invoke('zipformer_is_model_loaded'),

  getModelsDirectory: (): Promise<string> =>
    invoke('zipformer_get_models_directory'),

  downloadModel: (): Promise<void> =>
    invoke('zipformer_download_model'),

  loadModel: (): Promise<void> =>
    invoke('zipformer_load_model'),

  transcribeAudio: (audioData: number[]): Promise<string> =>
    invoke('zipformer_transcribe_audio', { audioData }),

  validateModelReady: (): Promise<string> =>
    invoke('zipformer_validate_model_ready'),
};
