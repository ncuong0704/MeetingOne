'use client';

import React, { useCallback, useEffect, useRef, useState } from 'react';
import Image from 'next/image';
import { invoke } from '@tauri-apps/api/core';
import type { Update } from '@tauri-apps/plugin-updater';
import { BRAND_NAME, BRAND_LOGO_PATH } from '@/constants/brand';
import {
  fetchLatestGitHubRelease,
  getGithubReleaseRepo,
  getGithubUpdaterLatestJsonUrl,
  isRemoteVersionNewer,
} from '@/lib/githubRelease';
import { Lock, Cpu, Banknote, Globe, Shield, RefreshCw } from 'lucide-react';
import pkg from '../../package.json';

const features = [
  {
    icon: Lock,
    title: 'Ưu tiên quyền riêng tư',
    desc: 'Dữ liệu và quy trình xử lý AI giữ trong phạm vi của bạn — không phụ thuộc đám mây.',
    color: 'text-blue-600',
    bg: 'bg-blue-50',
  },
  {
    icon: Cpu,
    title: 'Linh hoạt mô hình',
    desc: 'Dùng mô hình mã nguồn mở cục bộ hay API bên ngoài đều được — không bị khóa nhà cung cấp.',
    color: 'text-violet-600',
    bg: 'bg-violet-50',
  },
  {
    icon: Banknote,
    title: 'Tiết kiệm chi phí',
    desc: 'Giảm chi phí bằng cách chạy mô hình cục bộ hoặc chỉ trả cho các lần gọi bạn chọn.',
    color: 'text-emerald-600',
    bg: 'bg-emerald-50',
  },
  {
    icon: Globe,
    title: 'Làm việc mọi nơi',
    desc: 'Google Meet, Zoom, Teams — trực tuyến hay ngoại tuyến đều hoạt động.',
    color: 'text-orange-600',
    bg: 'bg-orange-50',
  },
];

type CheckState =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'updater_available'; version: string; notes?: string }
  | { status: 'updater_downloading'; downloaded: number; total?: number }
  | { status: 'uptodate'; latestTag: string }
  | { status: 'available'; latestTag: string; releaseUrl: string }
  | { status: 'error'; message: string };

async function resolveAppVersion(): Promise<string> {
  try {
    const { getVersion } = await import('@tauri-apps/api/app');
    return await getVersion();
  } catch {
    return pkg.version;
  }
}

