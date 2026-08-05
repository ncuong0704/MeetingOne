import { useEffect, RefObject } from 'react';
import { Virtualizer } from '@tanstack/react-virtual';
import { TranscriptSegmentData } from '@/types';

interface UsePlaybackFollowScrollProps {
  enabled: boolean;
  activeSegmentId: string | null;
  segments: TranscriptSegmentData[];
  scrollRef: RefObject<HTMLDivElement | null>;
  virtualizer?: Virtualizer<HTMLDivElement, Element>;
  useVirtualization: boolean;
}

function isElementInScrollContainer(
  el: HTMLElement,
  container: HTMLElement,
): boolean {
  const elRect = el.getBoundingClientRect();
  const cRect = container.getBoundingClientRect();
  return elRect.top >= cRect.top && elRect.bottom <= cRect.bottom;
}

export function usePlaybackFollowScroll({
  enabled,
  activeSegmentId,
  segments,
  scrollRef,
  virtualizer,
  useVirtualization,
}: UsePlaybackFollowScrollProps) {
  useEffect(() => {
    if (!enabled || !activeSegmentId) return;

    const index = segments.findIndex((s) => s.id === activeSegmentId);
    if (index < 0) return;

    if (useVirtualization && virtualizer) {
      const virtualItems = virtualizer.getVirtualItems();
      const first = virtualItems[0]?.index ?? 0;
      const last = virtualItems[virtualItems.length - 1]?.index ?? 0;
      if (index < first || index > last) {
        virtualizer.scrollToIndex(index, { align: 'center', behavior: 'smooth' });
      }
      return;
    }

    const container = scrollRef.current;
    const el = document.getElementById(`segment-${activeSegmentId}`);
    if (!container || !el) return;
    if (!isElementInScrollContainer(el, container)) {
      el.scrollIntoView({ behavior: 'smooth', block: 'center' });
    }
  }, [enabled, activeSegmentId, segments, scrollRef, virtualizer, useVirtualization]);
}
