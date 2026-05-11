import React, { useState, useEffect } from 'react';
import { Switch } from '@/components/ui/switch';
import { FolderOpen, HardDrive, Bell, Info, AlertTriangle } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { DeviceSelection, SelectedDevices } from '@/components/DeviceSelection';
import Analytics from '@/lib/analytics';
import { toast } from 'sonner';

export interface RecordingPreferences {
  save_folder: string;
  auto_save: boolean;
  file_format: string;
  preferred_mic_device: string | null;
  preferred_system_device: string | null;
}

interface RecordingSettingsProps {
  onSave?: (preferences: RecordingPreferences) => void;
}

export function RecordingSettings({ onSave }: RecordingSettingsProps) {
  const [preferences, setPreferences] = useState<RecordingPreferences>({
    save_folder: '',
    auto_save: true,
    file_format: 'mp4',
    preferred_mic_device: null,
    preferred_system_device: null
  });
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [showRecordingNotification, setShowRecordingNotification] = useState(true);

  // Load recording preferences on component mount
  useEffect(() => {
    const loadPreferences = async () => {
      try {
        const prefs = await invoke<RecordingPreferences>('get_recording_preferences');
        setPreferences(prefs);
      } catch (error) {
        console.error('Failed to load recording preferences:', error);
        // If loading fails, get default folder path
        try {
          const defaultPath = await invoke<string>('get_default_recordings_folder_path');
          setPreferences(prev => ({ ...prev, save_folder: defaultPath }));
        } catch (defaultError) {
          console.error('Failed to get default folder path:', defaultError);
        }
      } finally {
        setLoading(false);
      }
    };

    loadPreferences();
  }, []);

  // Load recording notification preference
  useEffect(() => {
    const loadNotificationPref = async () => {
      try {
        const { Store } = await import('@tauri-apps/plugin-store');
        const store = await Store.load('preferences.json');
        const show = await store.get<boolean>('show_recording_notification') ?? true;
        setShowRecordingNotification(show);
      } catch (error) {
        console.error('Failed to load notification preference:', error);
      }
    };
    loadNotificationPref();
  }, []);

  const handleAutoSaveToggle = async (enabled: boolean) => {
    const newPreferences = { ...preferences, auto_save: enabled };
    setPreferences(newPreferences);
    await savePreferences(newPreferences);

    // Track auto-save setting change
    await Analytics.track('auto_save_recording_toggled', {
      enabled: enabled.toString()
    });
  };

  const handleDeviceChange = async (devices: SelectedDevices) => {
    const newPreferences = {
      ...preferences,
      preferred_mic_device: devices.micDevice,
      preferred_system_device: devices.systemDevice
    };
    setPreferences(newPreferences);
    await savePreferences(newPreferences);

    // Track default device preference changes
    // Note: Individual device selection analytics are tracked in DeviceSelection component
    await Analytics.track('default_devices_changed', {
      has_preferred_microphone: (!!devices.micDevice).toString(),
      has_preferred_system_audio: (!!devices.systemDevice).toString()
    });
  };

  const handleOpenFolder = async () => {
    try {
      await invoke('open_recordings_folder');
    } catch (error) {
      console.error('Failed to open recordings folder:', error);
    }
  };

  const handleNotificationToggle = async (enabled: boolean) => {
    try {
      setShowRecordingNotification(enabled);
      const { Store } = await import('@tauri-apps/plugin-store');
      const store = await Store.load('preferences.json');
      await store.set('show_recording_notification', enabled);
      await store.save();
      toast.success('Đã lưu tùy chọn');
      await Analytics.track('recording_notification_preference_changed', {
        enabled: enabled.toString()
      });
    } catch (error) {
      console.error('Failed to save notification preference:', error);
      toast.error('Không lưu được tùy chọn');
    }
  };

  const savePreferences = async (prefs: RecordingPreferences) => {
    setSaving(true);
    try {
      await invoke('set_recording_preferences', { preferences: prefs });
      onSave?.(prefs);

      // Show success toast with device details
      const micDevice = prefs.preferred_mic_device || 'Mặc định';
      const systemDevice = prefs.preferred_system_device || 'Mặc định';
      toast.success('Đã lưu thiết bị âm thanh', {
        description: `Micro: ${micDevice}, Âm thanh hệ thống: ${systemDevice}`
      });
    } catch (error) {
      console.error('Failed to save recording preferences:', error);
      toast.error('Không lưu được thiết bị âm thanh', {
        description: error instanceof Error ? error.message : String(error)
      });
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <div className="animate-pulse">
        <div className="h-4 bg-gray-200 rounded w-1/4 mb-4"></div>
        <div className="h-8 bg-gray-200 rounded mb-4"></div>
      </div>
    );
  }

  return (
    <div className="space-y-4">

      {/* ── Lưu file âm thanh ────────────────────────────────────────── */}
      <div className="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
        <div className="px-5 py-4 border-b border-gray-50">
          <h3 className="text-base font-semibold text-gray-900">Lưu file ghi âm</h3>
          <p className="text-sm text-gray-500 mt-1">Cấu hình cách lưu file âm thanh sau cuộc họp.</p>
        </div>

        {/* Auto save toggle */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-gray-50">
          <div>
            <p className="text-base font-medium text-gray-800">Tự động lưu</p>
            <p className="text-sm text-gray-500 mt-0.5">Tự động lưu file âm thanh khi dừng ghi</p>
          </div>
          <Switch
            checked={preferences.auto_save}
            onCheckedChange={handleAutoSaveToggle}
            disabled={saving}
          />
        </div>

        {/* Folder location */}
        {preferences.auto_save ? (
          <div className="px-5 py-4 space-y-3">
            <div className="flex items-start gap-3 p-4 rounded-lg bg-gray-50 border border-gray-100">
              <div className="w-9 h-9 rounded-lg bg-white border border-gray-200 flex items-center justify-center shrink-0 shadow-sm">
                <HardDrive className="w-4.5 h-4.5 text-gray-500" />
              </div>
              <div className="flex-1 min-w-0">
                <p className="text-base font-medium text-gray-700 mb-1">Thư mục lưu</p>
                <p className="text-sm text-gray-500 font-mono break-all leading-relaxed">
                  {preferences.save_folder || 'Thư mục mặc định'}
                </p>
              </div>
              <button
                onClick={handleOpenFolder}
                className="shrink-0 flex items-center gap-2 px-3.5 py-2 text-sm font-medium text-gray-600 bg-white border border-gray-200 rounded-lg hover:bg-gray-50 hover:border-gray-300 transition-colors shadow-sm"
              >
                <FolderOpen className="w-4 h-4" />
                Mở
              </button>
            </div>

            <div className="flex items-start gap-3 bg-blue-50 border border-blue-100 rounded-lg px-4 py-3">
              <Info className="w-4 h-4 text-[#16478e] shrink-0 mt-0.5" />
              <p className="text-sm text-[#16478e]">
                <span className="font-semibold">Định dạng:</span> {preferences.file_format.toUpperCase()} ·{' '}
                Lưu với dấu thời gian: <span className="font-mono">YYYYMMDD_HHMMSS.{preferences.file_format}</span>
              </p>
            </div>
          </div>
        ) : (
          <div className="px-5 py-4">
            <div className="flex items-start gap-3 bg-amber-50 border border-amber-100 rounded-lg px-4 py-3">
              <AlertTriangle className="w-4 h-4 text-amber-500 shrink-0 mt-0.5" />
              <p className="text-sm text-amber-700">
                Lưu file đang tắt. Bật <span className="font-semibold">Tự động lưu</span> để lưu âm thanh cuộc họp sau khi ghi.
              </p>
            </div>
          </div>
        )}
      </div>

      {/* ── Thông báo ghi ────────────────────────────────────────────── */}
      <div className="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
        <div className="px-5 py-4 border-b border-gray-50">
          <h3 className="text-base font-semibold text-gray-900">Thông báo</h3>
        </div>
        <div className="flex items-center justify-between px-5 py-4">
          <div className="flex items-start gap-3">
            <div className="w-8 h-8 rounded-lg bg-gray-100 flex items-center justify-center shrink-0 mt-0.5">
              <Bell className="w-4 h-4 text-gray-500" />
            </div>
            <div>
              <p className="text-base font-medium text-gray-800">Thông báo khi bắt đầu ghi</p>
              <p className="text-sm text-gray-500 mt-0.5">Nhắc nhở mọi người trong cuộc họp khi bắt đầu ghi âm</p>
            </div>
          </div>
          <Switch
            checked={showRecordingNotification}
            onCheckedChange={handleNotificationToggle}
          />
        </div>
      </div>

      {/* ── Thiết bị âm thanh ────────────────────────────────────────── */}
      <div className="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
        <div className="px-5 py-4 border-b border-gray-50">
          <h3 className="text-base font-semibold text-gray-900">Thiết bị âm thanh mặc định</h3>
          <p className="text-sm text-gray-500 mt-1">
            Micro và âm thanh hệ thống ưu tiên — được chọn sẵn khi bắt đầu ghi mới.
          </p>
        </div>
        <div className="px-5 py-4">
          <DeviceSelection
            selectedDevices={{
              micDevice: preferences.preferred_mic_device,
              systemDevice: preferences.preferred_system_device,
            }}
            onDeviceChange={handleDeviceChange}
            disabled={saving}
          />
        </div>
      </div>

    </div>
  );
}