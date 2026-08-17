import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { convertFileSrc } from '@tauri-apps/api/core';
import {
  audioMimeType,
  classifyMediaPlaybackError,
  isMissingAudioInvokeError,
  prefersBlobPlayback,
} from '@/lib/meetingAudioPlayback';

/** `meetingFolderPath`: absolute folder where meeting audio was saved (file name resolved in Rust). */
export const useAudioPlayer = (
  meetingFolderPath: string | null,
  onTimeUpdate?: (time: number) => void,
) => {
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const onTimeUpdateRef = useRef(onTimeUpdate);
  onTimeUpdateRef.current = onTimeUpdate;

  useEffect(() => {
    if (!meetingFolderPath) return;

    let audio: HTMLAudioElement | null = null;
    let objectUrl: string | null = null;
    let cancelled = false;

    const load = async () => {
      let fileResolved = false;
      try {
        const filePath = await invoke<string>('resolve_meeting_audio_file_path', {
          folderPath: meetingFolderPath,
        });
        if (cancelled) return;
        fileResolved = true;

        let url = convertFileSrc(filePath);
        if (prefersBlobPlayback(filePath)) {
          const bytes = await invoke<number[]>('read_meeting_audio_file', {
            folderPath: meetingFolderPath,
          });
          if (cancelled) return;
          objectUrl = URL.createObjectURL(
            new Blob([new Uint8Array(bytes)], { type: audioMimeType(filePath) }),
          );
          url = objectUrl;
        }

        if (cancelled) {
          if (objectUrl) URL.revokeObjectURL(objectUrl);
          return;
        }

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
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        setError(isMissingAudioInvokeError(msg) ? 'FILE_NOT_FOUND' : 'Failed to load audio file');
      }
    };

    load();

    return () => {
      cancelled = true;
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

  return { isPlaying, currentTime, duration, error, play, pause, seek };
};
