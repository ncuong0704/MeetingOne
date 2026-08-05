# Transcript Audio Sync Highlight Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Trên trang Chi tiết cuộc họp, highlight segment transcript theo vị trí phát audio, click dòng để seek, auto-scroll chỉ khi segment active ra khỏi viewport, hỗ trợ pagination.

**Architecture:** Pure `resolveActiveSegment` trong `lib/transcriptAudioSync.ts`; hook `useTranscriptAudioSync` nối `currentTime` ↔ `activeSegmentId`; `TranscriptPanel` wire `seekRef` + `onTimeUpdate`; `VirtualizedTranscriptView` nhận highlight + `playbackFollow` scroll.

**Tech Stack:** React 18, TypeScript, `@tanstack/react-virtual`, HTML5 Audio (`useAudioPlayer`).

**Reference spec:** `docs/superpowers/specs/2026-08-05-transcript-audio-sync-highlight-design.md`

---

## File map

| File | Change |
|---|---|
| `frontend/src/lib/transcriptAudioSync.ts` | New: pure resolve + helpers |
| `frontend/src/hooks/useTranscriptAudioSync.ts` | New: sync state, click, load-until |
| `frontend/src/hooks/usePlaybackFollowScroll.ts` | New: out-of-view scroll |
| `frontend/src/hooks/useAudioPlayer.ts` | Add `onTimeUpdate` optional callback |
| `frontend/src/components/MeetingDetails/AudioPlayer.tsx` | `onTimeUpdate` prop |
| `frontend/src/components/MeetingDetails/TranscriptPanel.tsx` | Wire hook, seekRef, auto-open player |
| `frontend/src/components/VirtualizedTranscriptView.tsx` | activeSegmentId, onSegmentClick, styling |

---

### Task 1: Pure `resolveActiveSegment`

**Files:**
- Create: `frontend/src/lib/transcriptAudioSync.ts`

- [ ] **Step 1: Create library file**

```typescript
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
```

- [ ] **Step 2: Typecheck**

Run: `cd frontend && pnpm exec tsc --noEmit`
Expected: PASS (no errors in new file)

---

### Task 2: `useAudioPlayer` + `AudioPlayer` expose time updates

**Files:**
- Modify: `frontend/src/hooks/useAudioPlayer.ts`
- Modify: `frontend/src/components/MeetingDetails/AudioPlayer.tsx`

- [ ] **Step 1: Add optional callback to hook**

In `useAudioPlayer.ts`, change signature:

```typescript
export const useAudioPlayer = (
  meetingFolderPath: string | null,
  onTimeUpdate?: (time: number) => void,
) => {
```

Inside `timeupdate` listener:

```typescript
audio.addEventListener('timeupdate', () => {
  const time = audio!.currentTime;
  setCurrentTime(time);
  onTimeUpdate?.(time);
});
```

Use a ref for callback to avoid re-subscribe on every render:

```typescript
const onTimeUpdateRef = useRef(onTimeUpdate);
onTimeUpdateRef.current = onTimeUpdate;
// in listener: onTimeUpdateRef.current?.(time);
```

- [ ] **Step 2: Pass through AudioPlayer**

```typescript
interface AudioPlayerProps {
  meetingFolderPath: string;
  seekRef?: React.MutableRefObject<((time: number) => void) | null>;
  onTimeUpdate?: (time: number) => void;
}

export function AudioPlayer({ meetingFolderPath, seekRef, onTimeUpdate }: AudioPlayerProps) {
  const { isPlaying, currentTime, duration, error, play, pause, seek } =
    useAudioPlayer(meetingFolderPath, onTimeUpdate);
  // ... rest unchanged
}
```

- [ ] **Step 3: Typecheck**

Run: `cd frontend && pnpm exec tsc --noEmit`
Expected: PASS

---

### Task 3: `useTranscriptAudioSync` hook

**Files:**
- Create: `frontend/src/hooks/useTranscriptAudioSync.ts`

- [ ] **Step 1: Implement hook**

```typescript
import { useCallback, useEffect, useRef, useState } from 'react';
import { TranscriptSegmentData } from '@/types';
import {
  resolveActiveSegment,
  maxLoadedEndTime,
} from '@/lib/transcriptAudioSync';

const SUPPRESS_MS = 500;
const PLAYBACK_LOAD_DEBOUNCE_MS = 2000;

interface UseTranscriptAudioSyncProps {
  segments: TranscriptSegmentData[];
  currentTime: number;
  isPlaybackActive: boolean; // player visible and audio loaded
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

  // Resolve active segment from currentTime (throttled via rAF)
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

  // Load more when playback moves past loaded range
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
      if (segments.some((s) => s.id === segmentId)) return true;
      if (!onLoadMore || !hasMore) return false;

      for (let i = 0; i < 20 && hasMore; i++) {
        await onLoadMore();
        if (segments.some((s) => s.id === segmentId)) return true;
      }
      return segments.some((s) => s.id === segmentId);
    },
    [segments, onLoadMore, hasMore],
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
```

Note: `ensureSegmentLoaded` may need parent to pass fresh `segments` after `loadMore` — if stale closure issue in practice, refactor to check DOM or use ref for segments in loop. Alternative fix in Step 2 below.

- [ ] **Step 2: Fix load-until with segments ref (if needed during implementation)**

```typescript
const segmentsRef = useRef(segments);
segmentsRef.current = segments;

// in ensureSegmentLoaded loop:
if (segmentsRef.current.some((s) => s.id === segmentId)) return true;
await onLoadMore();
```

- [ ] **Step 3: Typecheck**

