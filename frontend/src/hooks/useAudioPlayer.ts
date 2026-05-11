import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';

/** `meetingFolderPath`: absolute folder where meeting audio was saved (file name resolved in Rust). */
export const useAudioPlayer = (meetingFolderPath: string | null) => {
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const audioRef = useRef<AudioContext | null>(null);
  const sourceRef = useRef<AudioBufferSourceNode | null>(null);
  const startTimeRef = useRef<number>(0);
  const audioBufferRef = useRef<AudioBuffer | null>(null);
  const rafRef = useRef<number>();
  const seekTimeRef = useRef<number>(0);
  /** Đồng bộ với Web Audio (state React có thể lệch khi seek nhanh). */
  const isPlayingRef = useRef(false);

  const initAudioContext = async () => {
    try {
      if (!audioRef.current) {
        console.log('Creating new AudioContext');
        const AudioContextClass = window.AudioContext || (window as any).webkitAudioContext;
        audioRef.current = new AudioContextClass();
        console.log('AudioContext created:', {
          state: audioRef.current.state,
          sampleRate: audioRef.current.sampleRate,
        });
      }

      if (audioRef.current.state === 'suspended') {
        console.log('Resuming suspended AudioContext');
        await audioRef.current.resume();
        console.log('AudioContext resumed:', audioRef.current.state);
      }
      
      setError(null);
      return true;
    } catch (error) {
      console.error('Error initializing AudioContext:', error);
      setError('Failed to initialize audio');
      return false;
    }
  };

  // Cleanup function
  useEffect(() => {
    return () => {
      console.log('Cleaning up audio resources');
      if (rafRef.current) {
        cancelAnimationFrame(rafRef.current);
      }
      if (sourceRef.current) {
        sourceRef.current.stop();
      }
      if (audioRef.current) {
        audioRef.current.close();
      }
    };
  }, []);

  const loadAudio = async () => {
    if (!meetingFolderPath) {
      console.log('No meeting folder path provided');
      return;
    }

    let resolvedAudioPath: string | null = null;

    try {
      // Initialize context first
      const initialized = await initAudioContext();
      if (!initialized || !audioRef.current) {
        console.error('Failed to initialize audio context');
        return;
      }

      resolvedAudioPath = await invoke<string>('resolve_meeting_audio_file_path', {
        folderPath: meetingFolderPath,
      });

      console.log('Loading audio from:', resolvedAudioPath);
      
      // Read the file using Tauri command
      const result = await invoke<number[]>('read_audio_file', { 
        filePath: resolvedAudioPath 
      });
      
      if (!result || result.length === 0) {
        throw new Error('Empty audio data received');
      }
      
      console.log('Audio file read, size:', result.length, 'bytes');
      
      // Create a copy of the audio data
      const audioData = new Uint8Array(result).buffer;
      
      console.log('Created audio buffer, size:', audioData.byteLength, 'bytes');
      
      // Decode the audio data
      const audioBuffer = await new Promise<AudioBuffer>((resolve, reject) => {
        audioRef.current!.decodeAudioData(
          audioData,
          buffer => {
            console.log('Audio decoded successfully:', {
              duration: buffer.duration,
              sampleRate: buffer.sampleRate,
              numberOfChannels: buffer.numberOfChannels,
              length: buffer.length
            });
            resolve(buffer);
          },
          error => {
            console.error('Audio decoding failed:', error);
            reject(new Error('Failed to decode audio data: ' + error));
          }
        );
      });
      
      audioBufferRef.current = audioBuffer;
      setDuration(audioBuffer.duration);
      setCurrentTime(0);
      setError(null);
      console.log('Audio loaded and ready to play');
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      const low = msg.toLowerCase();
      const isNotFound =
        msg.includes('os error 2') ||
        low.includes('cannot find') ||
        low.includes('no such file') ||
        low.includes('no audio file found');

      if (isNotFound) {
        console.warn('Audio file not found:', resolvedAudioPath ?? meetingFolderPath);
        setError('FILE_NOT_FOUND');
      } else {
        console.error('Error loading audio:', error);
        setError('Failed to load audio file');
      }
    }
  };

  // Load audio when meeting folder changes
  useEffect(() => {
    console.log('Meeting folder path changed:', meetingFolderPath);
    if (meetingFolderPath) {
      loadAudio();
    }
  }, [meetingFolderPath]);

  const stopPlayback = () => {
    console.log('Stopping playback');
    if (rafRef.current) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = undefined;
    }
    if (sourceRef.current) {
      try {
        const src = sourceRef.current;
        // Tránh onended khi stop() do tua/pause — handler cũ có thể gọi stopPlayback/setCurrentTime(0) và phá luồng seek.
        src.onended = null;
        src.stop();
        src.disconnect();
      } catch (e) {
        console.log('Error stopping source:', e);
      }
      sourceRef.current = null;
    }
    isPlayingRef.current = false;
    setIsPlaying(false);
  };

  const play = async () => {
    console.log('Play requested');
    
    try {
      // Initialize context if needed
      const initialized = await initAudioContext();
      if (!initialized) {
        throw new Error('Audio context initialization failed');
      }
      if (!audioRef.current) {
        throw new Error('Audio context is null after initialization');
      }
      if (!audioBufferRef.current) {
        throw new Error('No audio buffer loaded - try loading the audio file first');
      }
      if (audioRef.current.state !== 'running') {
        throw new Error(`Audio context is in invalid state: ${audioRef.current.state}`);
      }

      // Stop any existing playback
      stopPlayback();

      // Create and setup new source
      console.log('Creating new audio source');
      sourceRef.current = audioRef.current.createBufferSource();
      sourceRef.current.buffer = audioBufferRef.current;
      
      console.log('Audio buffer details:', {
        duration: audioBufferRef.current.duration,
        sampleRate: audioBufferRef.current.sampleRate,
        numberOfChannels: audioBufferRef.current.numberOfChannels,
        length: audioBufferRef.current.length
      });
      
      sourceRef.current.connect(audioRef.current.destination);
      
      // Setup ended callback
      sourceRef.current.onended = () => {
        console.log('Playback ended naturally');
        stopPlayback();
        // onended có thể chạy trước RAF — khi đó updateTime thoát sớm (sourceRef null) và không reset seekTimeRef; Play lại sẽ start gần cuối buffer và “lỗi”.
        seekTimeRef.current = 0;
        setCurrentTime(0);
      };
      
      // Start playback from the seek time (offset phải < duration buffer)
      const buffer = audioBufferRef.current;
      const maxOffset = Math.max(0, buffer.duration - 1e-6);
      const startTime = Math.min(Math.max(0, seekTimeRef.current), maxOffset);
      seekTimeRef.current = startTime;
      startTimeRef.current = audioRef.current.currentTime - startTime;
      console.log('Starting playback', {
        startTime,
        contextTime: audioRef.current.currentTime,
        seekTime: seekTimeRef.current
      });
      
      sourceRef.current.start(0, startTime);
      isPlayingRef.current = true;
      setIsPlaying(true);
      setError(null);

      // Setup time update
      const updateTime = () => {
        if (!audioRef.current || !sourceRef.current) {
          console.log('Update cancelled - context or source is null');
          return;
        }
        
        const newTime = audioRef.current.currentTime - startTimeRef.current;
        
        if (newTime >= duration) {
          console.log('Playback finished');
          stopPlayback();
          setCurrentTime(0);
          seekTimeRef.current = 0;
        } else {
          setCurrentTime(newTime);
          seekTimeRef.current = newTime;
          rafRef.current = requestAnimationFrame(updateTime);
        }
      };
      
      rafRef.current = requestAnimationFrame(updateTime);
    } catch (error) {
      console.error('Error during playback:', error);
      setError('Failed to play audio');
      stopPlayback();
    }
  };

  const seek = async (time: number) => {
    console.log('Seek requested:', time);
    if (time < 0) time = 0;
    if (time > duration) time = duration;
    
    const wasPlaying = isPlayingRef.current;
    
    // Stop current playback
    stopPlayback();
    
    // Update both current time and seek time reference
    seekTimeRef.current = time;
    setCurrentTime(time);
    
    // If it was playing before, restart playback at new position
    if (wasPlaying) {
      console.log('Restarting playback at:', time);
      await play();
    }
  };

  const pause = () => {
    console.log('Pause requested');
    stopPlayback();
  };

  return {
    isPlaying,
    currentTime,
    duration,
    error,
    play,
    pause,
    seek
  };
};
