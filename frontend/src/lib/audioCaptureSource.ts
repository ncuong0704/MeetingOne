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

/** Recording is possible if system output exists or the mic is usable. */
export function canRecordWithDevices(
  hasSystemAudio: boolean,
  hasMicrophoneAccess: boolean,
): boolean {
  return hasSystemAudio || hasMicrophoneAccess;
}

/**
 * Downgrade or reject the chosen capture source when the microphone is missing
 * or the OS denied access.
 */
export function effectiveAudioSource(
  source: AudioCaptureSource,
  hasMicrophoneAccess: boolean,
): AudioCaptureSource | { error: string } {
  if (source === 'microphone' && !hasMicrophoneAccess) {
    return {
      error:
        'Không có quyền microphone. Chọn «Âm thanh hệ thống» hoặc cấp quyền micro.',
    };
  }
  if (source === 'both' && !hasMicrophoneAccess) {
    return 'system';
  }
  return source;
}
