import React from 'react';
import { AlertTriangle, Mic, Speaker, RefreshCw, Info } from 'lucide-react';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { invoke } from '@tauri-apps/api/core';
import { useIsLinux } from '@/hooks/usePlatform';

interface PermissionWarningProps {
  hasMicrophone: boolean;
  hasMicrophoneAccess: boolean;
  hasSystemAudio: boolean;
  onRecheck: () => void;
  isRechecking?: boolean;
}

export function PermissionWarning({
  hasMicrophone,
  hasMicrophoneAccess,
  hasSystemAudio,
  onRecheck,
  isRechecking = false
}: PermissionWarningProps) {
  const isLinux = useIsLinux();

  if (isLinux) {
    return null;
  }

  if (hasMicrophoneAccess && hasSystemAudio) {
    return null;
  }

  const isMacOS = navigator.userAgent.includes('Mac');
  const canRecordWithSystemOnly = hasSystemAudio && !hasMicrophoneAccess;
  const cannotRecord = !hasSystemAudio && !hasMicrophoneAccess;

  const openMicrophoneSettings = async () => {
    if (isMacOS) {
      try {
        await invoke('open_system_settings', { preferencePane: 'Privacy_Microphone' });
      } catch (error) {
        console.error('Failed to open microphone settings:', error);
      }
    }
  };

  const openScreenRecordingSettings = async () => {
    if (isMacOS) {
      try {
        await invoke('open_system_settings', { preferencePane: 'Privacy_ScreenCapture' });
      } catch (error) {
        console.error('Failed to open screen recording settings:', error);
      }
    }
  };

  if (canRecordWithSystemOnly) {
    return (
      <div className="max-w-md mb-4 space-y-3">
        <Alert className="border-blue-300 bg-blue-50">
          <Info className="h-5 w-5 text-blue-600" />
          <AlertTitle className="text-blue-900 font-semibold">
            <div className="flex items-center gap-2">
              <Speaker className="h-4 w-4" />
              Ghi âm chỉ âm thanh hệ thống
            </div>
          </AlertTitle>
          <AlertDescription className="text-blue-800 mt-2">
            <p className="mb-3">
              {hasMicrophone
                ? 'Micro chưa sẵn sàng hoặc chưa được cấp quyền. Bạn vẫn có thể ghi âm và phiên âm âm thanh phát từ máy (cuộc gọi, video, v.v.).'
                : 'Không phát hiện micro trên máy. Bạn vẫn có thể ghi âm và phiên âm âm thanh phát từ hệ thống.'}
            </p>
            {hasMicrophone && (
              <div className="flex flex-wrap gap-2 mt-3">
                {isMacOS && (
                  <button
                    onClick={openMicrophoneSettings}
                    className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors"
                  >
                    <Mic className="h-4 w-4" />
                    Mở cài đặt micro
                  </button>
                )}
                <button
                  onClick={onRecheck}
                  disabled={isRechecking}
                  className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-blue-900 bg-blue-100 hover:bg-blue-200 rounded-md transition-colors disabled:opacity-50"
                >
                  <RefreshCw className={`h-4 w-4 ${isRechecking ? 'animate-spin' : ''}`} />
                  Kiểm tra lại
                </button>
              </div>
            )}
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  return (
    <div className="max-w-md mb-4 space-y-3">
      <Alert variant="destructive" className="border-amber-400 bg-amber-50">
        <AlertTriangle className="h-5 w-5 text-amber-600" />
        <AlertTitle className="text-amber-900 font-semibold">
          <div className="flex items-center gap-2">
            {!hasMicrophoneAccess && <Mic className="h-4 w-4" />}
            {!hasSystemAudio && <Speaker className="h-4 w-4" />}
            {cannotRecord
              ? 'Không thể ghi âm'
              : !hasMicrophoneAccess
                ? 'Cần quyền micro'
                : 'Cần quyền âm thanh hệ thống'}
          </div>
        </AlertTitle>
        <div className="mt-4 flex flex-wrap gap-2">
          {isMacOS && !hasMicrophoneAccess && hasMicrophone && (
            <button
              onClick={openMicrophoneSettings}
              className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-amber-600 hover:bg-amber-700 rounded-md transition-colors"
            >
              <Mic className="h-4 w-4" />
              Mở cài đặt micro
            </button>
          )}
          {isMacOS && !hasSystemAudio && (
            <button
              onClick={openScreenRecordingSettings}
              className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md transition-colors"
            >
              <Speaker className="h-4 w-4" />
              Mở cài đặt ghi màn hình
            </button>
          )}
          <button
            onClick={onRecheck}
            disabled={isRechecking}
            className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-amber-900 bg-amber-100 hover:bg-amber-200 rounded-md transition-colors disabled:opacity-50"
          >
            <RefreshCw className={`h-4 w-4 ${isRechecking ? 'animate-spin' : ''}`} />
            Kiểm tra lại
          </button>
        </div>
        <AlertDescription className="text-amber-800 mt-2">
          {!hasMicrophoneAccess && !hasSystemAudio && (
            <>
              <p className="mb-3">
                Không phát hiện thiết bị âm thanh khả dụng. Cần ít nhất micro hoặc âm thanh hệ thống để ghi âm.
              </p>
              {!hasMicrophone && (
                <div className="space-y-2 text-sm mb-4">
                  <p className="font-medium">Về micro:</p>
                  <ul className="list-disc list-inside ml-2 space-y-1">
                    <li>Micro đã kết nối và bật nguồn</li>
                    <li>Đã cấp quyền micro trong Cài đặt hệ thống</li>
                  </ul>
                </div>
              )}
            </>
          )}

          {!hasMicrophoneAccess && hasSystemAudio === false && hasMicrophone && (
            <>
              <p className="mb-3">
                ACT MeetingOne cần quyền micro để ghi cuộc họp.
              </p>
              <div className="space-y-2 text-sm mb-4">
                <p className="font-medium">Vui lòng kiểm tra:</p>
                <ul className="list-disc list-inside ml-2 space-y-1">
                  <li>Micro đã kết nối và bật nguồn</li>
                  <li>Đã cấp quyền micro trong Cài đặt hệ thống</li>
                  <li>Không có ứng dụng khác độc chiếm micro</li>
                </ul>
              </div>
            </>
          )}

          {!hasSystemAudio && (
            <>
              <p className="mb-3">
                {hasMicrophoneAccess
                  ? 'Không thu được âm thanh hệ thống. Bạn vẫn có thể ghi bằng micro, nhưng âm thanh từ máy sẽ không được ghi.'
                  : 'Âm thanh hệ thống cũng không khả dụng.'}
              </p>
              {isMacOS && (
                <div className="space-y-2 text-sm mb-4">
                  <p className="font-medium">Để bật âm thanh hệ thống trên macOS:</p>
                  <ul className="list-disc list-inside ml-2 space-y-1">
                    <li>Cài thiết bị âm thanh ảo (ví dụ BlackHole 2ch)</li>
                    <li>Cấp quyền Ghi màn hình cho ACT MeetingOne</li>
                    <li>Cấu hình định tuyến âm thanh trong Audio MIDI Setup</li>
                  </ul>
                </div>
              )}
            </>
          )}
        </AlertDescription>
      </Alert>
    </div>
  );
}
