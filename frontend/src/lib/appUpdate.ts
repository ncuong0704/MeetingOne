import { invoke } from '@tauri-apps/api/core';
import type { Update } from '@tauri-apps/plugin-updater';
import {
  fetchLatestGitHubRelease,
  getGithubReleaseRepo,
  isRemoteVersionNewer,
} from '@/lib/githubRelease';
import pkg from '../../package.json';

export type AppUpdateResult =
  | { kind: 'none'; latestTag?: string }
  | { kind: 'updater'; update: Update; version: string; notes?: string }
  | { kind: 'github'; latestTag: string; releaseUrl: string }
  | { kind: 'error'; message: string };

let pendingUpdate: Update | null = null;

export function getPendingAppUpdate(): Update | null {
  return pendingUpdate;
}

export function clearPendingAppUpdate(): void {
  pendingUpdate = null;
}

export async function resolveAppVersion(): Promise<string> {
  try {
    const { getVersion } = await import('@tauri-apps/api/app');
    return await getVersion();
  } catch {
    return pkg.version;
  }
}

export async function checkForAppUpdate(currentVersion: string): Promise<AppUpdateResult> {
  pendingUpdate = null;

  try {
    const { check } = await import('@tauri-apps/plugin-updater');
    const update = await check();
    if (update) {
      pendingUpdate = update;
      return {
        kind: 'updater',
        update,
        version: update.version,
        notes: update.body,
      };
    }
  } catch (e) {
    console.warn('[appUpdate] Tauri updater check() failed, trying GitHub:', e);
  }

  try {
    const repo = getGithubReleaseRepo();
    const release = await fetchLatestGitHubRelease(repo);
    if (isRemoteVersionNewer(release.tag_name, currentVersion)) {
      return {
        kind: 'github',
        latestTag: release.tag_name,
        releaseUrl: release.html_url,
      };
    }
    return { kind: 'none', latestTag: release.tag_name };
  } catch (e) {
    const message = e instanceof Error ? e.message : 'Không kiểm tra được cập nhật.';
    return { kind: 'error', message };
  }
}

export type InstallProgress = {
  downloaded: number;
  total?: number;
};

function formatUpdaterError(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error && error.message.trim()) return error.message;
  try {
    return JSON.stringify(error);
  } catch {
    return 'Cài đặt cập nhật thất bại.';
  }
}

/** Windows updater gọi process::exit sau khi chạy NSIS — IPC có thể ngắt dù cài đặt đã khởi chạy. */
function isLikelyUpdaterInstallExit(error: unknown, downloadFinished: boolean): boolean {
  if (downloadFinished) return true;
  const message = formatUpdaterError(error).toLowerCase();
  return (
    message.includes('callback') ||
    message.includes('ipc') ||
    message.includes('disconnected') ||
    message.includes('closed') ||
    message.includes('aborted')
  );
}

export async function installPendingAppUpdate(
  onProgress?: (progress: InstallProgress) => void
): Promise<'installed' | 'relaunching'> {
  const update = pendingUpdate;
  if (!update) {
    throw new Error('Không có bản cập nhật đang chờ.');
  }

  let downloaded = 0;
  let total: number | undefined;
  let downloadFinished = false;

  try {
    await update.downloadAndInstall((event) => {
      if (event.event === 'Started') {
        total = event.data.contentLength;
        onProgress?.({ downloaded: 0, total });
      } else if (event.event === 'Progress') {
        downloaded += event.data.chunkLength;
        onProgress?.({ downloaded, total });
      } else if (event.event === 'Finished') {
        downloadFinished = true;
      }
    });
  } catch (error) {
    if (!isLikelyUpdaterInstallExit(error, downloadFinished)) {
      throw new Error(formatUpdaterError(error));
    }
    pendingUpdate = null;
    return 'installed';
  }

  pendingUpdate = null;

  const { platform } = await import('@tauri-apps/plugin-os');
  if (platform() === 'windows') {
    return 'installed';
  }

  const { relaunch } = await import('@tauri-apps/plugin-process');
  await relaunch();
  return 'relaunching';
}

export async function openReleaseUrl(url: string): Promise<void> {
  try {
    await invoke('open_external_url', { url });
  } catch {
    window.open(url, '_blank', 'noopener,noreferrer');
  }
}

const DISMISSED_VERSION_KEY = 'app_update_dismissed_version';

export function getDismissedUpdateVersion(): string | null {
  if (typeof window === 'undefined') return null;
  return localStorage.getItem(DISMISSED_VERSION_KEY);
}

export function setDismissedUpdateVersion(version: string): void {
  localStorage.setItem(DISMISSED_VERSION_KEY, version);
}
