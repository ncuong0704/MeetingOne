import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface PermissionStatus {
  hasMicrophone: boolean;
  hasSystemAudio: boolean;
  micPermissionGranted: boolean;
  isChecking: boolean;
  error: string | null;
}

export function usePermissionCheck() {
  const [status, setStatus] = useState<PermissionStatus>({
    hasMicrophone: false,
    hasSystemAudio: false,
    micPermissionGranted: false,
    isChecking: true,
    error: null,
  });

  const checkPermissions = async () => {
    setStatus(prev => ({ ...prev, isChecking: true, error: null }));

    try {
      const devices = await invoke<Array<{ name: string; device_type: 'Input' | 'Output' }>>('get_audio_devices');

      const inputDevices = devices.filter(d => d.device_type === 'Input');
      const hasMicrophone = inputDevices.length > 0;

      const outputDevices = devices.filter(d => d.device_type === 'Output');
      const hasSystemAudio = outputDevices.length > 0;

      // Check actual OS-level mic permission by attempting to open a stream
      let micPermissionGranted = false;
      if (hasMicrophone) {
        try {
          micPermissionGranted = await invoke<boolean>('check_microphone_access');
        } catch {
          micPermissionGranted = false;
        }
      }

      console.log('Permission check:', {
        hasMicrophone,
        hasSystemAudio,
        micPermissionGranted,
        inputDevices: inputDevices.length,
        outputDevices: outputDevices.length
      });

      setStatus({
        hasMicrophone,
        hasSystemAudio,
        micPermissionGranted,
        isChecking: false,
        error: null,
      });

      return { hasMicrophone, hasSystemAudio, micPermissionGranted };
    } catch (error) {
      console.error('Failed to check audio permissions:', error);
      setStatus({
        hasMicrophone: false,
        hasSystemAudio: false,
        micPermissionGranted: false,
        isChecking: false,
        error: error instanceof Error ? error.message : 'Failed to check permissions',
      });
      return { hasMicrophone: false, hasSystemAudio: false, micPermissionGranted: false };
    }
  };

  const requestPermissions = async () => {
    try {
      await invoke('get_audio_devices');
      setTimeout(() => {
        checkPermissions();
      }, 1000);
    } catch (error) {
      console.error('Failed to request permissions:', error);
    }
  };

  useEffect(() => {
    checkPermissions();
  }, []);

  return {
    ...status,
    // Derived: mic is usable only when device exists AND OS permission granted
    hasMicrophoneAccess: status.hasMicrophone && status.micPermissionGranted,
    checkPermissions,
    requestPermissions,
  };
}
