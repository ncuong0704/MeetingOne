export const SPEAKER_PREVIEW_SECONDS = 15;

export type DetectedSpeaker = {
  id: string;
  displayName: string;
  color: string;
  previewStart: number | null;
};

export type SpeakerSegmentHint = {
  speakerId?: string | null;
  speakerName?: string | null;
  speakerColor?: string | null;
  timestamp?: number | null;
};

export function previewStopTime(
  start: number,
  duration?: number,
  windowSeconds = SPEAKER_PREVIEW_SECONDS,
): number {
  const stop = start + windowSeconds;
  if (duration === undefined || !Number.isFinite(duration)) return stop;
  return Math.min(stop, duration);
}

export function listDetectedSpeakers(segments: SpeakerSegmentHint[]): DetectedSpeaker[] {
  const byId = new Map<string, DetectedSpeaker>();
  const order: string[] = [];

  for (const segment of segments) {
    const id = segment.speakerId?.trim();
    if (!id) continue;

    const timestamp =
      typeof segment.timestamp === 'number' && Number.isFinite(segment.timestamp)
        ? segment.timestamp
        : null;

    const existing = byId.get(id);
    if (!existing) {
      byId.set(id, {
        id,
        displayName: segment.speakerName?.trim() || 'Người nói',
        color: segment.speakerColor?.trim() || '#888888',
        previewStart: timestamp,
      });
      order.push(id);
      continue;
    }

    if (timestamp !== null && (existing.previewStart === null || timestamp < existing.previewStart)) {
      existing.previewStart = timestamp;
    }
  }

  return order.map((id) => byId.get(id)!);
}

export function mergeTargets(
  speakers: DetectedSpeaker[],
  sourceId: string,
): DetectedSpeaker[] {
  return speakers.filter((speaker) => speaker.id !== sourceId);
}

export function shouldShowSpeakerButton(speakers: DetectedSpeaker[]): boolean {
  return speakers.length > 0;
}
