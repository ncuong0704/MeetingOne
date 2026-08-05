import { TranscriptSegmentData } from '@/types';

const DEFAULT_SEGMENT_DURATION_SEC = 5;

export function segmentEndTime(segment: TranscriptSegmentData): number {
  const start = segment.timestamp ?? 0;
  return segment.endTime ?? start + DEFAULT_SEGMENT_DURATION_SEC;
}

/**
 * Find the segment that should be highlighted at playback time `t` (seconds).
 * Segments must be sorted by timestamp ascending.
 */
export function resolveActiveSegment(
  segments: TranscriptSegmentData[],
  t: number,
): string | null {
  if (!segments.length || t < 0) return null;

  let inWindowBest: TranscriptSegmentData | null = null;
  let inWindowBestStart = -Infinity;

  let futureBest: TranscriptSegmentData | null = null;
  let futureBestDistance = Infinity;

  let pastBest: TranscriptSegmentData | null = null;
  let pastBestStart = -Infinity;

  for (const segment of segments) {
    const start = segment.timestamp ?? 0;
    const end = segmentEndTime(segment);

    if (start <= t && t <= end) {
      if (start > inWindowBestStart) {
        inWindowBest = segment;
        inWindowBestStart = start;
      }
    }

    if (start > t) {
      const dist = start - t;
      if (dist < futureBestDistance) {
        futureBest = segment;
        futureBestDistance = dist;
      }
    }

    if (start <= t && start > pastBestStart) {
      pastBest = segment;
      pastBestStart = start;
    }
  }

  if (inWindowBest) return inWindowBest.id;
  if (futureBest) return futureBest.id;
  if (pastBest) return pastBest.id;
  return segments[segments.length - 1]?.id ?? null;
}

export function maxLoadedEndTime(segments: TranscriptSegmentData[]): number {
  let max = 0;
  for (const s of segments) {
    const end = segmentEndTime(s);
    if (end > max) max = end;
  }
  return max;
}