export function About() {
  const [appVersion, setAppVersion] = useState<string>(pkg.version);
  const [check, setCheck] = useState<CheckState>({ status: 'idle' });
  const pendingUpdateRef = useRef<Update | null>(null);

  useEffect(() => {
    let cancelled = false;
    resolveAppVersion().then((v) => {
      if (!cancelled) setAppVersion(v);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const runGithubFallback = useCallback(async () => {
    const repo = getGithubReleaseRepo();
    const release = await fetchLatestGitHubRelease(repo);
    if (isRemoteVersionNewer(release.tag_name, appVersion)) {
      setCheck({
        status: 'available',
        latestTag: release.tag_name,
        releaseUrl: release.html_url,
      });
    } else {
      setCheck({ status: 'uptodate', latestTag: release.tag_name });
    }
  }, [appVersion]);

  const handleCheck = useCallback(async () => {
    setCheck({ status: 'loading' });
    pendingUpdateRef.current = null;
    try {
      const { check } = await import('@tauri-apps/plugin-updater');
      const update = await check();
      if (update) {
        pendingUpdateRef.current = update;
        setCheck({
          status: 'updater_available',
          version: update.version,
          notes: update.body,
        });
        return;
      }
    } catch (e) {
      // Dev (không phải Tauri), hoặc endpoint / chữ ký chưa cấu hình — thử GitHub API.
      console.error('[About] Tauri updater check() thất bại:', e);
    }

    try {
      await runGithubFallback();
    } catch (e) {
      const message = e instanceof Error ? e.message : 'Không kiểm tra được cập nhật.';
      console.error('[About] Kiểm tra cập nhật (GitHub fallback) thất bại:', e);
      setCheck({ status: 'error', message });
    }
  }, [runGithubFallback]);

  const handleDownloadAndInstall = useCallback(async () => {
    const update = pendingUpdateRef.current;
    if (!update) {
      console.warn('[About] Tải/cài đặt: không có bản cập nhật đang chờ (pendingUpdateRef rỗng).');
      return;
    }

    setCheck({ status: 'updater_downloading', downloaded: 0, total: undefined });
    try {
      let downloaded = 0;
      let total: number | undefined;
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          total = event.data.contentLength;
          setCheck({ status: 'updater_downloading', downloaded: 0, total });
        } else if (event.event === 'Progress') {
          downloaded += event.data.chunkLength;
          setCheck((prev) =>
            prev.status === 'updater_downloading'
              ? { status: 'updater_downloading', downloaded, total: prev.total ?? total }
              : prev
          );
        }
      });
      pendingUpdateRef.current = null;
      const { relaunch } = await import('@tauri-apps/plugin-process');
      await relaunch();
    } catch (e) {
      const message = e instanceof Error ? e.message : 'Cài đặt cập nhật thất bại.';
      console.error('[About] downloadAndInstall / relaunch thất bại:', e);
      setCheck({ status: 'error', message });
    }
  }, []);

  const openRelease = useCallback(async (url: string) => {
    try {
      await invoke('open_external_url', { url });
    } catch {
      window.open(url, '_blank', 'noopener,noreferrer');
    }
  }, []);

  const downloadPct =
    check.status === 'updater_downloading' && check.total && check.total > 0
      ? Math.min(100, Math.round((check.downloaded / check.total) * 100))
      : null;

  return (
    <div className="flex flex-col h-[80vh] overflow-y-auto bg-gray-50">

      {/* ── Hero ──────────────────────────────────────────────────────── */}
      <div className="bg-white border-b border-gray-100 px-6 pb-8 text-center space-y-4">
        <Image
          src={BRAND_LOGO_PATH}
          alt={BRAND_NAME}
          width={100}
          height={100}
          className="mx-auto object-contain"
        />

        <div className="space-y-1">
          <h1 className="text-2xl font-bold text-gray-900">{BRAND_NAME}</h1>
          <p className="text-sm text-gray-500 max-w-sm mx-auto leading-relaxed">
          AI thư ký cuộc họp — tự động tóm tắt và phân tích nội dung sau khi cuộc họp kết thúc.
          </p>
        </div>

        {/* Badges */}
        <div className="flex items-center justify-center gap-2 flex-wrap">
          <span className="inline-flex items-center gap-1.5 rounded-full bg-blue-50 px-3 py-1 text-xs font-medium text-blue-600">
            <Shield className="w-3 h-3" />
            Ưu tiên quyền riêng tư
          </span>
          <span className="inline-flex items-center gap-1.5 rounded-full bg-emerald-50 px-3 py-1 text-xs font-medium text-emerald-600">
            <Cpu className="w-3 h-3" />
            Sẵn sàng ngoại tuyến
          </span>
        </div>
      </div>

      {/* ── Phiên bản / cập nhật ─────────────────────────────────────── */}
      <div className="px-5 pt-5">
        <div className="rounded-xl border border-gray-100 bg-white p-4 shadow-sm space-y-3">
          <div className="flex items-center justify-between gap-2">
            <div>
              <p className="text-[11px] font-semibold uppercase tracking-widest text-gray-400">
                Phiên bản
              </p>
              <p className="text-sm font-medium text-gray-900 tabular-nums">{appVersion}</p>
            </div>
          </div>
          <button
            type="button"
            onClick={handleCheck}
            disabled={check.status === 'loading' || check.status === 'updater_downloading'}
            className="w-full inline-flex items-center justify-center gap-2 rounded-lg bg-gray-900 px-3 py-2 text-xs font-medium text-white hover:bg-gray-800 disabled:opacity-60 disabled:pointer-events-none"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${check.status === 'loading' ? 'animate-spin' : ''}`} />
            Kiểm tra cập nhật
          </button>

          {check.status === 'updater_available' && (
            <div className="space-y-2">
              <p className="text-xs text-amber-800">
                Có bản mới <span className="font-semibold">{check.version}</span> (đang chạy {appVersion}).
              </p>
              {check.notes ? (
                <p className="text-[11px] text-gray-600 leading-relaxed whitespace-pre-wrap max-h-24 overflow-y-auto">
                  {check.notes}
                </p>
              ) : null}
              <button
                type="button"
                onClick={handleDownloadAndInstall}
                className="w-full rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-xs font-medium text-amber-900 hover:bg-amber-100"
              >
                Tải và cài đặt (khởi động lại)
              </button>
            </div>
          )}

          {check.status === 'updater_downloading' && (
            <p className="text-xs text-gray-700">
              Đang tải và cài đặt…
              {downloadPct !== null ? ` ${downloadPct}%` : ''}
            </p>
          )}

          {check.status === 'uptodate' && (
            <p className="text-xs text-emerald-700">
              Bạn đang dùng phiên bản mới nhất (version: {check.latestTag}).
            </p>
          )}
          {check.status === 'available' && (
            <div className="space-y-2">
              <p className="text-xs text-amber-800">
                Trên GitHub có tag mới hơn: <span className="font-semibold">{check.latestTag}</span> (đang chạy{' '}
                {appVersion}). Nếu không thấy nút tự cập nhật, hãy tải thủ công.
              </p>
              <button
                type="button"
                onClick={() => openRelease(check.releaseUrl)}
                className="w-full rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-xs font-medium text-amber-900 hover:bg-amber-100"
              >
                Mở trang tải bản phát hành
              </button>
            </div>
          )}
          {check.status === 'error' && (
            <p className="text-xs text-red-600 leading-relaxed">{check.message}</p>
          )}

          {/* <p className="text-[10px] text-gray-400 leading-relaxed border-t border-gray-100 pt-2">
            Cập nhật tự động đọc <code className="text-gray-500">latest.json</code> tại{' '}
            <span className="break-all">{getGithubUpdaterLatestJsonUrl()}</span>. Mỗi release cần ký bằng{' '}
            <code className="text-gray-500">TAURI_SIGNING_PRIVATE_KEY</code> (hoặc{' '}
            <code className="text-gray-500">TAURI_SIGNING_PRIVATE_KEY_PATH</code>) khi build; fork repo khác cần sửa
            endpoint trong <code className="text-gray-500">tauri.conf.json</code>.
          </p> */}
        </div>
      </div>

      {/* ── Features ──────────────────────────────────────────────────── */}
      <div className="px-5 py-5 space-y-2.5">
        <p className="text-[11px] font-semibold uppercase tracking-widest text-gray-400 px-1">Tính năng nổi bật</p>
        <div className="grid grid-cols-2 gap-2.5">
          {features.map(({ icon: Icon, title, desc, color, bg }) => (
            <div
              key={title}
              className="bg-white rounded-xl border border-gray-100 p-3.5 shadow-sm hover:shadow-md hover:border-gray-200 transition-all"
            >
              <div className={`w-7 h-7 rounded-lg ${bg} flex items-center justify-center mb-2.5`}>
                <Icon className={`w-3.5 h-3.5 ${color}`} />
              </div>
              <h3 className="text-xs font-semibold text-gray-800 mb-1 leading-snug">{title}</h3>
              <p className="text-[11px] text-gray-500 leading-relaxed">{desc}</p>
            </div>
          ))}
        </div>
      </div>

    </div>
  );
}
