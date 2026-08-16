import React, { useState, useEffect } from 'react';
import { Switch } from '@/components/ui/switch';
import { FolderOpen, HardDrive, Info, AlertTriangle, FolderInput } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { DeviceSelection, SelectedDevices } from '@/components/DeviceSelection';
import Analytics from '@/lib/analytics';
import { toast } from 'sonner';
import {
  AUDIO_CAPTURE_SOURCE_OPTIONS,
  AudioCaptureSource,
  parseAudioCaptureSource,
} from '@/lib/audioCaptureSource';
import { useConfig } from '@/contexts/ConfigContext';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Label } from '@/components/ui/label';

export interface RecordingPreferences {
  save_folder: string;
  auto_save: boolean;
  file_format: string;
  preferred_mic_device: string | null;
  preferred_system_device: string | null;
  audio_source?: AudioCaptureSource;
}

interface RecordingSettingsProps {
  onSave?: (preferences: RecordingPreferences) => void;
}

export function RecordingSettings({ onSave }: RecordingSettingsProps) {
  const { setSelectedDevices, setAudioCaptureSource } = useConfig();
  const [preferences, setPreferences] = useState<RecordingPreferences>({
    save_folder: '',
    auto_save: true,
    file_format: 'mp4',
    preferred_mic_device: null,
    preferred_system_device: null,
    audio_source: 'both',
  });
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    const loadPreferences = async () => {
      try {
        const prefs = await invoke<RecordingPreferences>('get_recording_preferences');
        setPreferences({
          ...prefs,
          audio_source: parseAudioCaptureSource(prefs.audio_source),
        });
      } catch (error) {
        console.error('Failed to load recording preferences:', error);
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

  const handleAutoSaveToggle = async (enabled: boolean) => {
    const newPreferences = { ...preferences, auto_save: enabled };
    setPreferences(newPreferences);
    await savePreferences(newPreferences, 'Đã lưu tùy chọn lưu file');

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
    setSelectedDevices(devices);
    await savePreferences(newPreferences, 'Đã lưu thiết bị âm thanh', {
      description: `Micro: ${devices.micDevice || 'Mặc định'}, Âm thanh hệ thống: ${devices.systemDevice || 'Mặc định'}`
    });

    await Analytics.track('default_devices_changed', {
      has_preferred_microphone: (!!devices.micDevice).toString(),
      has_preferred_system_audio: (!!devices.systemDevice).toString()
    });
  };

  const handleAudioSourceChange = async (source: AudioCaptureSource) => {
    const newPreferences = { ...preferences, audio_source: source };
    setPreferences(newPreferences);
    setAudioCaptureSource(source);
    await savePreferences(newPreferences, 'Đã lưu nguồn ghi âm', {
      description: AUDIO_CAPTURE_SOURCE_OPTIONS.find((option) => option.value === source)?.label,
    });
  };

  const handleOpenFolder = async () => {
    try {
      await invoke('open_recordings_folder');
      await Analytics.track('storage_folder_opened', { folder_type: 'recordings' });
    } catch (error) {
      console.error('Failed to open recordings folder:', error);
      toast.error('Không mở được thư mục');
    }
  };

  const handleSelectFolder = async () => {
    try {
      const selectedPath = await invoke<string | null>('select_recording_folder');
      if (!selectedPath) return;

      const newPreferences = { ...preferences, save_folder: selectedPath };
      setPreferences(newPreferences);
      await savePreferences(newPreferences, 'Đã cập nhật thư mục lưu', {
        description: selectedPath
      });

      await Analytics.track('recording_folder_changed', { folder_path: selectedPath });
    } catch (error) {
      console.error('Failed to select recordings folder:', error);
      toast.error('Không chọn được thư mục', {
        description: error instanceof Error ? error.message : String(error)
      });
    }
  };

  const savePreferences = async (
    prefs: RecordingPreferences,
    successMessage: string,
    options?: { description?: string }
  ) => {
    setSaving(true);
    try {
      await invoke('set_recording_preferences', { preferences: prefs });
      onSave?.(prefs);
      toast.success(successMessage, options?.description ? { description: options.description } : undefined);
    } catch (error) {
      console.error('Failed to save recording preferences:', error);
      toast.error('Không lưu được cài đặt', {
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

      {/* ── Thư mục lưu trữ ──────────────────────────────────────────── */}
      <div className="app-surface overflow-hidden">
        <div className="px-5 py-4 border-b border-rule">
          <h3 className="text-base font-semibold text-ink">Thư mục lưu trữ</h3>
          <p className="text-sm text-ink-2 mt-1">
            Chọn nơi lưu file ghi âm, transcript và metadata của mỗi cuộc họp.
          </p>
        </div>

        <div className="px-5 py-4 space-y-3">
          <div className="flex items-start gap-3 p-4 rounded-md bg-secondary border border-rule">
            <div className="w-9 h-9 rounded-md bg-paper-2 border border-rule flex items-center justify-center shrink-0">
              <HardDrive className="w-4.5 h-4.5 text-muted-foreground" />
            </div>
            <div className="flex-1 min-w-0">
              <p className="text-base font-medium text-ink mb-1">Thư mục lưu</p>
              <p className="text-sm text-ink-2 font-mono break-all leading-relaxed">
                {preferences.save_folder || 'Thư mục mặc định'}
              </p>
            </div>
            <div className="flex shrink-0 flex-col gap-2 sm:flex-row">
              <button
                onClick={handleSelectFolder}
                disabled={saving}
                className="flex items-center justify-center gap-2 px-3.5 py-2 text-sm font-medium text-primary-foreground bg-primary border border-primary rounded-md hover:bg-primary-hover transition-colors disabled:opacity-50"
              >
                <FolderInput className="w-4 h-4" />
                Chọn thư mục
              </button>
              <button
                onClick={handleOpenFolder}
                className="flex items-center justify-center gap-2 px-3.5 py-2 text-sm font-medium text-muted-foreground bg-paper-2 border border-input rounded-md hover:bg-secondary transition-colors"
              >
                <FolderOpen className="w-4 h-4" />
                Mở
              </button>
            </div>
          </div>

          <div className="flex items-start gap-3 bg-primary/10 border border-primary/20 rounded-md px-4 py-3">
            <Info className="w-4 h-4 text-primary shrink-0 mt-0.5" />
            <p className="text-sm text-primary leading-relaxed">
              Mỗi cuộc họp được lưu trong một thư mục riêng, chứa <span className="font-semibold">transcripts.json</span>
              {preferences.auto_save && (
                <> và file âm thanh <span className="font-mono">audio.{preferences.file_format}</span></>
              )}
              . Cơ sở dữ liệu và mô hình AI được lưu trong thư mục dữ liệu ứng dụng.
            </p>
          </div>
        </div>
      </div>

      {/* ── Lưu file âm thanh ────────────────────────────────────────── */}
      <div className="app-surface overflow-hidden">
        <div className="px-5 py-4 border-b border-rule">
          <h3 className="text-base font-semibold text-ink">Lưu file ghi âm</h3>
          <p className="text-sm text-ink-2 mt-1">Cấu hình cách lưu file âm thanh sau cuộc họp.</p>
        </div>

        <div
          className="flex items-center justify-between px-5 py-4 border-b border-rule"
        >
          <div>
            <p className="text-base font-medium text-ink">Tự động lưu</p>
            <p className="text-sm text-ink-2 mt-0.5">Tự động lưu file âm thanh khi dừng ghi (transcript luôn được lưu)</p>
          </div>
          <Switch
            checked={preferences.auto_save}
            onCheckedChange={handleAutoSaveToggle}
            disabled={saving}
          />
        </div>

        {!preferences.auto_save && (
          <div className="px-5 py-4">
            <div className="flex items-start gap-3 bg-amber-50 border border-amber-100 rounded-lg px-4 py-3">
              <AlertTriangle className="w-4 h-4 text-amber-500 shrink-0 mt-0.5" />
              <p className="text-sm text-amber-700">
                Lưu file âm thanh đang tắt. Transcript vẫn được lưu vào thư mục đã chọn ở trên.
              </p>
            </div>
          </div>
        )}
      </div>

      {/* ── Thiết bị âm thanh ────────────────────────────────────────── */}
      <div className="app-surface overflow-hidden">
        <div className="px-5 py-4 border-b border-rule">
          <h3 className="text-base font-semibold text-ink">Thiết bị âm thanh mặc định</h3>
          <p className="text-sm text-ink-2 mt-1">
            Micro và âm thanh hệ thống ưu tiên — được chọn sẵn khi bắt đầu ghi mới.
          </p>
        </div>
        <div className="px-5 py-4 space-y-4">
          <div className="space-y-2">
            <Label className="text-sm font-medium text-ink">Nguồn ghi âm</Label>
            <Select
              value={parseAudioCaptureSource(preferences.audio_source)}
              onValueChange={(value) => handleAudioSourceChange(value as AudioCaptureSource)}
              disabled={saving}
            >
              <SelectTrigger className="w-full">
                <SelectValue placeholder="Chọn nguồn ghi âm" />
              </SelectTrigger>
              <SelectContent>
                {AUDIO_CAPTURE_SOURCE_OPTIONS.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-xs text-ink-2">
              Chỉ thu micro, chỉ âm thanh hệ thống, hoặc cả hai. Áp dụng cho lần bấm Ghi kế tiếp.
            </p>
          </div>
          <DeviceSelection
            selectedDevices={{
              micDevice: preferences.preferred_mic_device,
              systemDevice: preferences.preferred_system_device,
            }}
            onDeviceChange={handleDeviceChange}
            disabled={saving}
            audioSource={parseAudioCaptureSource(preferences.audio_source)}
          />
        </div>
      </div>

    </div>
  );
}
