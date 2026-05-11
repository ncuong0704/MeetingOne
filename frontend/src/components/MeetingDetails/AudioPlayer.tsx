"use client";

import { useAudioPlayer } from '@/hooks/useAudioPlayer';
import { Play, Pause, Loader2, MicOff, RotateCcw } from 'lucide-react';
import { useCallback } from 'react';

function formatTime(seconds: number): string {
  if (!isFinite(seconds) || seconds < 0) return '00:00';
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
}

interface AudioPlayerProps {
  /** Absolute path to the meeting folder on disk (audio file is resolved inside Rust). */
  meetingFolderPath: string;
  /** Optional: seek to a specific timestamp (called externally e.g. from transcript click) */
  seekRef?: React.MutableRefObject<((time: number) => void) | null>;
}

export function AudioPlayer({ meetingFolderPath, seekRef }: AudioPlayerProps) {
  const { isPlaying, currentTime, duration, error, play, pause, seek } = useAudioPlayer(meetingFolderPath);

  // Expose seek to parent via ref
  if (seekRef) {
    seekRef.current = seek;
  }

  const isLoaded = duration > 0;
  const progress = isLoaded ? Math.min((currentTime / duration) * 100, 100) : 0;

  const handleSeekClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (!isLoaded) return;
      const rect = e.currentTarget.getBoundingClientRect();
      const x = Math.max(0, Math.min(e.clientX - rect.left, rect.width));
      seek((x / rect.width) * duration);
    },
    [isLoaded, duration, seek],
  );

  const handleToggle = useCallback(async () => {
    if (isPlaying) pause();
    else await play();
  }, [isPlaying, play, pause]);

  if (error === 'FILE_NOT_FOUND') {
    return (
      <div className="flex items-center gap-2 border-b border-gray-100 bg-gray-50 px-4 py-2.5">
        <MicOff className="h-3.5 w-3.5 shrink-0 text-gray-400" />
        <span className="text-xs text-gray-500">Cuộc họp này chưa có file ghi âm được lưu</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center gap-2 border-b border-orange-100 bg-orange-50 px-4 py-2.5">
        <MicOff className="h-3.5 w-3.5 shrink-0 text-orange-500" />
        <span className="text-xs text-orange-600">Không thể phát audio: {error}</span>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-3 border-b border-gray-100 bg-gray-50/60 px-4 py-2.5">
      {/* Play / Pause / Loading */}
      <button
        onClick={handleToggle}
        disabled={!isLoaded}
        title={isPlaying ? 'Tạm dừng' : 'Phát audio'}
        className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-[#16478e] text-white shadow-sm transition-colors hover:bg-[#1a55ab] disabled:cursor-not-allowed disabled:opacity-40"
      >
        {!isLoaded ? (
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
        ) : isPlaying ? (
          <Pause className="h-3.5 w-3.5" fill="currentColor" />
        ) : (
          <Play className="h-3.5 w-3.5 translate-x-px" fill="currentColor" />
        )}
      </button>

      {/* Current time */}
      <span className="w-10 shrink-0 text-right font-mono text-xs text-gray-500 tabular-nums">
        {formatTime(currentTime)}
      </span>

      {/* Seek bar */}
      <div
        role="slider"
        aria-valuemin={0}
        aria-valuemax={duration}
        aria-valuenow={currentTime}
        aria-label="Vị trí phát"
        onClick={handleSeekClick}
        className={`group relative flex-1 ${isLoaded ? 'cursor-pointer' : 'cursor-default'}`}
      >
        {/* Track */}
        <div className="h-1.5 w-full overflow-hidden rounded-full bg-gray-200 group-hover:bg-gray-300 transition-colors">
          <div
            className="h-full rounded-full bg-[#16478e] transition-none"
            style={{ width: `${progress}%` }}
          />
        </div>
        {/* Thumb */}
        {isLoaded && (
          <div
            className="pointer-events-none absolute top-1/2 h-3.5 w-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white bg-[#16478e] shadow opacity-0 transition-opacity group-hover:opacity-100"
            style={{ left: `${progress}%` }}
          />
        )}
      </div>

      {/* Duration */}
      <span className="w-10 shrink-0 font-mono text-xs text-gray-400 tabular-nums">
        {formatTime(duration)}
      </span>

      {/* Back 10s shortcut */}
      {isLoaded && (
        <button
          onClick={() => seek(Math.max(0, currentTime - 10))}
          title="Tua lại 10 giây"
          className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-gray-400 hover:bg-gray-200 hover:text-gray-700 transition-colors"
        >
          <RotateCcw className="h-3.5 w-3.5" />
        </button>
      )}
    </div>
  );
}
