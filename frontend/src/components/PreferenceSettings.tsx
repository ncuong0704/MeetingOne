"use client"

import { useEffect, useRef } from "react"
import { FolderOpen, HardDrive, Info } from "lucide-react"
import { invoke } from "@tauri-apps/api/core"
import Analytics from "@/lib/analytics"
import { useConfig } from "@/contexts/ConfigContext"

function SettingCard({ children }: { children: React.ReactNode }) {
  return (
    <div className="app-surface overflow-hidden">
      {children}
    </div>
  );
}

function SettingCardHeader({ title, description }: { title: string; description?: string }) {
  return (
    <div className="px-5 py-4 border-b border-rule">
      <h3 className="text-base font-semibold text-ink">{title}</h3>
      {description && <p className="text-sm text-ink-2 mt-1 leading-relaxed">{description}</p>}
    </div>
  );
}

export function PreferenceSettings() {
  const { storageLocations, isLoadingPreferences, loadPreferences } = useConfig();
  const hasTrackedViewRef = useRef(false);

  useEffect(() => {
    loadPreferences();
    hasTrackedViewRef.current = false;
  }, [loadPreferences]);

  useEffect(() => {
    if (hasTrackedViewRef.current) return;
    const track = async () => {
      if (!isLoadingPreferences) {
        await Analytics.track('preferences_viewed', {});
        hasTrackedViewRef.current = true;
      }
    };
    track();
  }, [isLoadingPreferences]);

  const handleOpenFolder = async (folderType: 'database' | 'models' | 'recordings') => {
    try {
      switch (folderType) {
        case 'database':   await invoke('open_database_folder');   break;
        case 'models':     await invoke('open_models_folder');     break;
        case 'recordings': await invoke('open_recordings_folder'); break;
      }
      await Analytics.track('storage_folder_opened', { folder_type: folderType });
    } catch (error) {
      console.error(`Failed to open ${folderType} folder:`, error);
    }
  };

  if (isLoadingPreferences && !storageLocations) {
    return (
      <div className="space-y-3 animate-pulse">
        <div className="h-32 bg-gray-100 rounded-xl" />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <SettingCard>
        <SettingCardHeader
          title="Vị trí lưu trữ dữ liệu"
          description="Xem và truy cập nơi ACT MeetingOne lưu dữ liệu của bạn."
        />
        <div className="p-5 space-y-3">
          <div className="flex items-start gap-3 p-4 rounded-md bg-secondary border border-rule">
            <div className="w-9 h-9 rounded-md bg-paper-2 border border-rule flex items-center justify-center shrink-0">
              <HardDrive className="w-4.5 h-4.5 text-muted-foreground" />
            </div>
            <div className="flex-1 min-w-0">
              <p className="text-base font-medium text-ink mb-1">Bản ghi cuộc họp</p>
              <p className="text-sm text-ink-2 font-mono break-all leading-relaxed">
                {storageLocations?.recordings || 'Đang tải...'}
              </p>
            </div>
            <button
              onClick={() => handleOpenFolder('recordings')}
              className="shrink-0 flex items-center gap-2 px-3.5 py-2 text-sm font-medium text-muted-foreground bg-paper-2 border border-input rounded-md hover:bg-secondary transition-colors"
            >
              <FolderOpen className="w-4 h-4" />
              Mở
            </button>
          </div>

          <div className="flex items-start gap-3 bg-primary/10 border border-primary/20 rounded-md px-4 py-3">
            <Info className="w-4 h-4 text-primary shrink-0 mt-0.5" />
            <p className="text-sm text-primary leading-relaxed">
              Cơ sở dữ liệu và mô hình AI được lưu cùng nhau trong thư mục dữ liệu ứng dụng.
            </p>
          </div>
        </div>
      </SettingCard>
    </div>
  );
}
