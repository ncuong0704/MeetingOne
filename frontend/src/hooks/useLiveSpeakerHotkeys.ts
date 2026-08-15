'use client';

import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useRef, useState } from 'react';
import { getSpeakerHotkeys, insertLiveSpeaker, SpeakerHotkeys } from '@/lib/speakerHotkeys';

interface LiveSpeakerEvent {
  name: string | null;
  color: string | null;
}

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
  return target.isContentEditable;
}

export function useLiveSpeakerHotkeys(isRecording: boolean) {
  const [pendingName, setPendingName] = useState<string | null>(null);
  const [pendingColor, setPendingColor] = useState<string | null>(null);
  const hotkeysRef = useRef<SpeakerHotkeys>({});
  const pendingNameRef = useRef<string | null>(null);
  pendingNameRef.current = pendingName;

  const reloadHotkeys = useCallback(async () => {
    try {
      hotkeysRef.current = await getSpeakerHotkeys();
    } catch {
      hotkeysRef.current = {};
    }
  }, []);

  useEffect(() => {
    reloadHotkeys();
  }, [reloadHotkeys]);

  useEffect(() => {
    if (!isRecording) {
      setPendingName(null);
      setPendingColor(null);
    }
  }, [isRecording]);

  useEffect(() => {
    let unlistenPending: (() => void) | undefined;
    let unlistenChanged: (() => void) | undefined;
    listen<LiveSpeakerEvent>('live-speaker-pending', (event) => {
      setPendingName(event.payload.name);
      setPendingColor(event.payload.color);
    }).then((fn) => {
      unlistenPending = fn;
    });
    listen<{ speaker_name?: string | null }>('transcript-update', (event) => {
      const stamped = event.payload.speaker_name;
      if (stamped && stamped === pendingNameRef.current) {
        setPendingName(null);
        setPendingColor(null);
      }
    }).then((fn) => {
      unlistenChanged = fn;
    });
    return () => {
      unlistenPending?.();
      unlistenChanged?.();
    };
  }, []);

  useEffect(() => {
    if (!isRecording) return;

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.ctrlKey || event.altKey || event.metaKey) return;
      if (isTypingTarget(event.target)) return;
      if (!/^[1-9]$/.test(event.key)) return;
      const name = (hotkeysRef.current[event.key] ?? '').trim();
      if (!name) return;
      event.preventDefault();
      insertLiveSpeaker(name).catch(console.error);
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [isRecording]);

  return { pendingName, pendingColor, reloadHotkeys };
}
