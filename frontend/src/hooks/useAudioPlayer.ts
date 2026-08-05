import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { convertFileSrc } from '@tauri-apps/api/core';

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

    const load = async () => {
      try {
        const filePath = await invoke<string>('resolve_meeting_audio_file_path', {
          folderPath: meetingFolderPath,
        });

        const url = convertFileSrc(filePath);
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
          // MEDIA_ERR_NETWORK (2) or MEDIA_ERR_SRC_NOT_SUPPORTED (4) → file not accessible
          setError(code === 2 || code === 4 ? 'FILE_NOT_FOUND' : 'Failed to load audio file');
        });
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        const notFound =
          msg.toLowerCase().includes('no audio file') ||
          msg.includes('os error 2') ||
          msg.toLowerCase().includes('no such file') ||
          msg.toLowerCase().includes('cannot find');
        setError(notFound ? 'FILE_NOT_FOUND' : 'Failed to load audio file');
      }
    };

    load();

    return () => {
      if (audio) {
        audio.pause();
        audio.src = '';
      }
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
