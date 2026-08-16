'use client';

import React, { useCallback, useEffect, useState } from 'react';
import { BRAND_NAME } from '@/constants/brand';
import {
  checkForAppUpdate,
  clearPendingAppUpdate,
  getPendingAppUpdate,
  installPendingAppUpdate,
  openReleaseUrl,
  resolveAppVersion,
} from '@/lib/appUpdate';
import { Lock, Cpu, Banknote, Globe, RefreshCw } from 'lucide-react';

const features = [
  {
    icon: Lock,
    title: 'Ưu tiên quyền riêng tư',
    desc: 'Dữ liệu và xử lý AI giữ trong phạm vi của bạn — không phụ thuộc đám mây.',
    color: 'text-primary',
  },
  {
    icon: Cpu,
    title: 'Linh hoạt mô hình',
    desc: 'Mô hình mã nguồn mở cục bộ hay API bên ngoài — không khóa nhà cung cấp.',
    color: 'text-sky-700',
  },
  {
    icon: Banknote,
    title: 'Tiết kiệm chi phí',
    desc: 'Chạy mô hình cục bộ, hoặc chỉ trả cho các lần gọi bạn chọn.',
    color: 'text-emerald-600',
  },
  {
    icon: Globe,
    title: 'Làm việc mọi nơi',
    desc: 'Google Meet, Zoom, Teams — trực tuyến hay ngoại tuyến đều dùng được.',
    color: 'text-amber-600',
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

export function About() {
  const [appVersion, setAppVersion] = useState<string>('');
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
    clearPendingAppUpdate();

    const version = appVersion || await resolveAppVersion();
    const result = await checkForAppUpdate(version);

    if (result.kind === 'updater') {
      setCheck({
        status: 'updater_available',
        version: result.version,
        notes: result.notes,
      });
      return;
    }

    if (result.kind === 'github') {
      setCheck({
        status: 'available',
        latestTag: result.latestTag,
        releaseUrl: result.releaseUrl,
      });
      return;
    }

    if (result.kind === 'none') {
      setCheck({ status: 'uptodate', latestTag: result.latestTag ?? version });
      return;
    }

    setCheck({ status: 'error', message: result.message });
  }, [appVersion]);

  const handleDownloadAndInstall = useCallback(async () => {
    if (!getPendingAppUpdate()) {
      console.warn('[About] Tải/cài đặt: không có bản cập nhật đang chờ.');
      return;
    }

    setCheck({ status: 'updater_downloading', downloaded: 0, total: undefined });
    try {
      const result = await installPendingAppUpdate((progress) => {
        setCheck((prev) =>
          prev.status === 'updater_downloading'
            ? {
                status: 'updater_downloading',
                downloaded: progress.downloaded,
                total: progress.total ?? prev.total,
              }
            : prev
        );
      });
      if (result === 'installed') {
        setCheck({
          status: 'error',
          message:
            'Trình cài đặt đang chạy. Vui lòng hoàn tất cửa sổ cài đặt (nếu có) rồi mở lại ứng dụng.',
        });
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : 'Cài đặt cập nhật thất bại.';
      console.error('[About] downloadAndInstall thất bại:', e);
      setCheck({ status: 'error', message });
    }
  }, []);

  const downloadPct =
    check.status === 'updater_downloading' && check.total && check.total > 0
      ? Math.min(100, Math.round((check.downloaded / check.total) * 100))
      : null;

  const busy = check.status === 'loading' || check.status === 'updater_downloading';

  return (
    <div className="flex flex-col max-h-[min(34rem,calc(100vh-6rem))] text-left">
      <header className="shrink-0 px-6 pt-7 pb-5 pr-12 border-b border-rule text-center">
        <p className="font-mono text-[10px] uppercase tracking-[0.12em] text-ink-2 mb-3">
          Thư ký cuộc họp
        </p>
        <h1 className="text-2xl font-semibold tracking-tight text-primary">{BRAND_NAME}</h1>
        <p className="text-sm text-ink-2 mt-2 max-w-[40ch] mx-auto leading-relaxed">
          Tóm tắt và phân tích nội dung sau cuộc họp, chạy trên máy của bạn.
        </p>
      </header>

      <section className="shrink-0 px-6 py-4 border-b border-rule space-y-3">
        <div className="flex items-center justify-between gap-3">
          <div>
            <p className="font-mono text-[10px] uppercase tracking-[0.12em] text-ink-2">
              Phiên bản
            </p>
            <p className="text-sm font-medium text-amber-600 font-mono tabular-nums mt-0.5">
              {appVersion || '…'}
            </p>
          </div>
          <button
            type="button"
            onClick={handleCheck}
            disabled={busy}
            className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-primary-foreground hover:bg-primary-hover disabled:opacity-60 disabled:pointer-events-none"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${check.status === 'loading' ? 'animate-spin' : ''}`} />
            Kiểm tra cập nhật
          </button>
        </div>

        {check.status === 'updater_available' && (
          <div className="space-y-2 rounded-md border border-rule bg-paper-3 p-3">
            <p className="text-sm text-ink">
              Có bản mới <span className="font-medium">{check.version}</span>
              {appVersion ? ` (đang chạy ${appVersion})` : ''}.
            </p>
            {check.notes ? (
              <p className="text-xs text-ink-2 leading-relaxed whitespace-pre-wrap max-h-24 overflow-y-auto">
                {check.notes}
              </p>
            ) : null}
            <button
              type="button"
              onClick={handleDownloadAndInstall}
              className="w-full rounded-md bg-primary px-3 py-2 text-sm font-medium text-primary-foreground hover:bg-primary-hover"
            >
              Tải và cài đặt (khởi động lại)
            </button>
          </div>
        )}

        {check.status === 'updater_downloading' && (
          <p className="text-sm text-ink-2">
            Đang tải và cài đặt…
            {downloadPct !== null ? ` ${downloadPct}%` : ''}
          </p>
        )}

        {check.status === 'uptodate' && (
          <p className="text-sm text-ink-2">
            Đang dùng phiên bản mới nhất ({check.latestTag}).
          </p>
        )}

        {check.status === 'available' && (
          <div className="space-y-2 rounded-md border border-rule bg-paper-3 p-3">
            <p className="text-sm text-ink">
              Trên GitHub có tag mới hơn: <span className="font-medium">{check.latestTag}</span>
              {appVersion ? ` (đang chạy ${appVersion})` : ''}. Nếu không thấy nút tự cập nhật, hãy tải thủ công.
            </p>
            <button
              type="button"
              onClick={() => openReleaseUrl(check.releaseUrl)}
              className="w-full rounded-md border border-primary px-3 py-2 text-sm font-medium text-primary hover:bg-secondary"
            >
              Mở trang tải bản phát hành
            </button>
          </div>
        )}

        {check.status === 'error' && (
          <p className="text-sm text-destructive leading-relaxed">{check.message}</p>
        )}
      </section>

      <section className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
        <p className="font-mono text-[10px] uppercase tracking-[0.12em] text-ink-2 mb-1">
          Đặc điểm
        </p>
        <ul>
          {features.map(({ icon: Icon, title, desc, color }, index) => (
            <li
              key={title}
              className={`flex gap-3 py-3 ${index < features.length - 1 ? 'border-b border-rule' : ''}`}
            >
              <Icon className={`w-4 h-4 mt-0.5 shrink-0 ${color}`} aria-hidden />
              <div className="min-w-0">
                <h3 className="text-sm font-medium text-ink">{title}</h3>
                <p className="text-xs text-ink-2 mt-0.5 leading-relaxed">{desc}</p>
              </div>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
