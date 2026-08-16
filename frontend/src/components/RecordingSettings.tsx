import React, { useState, useEffect } from 'react';
import { Switch } from '@/components/ui/switch';
import { FolderOpen, FolderInput } from 'lucide-react';
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
      <div className="max-w-xl space-y-6 animate-pulse">
        <div className="h-28 bg-paper-3 rounded-md" />
        <div className="h-40 bg-paper-3 rounded-md" />
      </div>
    );
  }

  return (
    <div className="max-w-xl space-y-6">
      <section>
        <h2 className="text-sm font-semibold text-ink tracking-tight mb-2">Lưu trữ</h2>
        <div className="app-surface divide-y divide-rule overflow-hidden">
          <div className="px-4 py-3 flex items-center gap-3">
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium text-ink">Thư mục</p>
              <p
                className="mt-0.5 text-xs font-mono text-ink-2 truncate"
                title={preferences.save_folder || undefined}
              >
                {preferences.save_folder || 'Thư mục mặc định'}
              </p>
            </div>
            <div className="flex shrink-0 gap-1.5">
              <button
                type="button"
                onClick={handleSelectFolder}
                disabled={saving}
                className="inline-flex h-8 items-center gap-1.5 rounded-md border border-rule bg-paper-2 px-2.5 text-xs font-medium text-ink hover:bg-secondary disabled:opacity-50"
              >
                <FolderInput className="h-3.5 w-3.5" />
                Chọn
              </button>
              <button
                type="button"
                onClick={handleOpenFolder}
                className="inline-flex h-8 items-center gap-1.5 rounded-md border border-rule bg-paper-2 px-2.5 text-xs font-medium text-ink-2 hover:bg-secondary hover:text-ink"
              >
                <FolderOpen className="h-3.5 w-3.5" />
                Mở
              </button>
            </div>
          </div>

          <div className="px-4 py-3 flex items-center justify-between gap-4">
            <div className="min-w-0">
              <p className="text-sm font-medium text-ink">Tự động lưu file</p>
              <p className="mt-0.5 text-xs text-ink-2 leading-snug">
                {preferences.auto_save
                  ? `Lưu audio.${preferences.file_format} khi dừng ghi. Transcript luôn được lưu.`
                  : 'Đang tắt. Transcript vẫn lưu vào thư mục trên, không lưu file âm thanh.'}
              </p>
            </div>
            <Switch
              checked={preferences.auto_save}
              onCheckedChange={handleAutoSaveToggle}
              disabled={saving}
            />
          </div>
        </div>
      </section>

      <section>
        <h2 className="text-sm font-semibold text-ink tracking-tight mb-2">Ghi âm</h2>
        <div className="app-surface overflow-hidden px-4 py-3 space-y-4">
          <div className="space-y-1.5">
            <Label className="text-sm font-medium text-ink">Nguồn</Label>
            <Select
              value={parseAudioCaptureSource(preferences.audio_source)}
              onValueChange={(value) => handleAudioSourceChange(value as AudioCaptureSource)}
              disabled={saving}
            >
              <SelectTrigger className="w-full h-9">
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
          </div>
          <div className="border-t border-rule pt-3">
            <DeviceSelection
              selectedDevices={{
                micDevice: preferences.preferred_mic_device,
                systemDevice: preferences.preferred_system_device,
              }}
              onDeviceChange={handleDeviceChange}
              disabled={saving}
              audioSource={parseAudioCaptureSource(preferences.audio_source)}
              compact
            />
          </div>
        </div>
      </section>
    </div>
  );
}
