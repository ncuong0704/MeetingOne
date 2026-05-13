import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { useConfig } from '@/contexts/ConfigContext';
import { useRecordingState, RecordingStatus } from '@/contexts/RecordingStateContext';
import { recordingService } from '@/services/recordingService';
import Analytics from '@/lib/analytics';
import { showRecordingNotification } from '@/lib/recordingNotification';
import { toast } from 'sonner';

interface UseRecordingStartReturn {
  handleRecordingStart: () => Promise<void>;
  isAutoStarting: boolean;
}

/**
 * Custom hook for managing recording start lifecycle.
 * Handles both manual start (button click) and auto-start (from sidebar navigation).
 *
 * Features:
 * - Meeting title generation (format: Meeting DD_MM_YY_HH_MM_SS)
 * - Transcript clearing on start
 * - Analytics tracking
 * - Recording notification display
 * - Auto-start from sidebar via sessionStorage flag
 */
export function useRecordingStart(
  isRecording: boolean,
  setIsRecording: (value: boolean) => void,
  showModal?: (name: 'modelSelector', message?: string) => void,
  hasMicrophoneAccess: boolean = true
): UseRecordingStartReturn {
  const [isAutoStarting, setIsAutoStarting] = useState(false);

  const { clearTranscripts, setMeetingTitle } = useTranscripts();
  const { setIsMeetingActive } = useSidebar();
  const { selectedDevices, micEnabled } = useConfig();
  const { setStatus } = useRecordingState();

  // Generate meeting title with timestamp
  const generateMeetingTitle = useCallback(() => {
    const now = new Date();
    const day = String(now.getDate()).padStart(2, '0');
    const month = String(now.getMonth() + 1).padStart(2, '0');
    const year = String(now.getFullYear()).slice(-2);
    const hours = String(now.getHours()).padStart(2, '0');
    const minutes = String(now.getMinutes()).padStart(2, '0');
    const seconds = String(now.getSeconds()).padStart(2, '0');
    return `Cuộc họp ${day}_${month}_${year}_${hours}_${minutes}_${seconds}`;
  }, []);

  // Check if ZipFormer Vietnamese ASR model is ready
  const checkParakeetReady = useCallback(async (): Promise<boolean> => {
    try {
      await invoke('zipformer_init');
      return await invoke<boolean>('zipformer_is_model_loaded');
    } catch (error) {
      console.error('Failed to check ZipFormer status:', error);
      return false;
    }
  }, []);

  // Check if ZipFormer model is currently downloading
  const checkIfModelDownloading = useCallback(async (): Promise<boolean> => {
    try {
      const status = await invoke<{ type: string }>('zipformer_get_model_status');
      return status?.type === 'Downloading';
    } catch (error) {
      console.error('Failed to check ZipFormer download status:', error);
      return false;
    }
  }, []);

  // Handle manual recording start (from button click)
  const handleRecordingStart = useCallback(async () => {
    try {
      console.log('handleRecordingStart called - checking Parakeet model status');

      // Check if Parakeet transcription model is ready before starting
      const parakeetReady = await checkParakeetReady();
      if (!parakeetReady) {
        const isDownloading = await checkIfModelDownloading();
        if (isDownloading) {
          toast.info('Đang tải mô hình', {
            description: 'Vui lòng đợi mô hình nhận dạng tải xong trước khi ghi âm.',
            duration: 5000,
          });
          Analytics.trackButtonClick('start_recording_blocked_downloading', 'home_page');
        } else {
          toast.error('Mô hình nhận dạng chưa sẵn sàng', {
            description: 'Vui lòng tải mô hình nhận dạng trước khi ghi âm.',
            duration: 5000,
          });
          showModal?.('modelSelector', 'Cần cài đặt mô hình nhận dạng giọng nói');
          Analytics.trackButtonClick('start_recording_blocked_missing', 'home_page');
        }
        setStatus(RecordingStatus.IDLE);
        return;
      }

      console.log('Parakeet ready - setting up meeting title and state');

      const randomTitle = generateMeetingTitle();
      setMeetingTitle(randomTitle);

      // Set STARTING status before initiating backend recording
      setStatus(RecordingStatus.STARTING, 'Đang khởi tạo ghi âm...');

      // Start the actual backend recording
      const shouldUseMicrophone = hasMicrophoneAccess && micEnabled;
      console.log('Starting backend recording with meeting:', randomTitle, 'micEnabled:', shouldUseMicrophone);
      await recordingService.startRecordingWithDevices(
        shouldUseMicrophone ? (selectedDevices?.micDevice || null) : null,
        selectedDevices?.systemDevice || null,
        randomTitle,
        shouldUseMicrophone
      );
      console.log('Backend recording started successfully');

      // Update state after successful backend start
      // Note: RECORDING status will be set by RecordingStateContext event listener
      console.log('Setting isRecordingState to true');
      setIsRecording(true); // This will also update the sidebar via the useEffect
      clearTranscripts(); // Clear previous transcripts when starting new recording
      setIsMeetingActive(true);
      Analytics.trackButtonClick('start_recording', 'home_page');

      // Show recording notification if enabled
      await showRecordingNotification();
    } catch (error) {
      console.error('Failed to start recording:', error);
      setStatus(RecordingStatus.ERROR, error instanceof Error ? error.message : 'Không thể bắt đầu ghi âm');
      setIsRecording(false); // Reset state on error
      Analytics.trackButtonClick('start_recording_error', 'home_page');
      // Re-throw so RecordingControls can handle device-specific errors
      throw error;
    }
  }, [generateMeetingTitle, setMeetingTitle, setIsRecording, clearTranscripts, setIsMeetingActive, checkParakeetReady, checkIfModelDownloading, selectedDevices, micEnabled, hasMicrophoneAccess, showModal, setStatus]);

  // Check for autoStartRecording flag and start recording automatically
  useEffect(() => {
    const checkAutoStartRecording = async () => {
      if (typeof window !== 'undefined') {
        const shouldAutoStart = sessionStorage.getItem('autoStartRecording');
        if (shouldAutoStart === 'true' && !isRecording && !isAutoStarting) {
          console.log('Auto-starting recording from navigation...');
          setIsAutoStarting(true);
          sessionStorage.removeItem('autoStartRecording'); // Clear the flag

          // Check if Parakeet transcription model is ready before starting
          const parakeetReady = await checkParakeetReady();
          if (!parakeetReady) {
            const isDownloading = await checkIfModelDownloading();
            if (isDownloading) {
              toast.info('Đang tải mô hình', {
                description: 'Vui lòng đợi mô hình nhận dạng tải xong trước khi ghi âm.',
                duration: 5000,
              });
              Analytics.trackButtonClick('start_recording_blocked_downloading', 'sidebar_auto');
            } else {
              toast.error('Mô hình nhận dạng chưa sẵn sàng', {
                description: 'Vui lòng tải mô hình nhận dạng trước khi ghi âm.',
                duration: 5000,
              });
              showModal?.('modelSelector', 'Cần cài đặt mô hình nhận dạng giọng nói');
              Analytics.trackButtonClick('start_recording_blocked_missing', 'sidebar_auto');
            }
            setStatus(RecordingStatus.IDLE);
            setIsAutoStarting(false);
            return;
          }

          // Start the actual backend recording
          try {
            // Generate meeting title
            const generatedMeetingTitle = generateMeetingTitle();

            // Set STARTING status before initiating backend recording
            setStatus(RecordingStatus.STARTING, 'Đang khởi tạo ghi âm...');

            const shouldUseMicrophone = hasMicrophoneAccess && micEnabled;
            console.log('Auto-starting backend recording with meeting:', generatedMeetingTitle, 'micEnabled:', shouldUseMicrophone);
            const result = await recordingService.startRecordingWithDevices(
              shouldUseMicrophone ? (selectedDevices?.micDevice || null) : null,
              selectedDevices?.systemDevice || null,
              generatedMeetingTitle,
              shouldUseMicrophone
            );
            console.log('Auto-start backend recording result:', result);

            // Update UI state after successful backend start
            // Note: RECORDING status will be set by RecordingStateContext event listener
            setMeetingTitle(generatedMeetingTitle);
            setIsRecording(true);
            clearTranscripts();
            setIsMeetingActive(true);
            Analytics.trackButtonClick('start_recording', 'sidebar_auto');

            // Show recording notification if enabled
            await showRecordingNotification();
          } catch (error) {
            console.error('Failed to auto-start recording:', error);
            setStatus(RecordingStatus.ERROR, error instanceof Error ? error.message : 'Không thể tự động bắt đầu ghi âm');
            alert('Không thể bắt đầu ghi âm. Xem chi tiết trong console.');
            Analytics.trackButtonClick('start_recording_error', 'sidebar_auto');
          } finally {
            setIsAutoStarting(false);
          }
        }
      }
    };

    checkAutoStartRecording();
  }, [
    isRecording,
    isAutoStarting,
    selectedDevices,
    micEnabled,
    hasMicrophoneAccess,
    generateMeetingTitle,
    setMeetingTitle,
    setIsRecording,
    clearTranscripts,
    setIsMeetingActive,
    checkParakeetReady,
    checkIfModelDownloading,
    showModal,
    setStatus,
  ]);

  // Listen for direct recording trigger from sidebar when already on home page
  useEffect(() => {
    const handleDirectStart = async () => {
      if (isRecording || isAutoStarting) {
        console.log('Recording already in progress, ignoring direct start event');
        return;
      }

      console.log('Direct start from sidebar - checking Parakeet model status');
      setIsAutoStarting(true);

      // Check if Parakeet transcription model is ready before starting
      const parakeetReady = await checkParakeetReady();
      if (!parakeetReady) {
        const isDownloading = await checkIfModelDownloading();
        if (isDownloading) {
          toast.info('Đang tải mô hình', {
            description: 'Vui lòng đợi mô hình nhận dạng tải xong trước khi ghi âm.',
            duration: 5000,
          });
          Analytics.trackButtonClick('start_recording_blocked_downloading', 'sidebar_direct');
        } else {
          toast.error('Mô hình nhận dạng chưa sẵn sàng', {
            description: 'Vui lòng tải mô hình nhận dạng trước khi ghi âm.',
            duration: 5000,
          });
          showModal?.('modelSelector', 'Cần cài đặt mô hình nhận dạng giọng nói');
          Analytics.trackButtonClick('start_recording_blocked_missing', 'sidebar_direct');
        }
        setStatus(RecordingStatus.IDLE);
        setIsAutoStarting(false);
        return;
      }

      try {
        // Generate meeting title
        const generatedMeetingTitle = generateMeetingTitle();

        // Set STARTING status before initiating backend recording
        setStatus(RecordingStatus.STARTING, 'Đang khởi tạo ghi âm...');

        const shouldUseMicrophone = hasMicrophoneAccess && micEnabled;
        console.log('Starting backend recording with meeting:', generatedMeetingTitle, 'micEnabled:', shouldUseMicrophone);
        const result = await recordingService.startRecordingWithDevices(
          shouldUseMicrophone ? (selectedDevices?.micDevice || null) : null,
          selectedDevices?.systemDevice || null,
          generatedMeetingTitle,
          shouldUseMicrophone
        );
        console.log('Backend recording result:', result);

        // Update UI state after successful backend start
        // Note: RECORDING status will be set by RecordingStateContext event listener
        setMeetingTitle(generatedMeetingTitle);
        setIsRecording(true);
        clearTranscripts();
        setIsMeetingActive(true);
        Analytics.trackButtonClick('start_recording', 'sidebar_direct');

        // Show recording notification if enabled
        await showRecordingNotification();
      } catch (error) {
        console.error('Failed to start recording from sidebar:', error);
        setStatus(RecordingStatus.ERROR, error instanceof Error ? error.message : 'Không thể bắt đầu ghi âm từ thanh bên');
        alert('Không thể bắt đầu ghi âm. Xem chi tiết trong console.');
        Analytics.trackButtonClick('start_recording_error', 'sidebar_direct');
      } finally {
        setIsAutoStarting(false);
      }
    };

    window.addEventListener('start-recording-from-sidebar', handleDirectStart);

    return () => {
      window.removeEventListener('start-recording-from-sidebar', handleDirectStart);
    };
  }, [
    isRecording,
    isAutoStarting,
    selectedDevices,
    micEnabled,
    hasMicrophoneAccess,
    generateMeetingTitle,
    setMeetingTitle,
    setIsRecording,
    clearTranscripts,
    setIsMeetingActive,
    checkParakeetReady,
    checkIfModelDownloading,
    showModal,
    setStatus,
  ]);

  return {
    handleRecordingStart,
    isAutoStarting,
  };
}
