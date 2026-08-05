import { useCallback, useEffect, useRef, useState } from 'react';
import { TranscriptSegmentData } from '@/types';
import {
  resolveActiveSegment,
  maxLoadedEndTime,
} from '@/lib/transcriptAudioSync';

const SUPPRESS_MS = 500;
const PLAYBACK_LOAD_DEBOUNCE_MS = 2000;
const MAX_LOAD_MORE_ATTEMPTS = 20;

interface UseTranscriptAudioSyncProps {
  segments: TranscriptSegmentData[];
  currentTime: number;
  isPlaybackActive: boolean;
  seekRef: React.MutableRefObject<((time: number) => void) | null>;
  hasMore?: boolean;
  onLoadMore?: () => Promise<void> | void;
}

export function useTranscriptAudioSync({
  segments,
  currentTime,
  isPlaybackActive,
  seekRef,
  hasMore,
  onLoadMore,
}: UseTranscriptAudioSyncProps) {
  const [activeSegmentId, setActiveSegmentId] = useState<string | null>(null);
  const suppressUntilRef = useRef(0);
  const lastLoadMoreRef = useRef(0);
  const rafRef = useRef<number | null>(null);
  const segmentsRef = useRef(segments);
  const hasMoreRef = useRef(hasMore);

  segmentsRef.current = segments;
  hasMoreRef.current = hasMore;

  useEffect(() => {
    if (!isPlaybackActive || segments.length === 0) {
      setActiveSegmentId(null);
      return;
    }

    if (Date.now() < suppressUntilRef.current) return;

    if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
    rafRef.current = requestAnimationFrame(() => {
      const id = resolveActiveSegment(segments, currentTime);
      setActiveSegmentId(id);
      rafRef.current = null;
    });

    return () => {
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
    };
  }, [segments, currentTime, isPlaybackActive]);

  useEffect(() => {
    if (!isPlaybackActive || !hasMore || !onLoadMore) return;
    const maxEnd = maxLoadedEndTime(segments);
    if (currentTime <= maxEnd) return;

    const now = Date.now();
    if (now - lastLoadMoreRef.current < PLAYBACK_LOAD_DEBOUNCE_MS) return;
    lastLoadMoreRef.current = now;
    onLoadMore();
  }, [currentTime, segments, hasMore, onLoadMore, isPlaybackActive]);

  const ensureSegmentLoaded = useCallback(
    async (segmentId: string): Promise<boolean> => {
      if (segmentsRef.current.some((s) => s.id === segmentId)) return true;
      if (!onLoadMore || !hasMoreRef.current) return false;

      for (let i = 0; i < MAX_LOAD_MORE_ATTEMPTS && hasMoreRef.current; i++) {
        await onLoadMore();
        if (segmentsRef.current.some((s) => s.id === segmentId)) return true;
      }
      return segmentsRef.current.some((s) => s.id === segmentId);
    },
    [onLoadMore],
  );

  const handleSegmentClick = useCallback(
    async (segment: TranscriptSegmentData) => {
      const t = segment.timestamp ?? 0;
      suppressUntilRef.current = Date.now() + SUPPRESS_MS;
      setActiveSegmentId(segment.id);

      await ensureSegmentLoaded(segment.id);
      seekRef.current?.(t);
    },
    [seekRef, ensureSegmentLoaded],
  );

  return { activeSegmentId, handleSegmentClick };
}
