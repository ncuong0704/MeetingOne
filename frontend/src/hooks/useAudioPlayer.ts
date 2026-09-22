import { useState, useEffect, useRef } from 'react';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  audioMimeType,
  classifyMediaPlaybackError,
  isMissingAudioInvokeError,
  prefersBlobPlayback,
} from '@/lib/meetingAudioPlayback';

type MeetingAudioResolution =
  | { status: 'ready'; path: string }
  | { status: 'preparing' };

type MeetingAudioStatusEvent = {
  folderPath: string;
  ready: boolean;
  path?: string;
  error?: string;
};

/** While preparing, re-resolve this often as a safety net in case a status event is missed. */
const PREPARING_RECHECK_MS = 5000;

/** `meetingFolderPath`: absolute folder where meeting audio was saved (file name resolved in Rust). */
export const useAudioPlayer = (
  meetingFolderPath: string | null,
  onTimeUpdate?: (time: number) => void,
) => {
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [isPreparing, setIsPreparing] = useState(false);
  const [prepareProgress, setPrepareProgress] = useState(0);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const onTimeUpdateRef = useRef(onTimeUpdate);
  onTimeUpdateRef.current = onTimeUpdate;

  useEffect(() => {
    if (!meetingFolderPath) return;

    let audio: HTMLAudioElement | null = null;
    let objectUrl: string | null = null;
    let cancelled = false;
    let settled = false;
    let recheckTimer: ReturnType<typeof setTimeout> | null = null;

    const clearRecheck = () => {
      if (recheckTimer) {
        clearTimeout(recheckTimer);
        recheckTimer = null;
      }
    };

    const scheduleRecheck = () => {
      if (cancelled || settled || recheckTimer) return;
      recheckTimer = setTimeout(() => {
        recheckTimer = null;
        if (!cancelled && !settled) load();
      }, PREPARING_RECHECK_MS);
    };

    const attachAudio = (url: string, fileResolved: boolean) => {
      audio = new Audio(url);
      audioRef.current = audio;

      audio.addEventListener('loadedmetadata', () => {
        setDuration(audio!.duration);
        setError(null);
      });

      audio.addEventListener('timeupdate', () => {
        const time = audio!.currentTime;
        setCurrentTime(time);
        onTimeUpdateRef.current?.(time);
      });

      audio.addEventListener('ended', () => {
        setIsPlaying(false);
        setCurrentTime(0);
        audio!.currentTime = 0;
      });

      audio.addEventListener('error', () => {
        const code = audio?.error?.code;
        setError(classifyMediaPlaybackError(code, fileResolved));
      });
    };

    const load = async () => {
      let fileResolved = false;
      try {
        const resolved = await invoke<MeetingAudioResolution>('resolve_meeting_audio_file_path', {
          folderPath: meetingFolderPath,
        });
        if (cancelled) return;

        if (resolved.status === 'preparing') {
          // Rust is transcoding the long WAV in the background (CPU-capped); it will
          // emit meeting-audio-status when playable. The recheck timer is the safety
          // net if that event is ever missed.
          setIsPreparing(true);
          setPrepareProgress(0);
          scheduleRecheck();
          return;
        }
        clearRecheck();
        setIsPreparing(false);
        fileResolved = true;

        let url = convertFileSrc(resolved.path);
        if (prefersBlobPlayback(resolved.path)) {
          try {
            // Raw IPC response: an ArrayBuffer (number[] on older cores) — either way
            // Uint8Array accepts it without a giant JSON-array parse in WebView2.
            const bytes = await invoke<ArrayBuffer | number[]>('read_meeting_audio_file', {
              folderPath: meetingFolderPath,
            });
            if (cancelled) return;
            objectUrl = URL.createObjectURL(
              new Blob([new Uint8Array(bytes)], { type: audioMimeType(resolved.path) }),
            );
            url = objectUrl;
          } catch {
            // Too large for a blob or raw IPC unavailable: stream via convertFileSrc.
          }
        }

        if (cancelled) {
          if (objectUrl) URL.revokeObjectURL(objectUrl);
          return;
        }

        attachAudio(url, fileResolved);
        settled = true;
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        setError(isMissingAudioInvokeError(msg) ? 'FILE_NOT_FOUND' : 'Failed to load audio file');
      }
    };

    let unlistenStatus: (() => void) | null = null;
    let unlistenProgress: (() => void) | null = null;

    // Subscribe before the first resolve so the ready event can never be missed.
    (async () => {
      try {
        unlistenStatus = await listen<MeetingAudioStatusEvent>('meeting-audio-status', (event) => {
          if (cancelled || settled || event.payload.folderPath !== meetingFolderPath) return;
          clearRecheck();
          setIsPreparing(false);
          if (event.payload.ready) {
            load();
          } else {
            setError('PREPARE_FAILED');
          }
        });
        unlistenProgress = await listen<{ folderPath: string; percent: number }>(
          'meeting-audio-progress',
          (event) => {
            if (cancelled || event.payload.folderPath !== meetingFolderPath) return;
            setPrepareProgress(Math.max(0, Math.min(100, Math.round(event.payload.percent))));
          },
        );
      } catch {
        // Event channel unavailable: the periodic recheck keeps the UI correct.
      }
      load();
    })();

    return () => {
      cancelled = true;
      clearRecheck();
      unlistenStatus?.();
      unlistenProgress?.();
      if (audio) {
        audio.pause();
        audio.src = '';
      }
      if (objectUrl) URL.revokeObjectURL(objectUrl);
      audioRef.current = null;
      setIsPlaying(false);
      setCurrentTime(0);
      setDuration(0);
      setError(null);
      setIsPreparing(false);
      setPrepareProgress(0);
    };
  }, [meetingFolderPath]);

  const play = async () => {
    if (!audioRef.current) return;
    try {
      await audioRef.current.play();
      setIsPlaying(true);
    } catch {
      setError('Failed to play audio');
    }
  };

  const pause = () => {
    audioRef.current?.pause();
    setIsPlaying(false);
  };

  const seek = (time: number) => {
    if (!audioRef.current || !duration) return;
    audioRef.current.currentTime = Math.max(0, Math.min(time, duration));
    setCurrentTime(audioRef.current.currentTime);
  };

  return { isPlaying, currentTime, duration, error, isPreparing, prepareProgress, play, pause, seek };
};