Run: `cd frontend && pnpm exec tsc --noEmit`
Expected: PASS

---

### Task 4: `usePlaybackFollowScroll`

**Files:**
- Create: `frontend/src/hooks/usePlaybackFollowScroll.ts`

- [ ] **Step 1: Implement out-of-view scroll**

```typescript
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
```

- [ ] **Step 2: Typecheck**

Run: `cd frontend && pnpm exec tsc --noEmit`
Expected: PASS

---

### Task 5: `VirtualizedTranscriptView` — highlight + click

**Files:**
- Modify: `frontend/src/components/VirtualizedTranscriptView.tsx`

- [ ] **Step 1: Extend props**

```typescript
export interface VirtualizedTranscriptViewProps {
  // ... existing ...
  activeSegmentId?: string | null;
  onSegmentClick?: (segment: TranscriptSegmentData) => void;
  playbackFollow?: boolean;
}
```

- [ ] **Step 2: Update `TranscriptSegment`**

Add props `isActive`, `onRowClick`, pass `endTime` if needed.

On wrapper `div`:

```typescript
<div
  id={`segment-${id}`}
  className={`mb-3 group/seg rounded-md transition-colors ${
    isActive ? 'bg-[rgba(255,215,0,0.25)]' : ''
  } ${onRowClick ? 'cursor-pointer hover:bg-gray-50' : ''}`}
  onClick={() => !isEditing && onRowClick?.()}
>
```

Pencil button: `onClick={(e) => { e.stopPropagation(); handleStartEdit(); }}`

Timestamp when active: add `isActive ? 'text-[#16478e] font-semibold' : ''`

- [ ] **Step 3: Wire in main component**

Import `usePlaybackFollowScroll`.

Pass to segments in map:

```typescript
isActive={segment.id === activeSegmentId}
onRowClick={onSegmentClick ? () => onSegmentClick(segment) : undefined}
```

Call hook:

```typescript
usePlaybackFollowScroll({
  enabled: playbackFollow ?? false,
  activeSegmentId: activeSegmentId ?? null,
  segments,
  scrollRef,
  virtualizer,
  useVirtualization,
});
```

- [ ] **Step 4: Typecheck**

Run: `cd frontend && pnpm exec tsc --noEmit`
Expected: PASS

---

### Task 6: Wire `TranscriptPanel`

**Files:**
- Modify: `frontend/src/components/MeetingDetails/TranscriptPanel.tsx`

- [ ] **Step 1: Add refs and state**

```typescript
import { useRef, useState, useCallback } from 'react';
import { useTranscriptAudioSync } from '@/hooks/useTranscriptAudioSync';

const seekRef = useRef<((time: number) => void) | null>(null);
const [currentTime, setCurrentTime] = useState(0);
const [audioLoaded, setAudioLoaded] = useState(false);

const handleTimeUpdate = useCallback((t: number) => {
  setCurrentTime(t);
  setAudioLoaded(true);
}, []);
```

- [ ] **Step 2: Sync hook + auto-open player on click**

```typescript
const isPlaybackActive = showAudioPlayer && audioLoaded;

const { activeSegmentId, handleSegmentClick } = useTranscriptAudioSync({
  segments: convertedSegments,
  currentTime,
  isPlaybackActive,
  seekRef,
  hasMore,
  onLoadMore,
});

const onSegmentClick = useCallback(
  (segment: TranscriptSegmentData) => {
    if (!showAudioPlayer) setShowAudioPlayer(true);
    handleSegmentClick(segment);
  },
  [showAudioPlayer, handleSegmentClick],
);
```

- [ ] **Step 3: Pass props to children**

```typescript
<AudioPlayer
  meetingFolderPath={meetingFolderPath}
  seekRef={seekRef}
  onTimeUpdate={handleTimeUpdate}
/>

<VirtualizedTranscriptView
  // ... existing props ...
  activeSegmentId={activeSegmentId}
  onSegmentClick={meetingFolderPath ? onSegmentClick : undefined}
  playbackFollow={showAudioPlayer && !!meetingFolderPath}
/>
```

Reset `audioLoaded` when `meetingFolderPath` changes or player hidden (optional `useEffect`).

- [ ] **Step 4: Typecheck**

Run: `cd frontend && pnpm exec tsc --noEmit`
Expected: PASS

---

### Task 7: Manual verification

- [ ] **Step 1: Start dev app**

Run: `cd frontend && pnpm run tauri:dev` (or existing Windows script)

- [ ] **Step 2: Manual checklist** (from spec section 9)

1. Meeting with audio → bật player → play → highlight follows.
2. Click dòng → seek; pause stays paused.
3. Scroll off active → list scrolls; in view → no scroll.
4. >100 segments → play past 100th → loads more.
5. Click far segment → load until + seek.
6. Pencil → edit only, no seek.

- [ ] **Step 3: Document results** in PR or commit message when user requests commit.

---

## Plan self-review

| Spec requirement | Task |
|---|---|
| resolveActiveSegment algorithm | Task 1 |
| onTimeUpdate / currentTime | Task 2 |
| suppress 500ms after click | Task 3 |
| loadMore on playback + click | Task 3 |
| out-of-view scroll only | Task 4 |
| highlight UI + click row | Task 5 |
| meeting details only wire | Task 6 |
| manual tests | Task 7 |

No TBD placeholders. Type names consistent (`TranscriptSegmentData`, `activeSegmentId`).

---

## Execution handoff

Plan complete. Two execution options:

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks.
2. **Inline Execution** — implement all tasks in this session with checkpoints.

Which approach?
