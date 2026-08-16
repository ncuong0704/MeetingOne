'use client';

import { useEffect, useRef } from 'react';
import { toast } from 'sonner';
import { isTauriRuntime } from '@/lib/tauriRuntime';
import {
  checkForAppUpdate,
  getDismissedUpdateVersion,
  installPendingAppUpdate,
  openReleaseUrl,
  resolveAppVersion,
  setDismissedUpdateVersion,
} from '@/lib/appUpdate';

const AUTO_CHECK_DELAY_MS = 4000;
const UPDATE_TOAST_ID = 'app-update-available';

const UPDATE_TOAST_ACTION_CLASS =
  '!bg-primary !text-primary-foreground hover:!bg-primary-hover !border-primary';
const UPDATE_TOAST_ACTION_STYLE = {
  backgroundColor: 'var(--color-accent)',
  color: 'var(--color-accent-ink)',
  borderColor: 'var(--color-accent)',
} as const;

function notifyUpdateAvailable(
  versionLabel: string,
  currentVersion: string,
  onInstall: () => void,
  onDismiss: () => void,
  notes?: string
) {
  const description = notes
    ? notes.length > 160
      ? `${notes.slice(0, 160).trim()}…`
      : notes
    : `Bạn đang dùng ${currentVersion}.`;

  toast.info(`Có phiên bản mới ${versionLabel}`, {
    id: UPDATE_TOAST_ID,
    description,
    duration: 12000,
    classNames: {
      toast: 'app-update-toast',
      actionButton: UPDATE_TOAST_ACTION_CLASS,
    },
    actionButtonStyle: UPDATE_TOAST_ACTION_STYLE,
    action: {
      label: 'Cập nhật ngay',
      onClick: onInstall,
    },
    cancel: {
      label: 'Để sau',
      onClick: onDismiss,
    },
  });
}

function notifyGithubUpdate(
  latestTag: string,
  currentVersion: string,
  releaseUrl: string,
  onDismiss: () => void
) {
  toast.info(`Có bản mới trên GitHub: ${latestTag}`, {
    id: UPDATE_TOAST_ID,
    description: `Đang chạy ${currentVersion}. Tải thủ công nếu chưa thấy nút tự cập nhật.`,
    duration: 12000,
    classNames: {
      toast: 'app-update-toast',
      actionButton: UPDATE_TOAST_ACTION_CLASS,
    },
    actionButtonStyle: UPDATE_TOAST_ACTION_STYLE,
    action: {
      label: 'Mở trang tải',
      onClick: () => openReleaseUrl(releaseUrl),
    },
    cancel: {
      label: 'Để sau',
      onClick: onDismiss,
    },
  });
}

/** Hiển thị toast demo (dev / preview URL). */
export function previewAppUpdateNotification() {
  const mockVersion = '0.4.0';
  const currentVersion = '0.3.0';
  notifyUpdateAvailable(
    mockVersion,
    currentVersion,
    () => {
      toast.loading('Đang tải và cài đặt bản cập nhật…', {
        id: UPDATE_TOAST_ID,
        duration: Infinity,
      });
      window.setTimeout(() => {
        toast.success('Demo: cập nhật thành công (preview)', {
          id: UPDATE_TOAST_ID,
          description: 'Trên app thật, ứng dụng sẽ tự khởi động lại.',
        });
      }, 1500);
    },
    () => toast.dismiss(UPDATE_TOAST_ID),
    'Sửa lỗi ghi âm, cải thiện mẫu báo cáo và prompt AI mặc định.'
  );
}

function shouldPreviewAppUpdate(): boolean {
  if (typeof window === 'undefined') return false;
  return new URLSearchParams(window.location.search).get('previewAppUpdate') === '1';
}

export function AppUpdateNotifier() {
  const checkedRef = useRef(false);

  useEffect(() => {
    if (checkedRef.current) return;
    checkedRef.current = true;

    if (shouldPreviewAppUpdate()) {
      window.setTimeout(() => previewAppUpdateNotification(), 800);
      return;
    }

    const timer = window.setTimeout(async () => {
      try {
        const currentVersion = await resolveAppVersion();
        const result = await checkForAppUpdate(currentVersion);

        if (result.kind === 'none' || result.kind === 'error') {
          if (result.kind === 'error') {
            console.warn('[AppUpdateNotifier]', result.message);
          }
          return;
        }

        const remoteVersion =
          result.kind === 'updater' ? result.version : result.latestTag;

        if (getDismissedUpdateVersion() === remoteVersion) {
          return;
        }

        const dismiss = () => setDismissedUpdateVersion(remoteVersion);

        if (result.kind === 'updater') {
          notifyUpdateAvailable(
            result.version,
            currentVersion,
            async () => {
              toast.loading('Đang tải và cài đặt bản cập nhật…', {
                id: UPDATE_TOAST_ID,
                duration: Infinity,
              });
              try {
                const result = await installPendingAppUpdate((progress) => {
                  if (progress.total && progress.total > 0) {
                    const pct = Math.min(
                      100,
                      Math.round((progress.downloaded / progress.total) * 100)
                    );
                    toast.loading(`Đang cài đặt… ${pct}%`, {
                      id: UPDATE_TOAST_ID,
                      duration: Infinity,
                    });
                  }
                });
                if (result === 'installed') {
                  toast.success('Đang cài đặt bản mới', {
                    id: UPDATE_TOAST_ID,
                    description:
                      'Hoàn tất cửa sổ cài đặt (nếu có) rồi mở lại ứng dụng.',
                  });
                }
              } catch (e) {
                const message =
                  e instanceof Error ? e.message : 'Cài đặt cập nhật thất bại.';
                toast.error('Không cập nhật được', {
                  id: UPDATE_TOAST_ID,
                  description: message,
                });
              }
            },
            dismiss,
            result.notes
          );
          return;
        }

        if (result.kind === 'github') {
          notifyGithubUpdate(result.latestTag, currentVersion, result.releaseUrl, dismiss);
        }
      } catch (e) {
        console.warn('[AppUpdateNotifier] Auto check failed:', e);
      }
    }, AUTO_CHECK_DELAY_MS);

    return () => window.clearTimeout(timer);
  }, []);

  return null;
}

/** Chạy trên Tauri; browser dev chỉ khi `?previewAppUpdate=1`. */
export function AppUpdateNotifierGate() {
  if (!isTauriRuntime() && !shouldPreviewAppUpdate()) return null;
  return <AppUpdateNotifier />;
}
