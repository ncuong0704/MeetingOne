export type AudioCaptureSource = 'microphone' | 'system' | 'both';

export const AUDIO_CAPTURE_SOURCE_OPTIONS: {
  value: AudioCaptureSource;
  label: string;
  shortLabel: string;
}[] = [
  { value: 'microphone', label: 'Microphone', shortLabel: 'Micro' },
  { value: 'system', label: 'Âm thanh hệ thống', shortLabel: 'Hệ thống' },
  { value: 'both', label: 'Microphone + Âm thanh hệ thống', shortLabel: 'Cả hai' },
];

export function parseAudioCaptureSource(value: unknown): AudioCaptureSource {
  if (value === 'microphone' || value === 'system' || value === 'both') {
    return value;
  }
  return 'both';
}

export function wantsMicrophone(source: AudioCaptureSource): boolean {
  return source === 'microphone' || source === 'both';
}

export function wantsSystem(source: AudioCaptureSource): boolean {
  return source === 'system' || source === 'both';
}
