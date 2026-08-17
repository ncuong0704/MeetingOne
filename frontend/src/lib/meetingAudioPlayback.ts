/** Playback helpers: import stores 16 kHz PCM WAV, live recordings store audio.mp4. */

export function prefersBlobPlayback(filePath: string): boolean {
  return filePath.toLowerCase().endsWith('.wav');
}

export function audioMimeType(filePath: string): string {
  const ext = filePath.split('.').pop()?.toLowerCase();
  switch (ext) {
    case 'wav':
      return 'audio/wav';
    case 'mp3':
      return 'audio/mpeg';
    case 'mp4':
    case 'm4a':
      return 'audio/mp4';
    default:
      return 'application/octet-stream';
  }
}

export function isMissingAudioInvokeError(message: string): boolean {
  const msg = message.toLowerCase();
  return (
    msg.includes('no audio file') ||
    msg.includes('os error 2') ||
    msg.includes('no such file') ||
    msg.includes('cannot find')
  );
}

export function classifyMediaPlaybackError(
  code: number | undefined,
  fileResolved: boolean,
): 'FILE_NOT_FOUND' | 'PLAYBACK_FAILED' | 'Failed to load audio file' {
  if (!fileResolved) return 'FILE_NOT_FOUND';
  if (code === 2 || code === 4) return 'PLAYBACK_FAILED';
  return 'Failed to load audio file';
}
