'use client';

import React, { useEffect, useRef } from 'react';
import { useRecordingStop } from '@/hooks/useRecordingStop';
import { recordingService } from '@/services/recordingService';
import { subscribeSafely } from '@/lib/asyncSubscription';

const noop = () => {};

/** All stop sources observe the same durable backend completion. */
export function RecordingPostProcessingProvider({ children }: { children: React.ReactNode }) {
  const { handleRecordingStop } = useRecordingStop(noop, noop);
  const handler = useRef(handleRecordingStop);
  handler.current = handleRecordingStop;

  useEffect(() => {
    const dispose = subscribeSafely(
      () => recordingService.onRecordingStopped(() => { void handler.current(true); }),
      (error) => console.error('Could not observe recording completion', error),
    );
    // A reload may miss the event while Rust is still finalizing.
    let active = true;
    void recordingService.getLastRecordingResult().then((result) => {
      if (active && result) void handler.current(true);
    }).catch((error) => console.warn('Could not restore recording completion', error));
    return () => { active = false; dispose(); };
  }, []);

  return <>{children}</>;
}
