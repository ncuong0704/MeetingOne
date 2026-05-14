'use client';

import React, { useCallback, useEffect, useState } from 'react';
import Image from 'next/image';
import { invoke } from '@tauri-apps/api/core';
import { BRAND_NAME, BRAND_LOGO_PATH } from '@/constants/brand';
import {
  fetchLatestGitHubRelease,
  getGithubReleaseRepo,
  isRemoteVersionNewer,
} from '@/lib/githubRelease';
import { Lock, Cpu, Banknote, Globe, Github, Shield, RefreshCw } from 'lucide-react';
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

  useEffect(() => {
    let cancelled = false;
    resolveAppVersion().then((v) => {
      if (!cancelled) setAppVersion(v);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const handleCheck = useCallback(async () => {
    setCheck({ status: 'loading' });
    const repo = getGithubReleaseRepo();
    try {
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
    } catch (e) {
      const message = e instanceof Error ? e.message : 'Không kiểm tra được cập nhật.';
      setCheck({ status: 'error', message });
    }
  }, [appVersion]);

  const openRelease = useCallback(async (url: string) => {
    try {
      await invoke('open_external_url', { url });
    } catch {
      window.open(url, '_blank', 'noopener,noreferrer');
    }
  }, []);

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
            Ghi chú và tóm tắt thời gian thực — dữ liệu không rời khỏi máy của bạn.
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

      {/* ── Phiên bản / GitHub ───────────────────────────────────────── */}
      <div className="px-5 pt-5">
        <div className="rounded-xl border border-gray-100 bg-white p-4 shadow-sm space-y-3">
          <div className="flex items-center justify-between gap-2">
            <div>
              <p className="text-[11px] font-semibold uppercase tracking-widest text-gray-400">
                Phiên bản
              </p>
              <p className="text-sm font-medium text-gray-900 tabular-nums">{appVersion}</p>
            </div>
            <a
              href={`https://github.com/${getGithubReleaseRepo()}`}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1.5 rounded-lg border border-gray-200 bg-gray-50 px-2.5 py-1.5 text-xs font-medium text-gray-700 hover:bg-gray-100"
            >
              <Github className="w-3.5 h-3.5" />
              GitHub
            </a>
          </div>
          <button
            type="button"
            onClick={handleCheck}
            disabled={check.status === 'loading'}
            className="w-full inline-flex items-center justify-center gap-2 rounded-lg bg-gray-900 px-3 py-2 text-xs font-medium text-white hover:bg-gray-800 disabled:opacity-60 disabled:pointer-events-none"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${check.status === 'loading' ? 'animate-spin' : ''}`} />
            Kiểm tra cập nhật trên GitHub
          </button>
          {check.status === 'uptodate' && (
            <p className="text-xs text-emerald-700">
              Bạn đang dùng phiên bản mới nhất (release mới nhất: {check.latestTag}).
            </p>
          )}
          {check.status === 'available' && (
            <div className="space-y-2">
              <p className="text-xs text-amber-800">
                Có bản mới: <span className="font-semibold">{check.latestTag}</span> (đang chạy {appVersion}).
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
