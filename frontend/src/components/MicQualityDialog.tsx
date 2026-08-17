'use client';

import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { cn } from '@/lib/utils';
import {
  AnalysisResult,
  MicQualityProgress,
  confidenceColorClass,
  confidenceLabel,
  dnsmosColorClass,
  dnsmosLabel,
  micQualityAnalyze,
  micQualityCancel,
  micQualityDownloadModel,
  micQualityIsModelReady,
  stripSuggestionEmoji,
} from '@/lib/micQuality';

interface MicQualityDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  deviceName: string | null;
}

type Phase = 'idle' | 'need_download' | 'downloading' | 'recording' | 'analyzing' | 'transcribing' | 'result' | 'error';

export function MicQualityDialog({ open, onOpenChange, deviceName }: MicQualityDialogProps) {
  const [phase, setPhase] = useState<Phase>('idle');
  const [percent, setPercent] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<AnalysisResult | null>(null);

  useEffect(() => {
    if (!open) {
      void micQualityCancel();
      setPhase('idle');
      setPercent(0);
      setError(null);
      setResult(null);
      return;
    }
    micQualityIsModelReady()
      .then((ready) => setPhase(ready ? 'idle' : 'need_download'))
      .catch((e) => {
        setError(String(e));
        setPhase('error');
      });
  }, [open]);

  useEffect(() => {
    if (!open) return;
    let unlistenProgress: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;
    let unlistenErr: (() => void) | undefined;
    const setup = async () => {
      unlistenProgress = await listen<MicQualityProgress>('mic-quality-progress', (event) => {
        setPercent(event.payload.percent);
        if (event.payload.phase === 'download') setPhase('downloading');
        if (event.payload.phase === 'recording') setPhase('recording');
        if (event.payload.phase === 'analyzing') setPhase('analyzing');
        if (event.payload.phase === 'transcribing') setPhase('transcribing');
      });
      unlistenDone = await listen('mic-quality-download-complete', () => {
        setPhase('idle');
        setPercent(0);
      });
      unlistenErr = await listen<{ error: string }>('mic-quality-download-error', (event) => {
        setError(event.payload.error);
        setPhase('error');
      });
    };
    setup().catch(console.error);
    return () => {
      unlistenProgress?.();
      unlistenDone?.();
      unlistenErr?.();
    };
  }, [open]);

  const handleDownload = async () => {
    setPhase('downloading');
    setPercent(0);
    setError(null);
    try {
      await micQualityDownloadModel();
    } catch (e) {
      setError(String(e));
      setPhase('error');
    }
  };

  const handleStart = async () => {
    setPhase('recording');
    setPercent(0);
    setError(null);
    setResult(null);
    try {
      const analysis = await micQualityAnalyze(deviceName);
      if (analysis.error_message) {
        setError(analysis.error_message);
        setPhase('error');
        return;
      }
      setResult(analysis);
      setPhase('result');
    } catch (e) {
      const msg = String(e);
      if (msg.includes('Đã hủy')) {
        onOpenChange(false);
        return;
      }
      setError(msg);
      setPhase('error');
    }
  };

  const handleClose = () => {
    void micQualityCancel();
    onOpenChange(false);
  };

  const busy =
    phase === 'recording' ||
    phase === 'analyzing' ||
    phase === 'transcribing' ||
    phase === 'downloading';
  const statusText =
    phase === 'downloading'
      ? `Đang tải model DNSMOS... ${percent}%`
      : phase === 'recording'
        ? `Đang ghi âm... ${percent}%`
        : phase === 'analyzing'
          ? 'Đang phân tích...'
          : phase === 'transcribing'
            ? 'Đang nhận dạng...'
            : phase === 'need_download'
              ? 'Cần tải model DNSMOS (~5MB)'
              : 'Sẵn sàng';

  return (
    <Dialog open={open} onOpenChange={(next) => { if (!next) handleClose(); }}>
      <DialogContent className="gap-0 overflow-hidden p-0 sm:max-w-md">
        <DialogHeader className="space-y-0 px-5 pb-4 pt-5 pr-12 text-left">
          <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-ink-2">
            Kiểm tra micro
          </p>
          <DialogTitle className="mt-1 text-base font-semibold tracking-[-0.02em] text-ink">
            {phase === 'result' ? 'Kết quả đánh giá chất lượng âm thanh' : 'Đánh giá Microphone'}
          </DialogTitle>
          <DialogDescription className="mt-1.5 text-xs text-ink-2">
            {phase === 'result'
              ? result?.is_ready
                ? 'Sẵn sàng cho nhận dạng'
                : 'Cần cải thiện chất lượng'
              : 'Đứng tại vị trí phát biểu và nói nội dung bất kỳ (ví dụ: «Một hai ba bốn năm»). Thời gian ghi âm: 8–10 giây.'}
          </DialogDescription>
          {deviceName && (
            <p className="mt-2 font-mono text-[11px] text-ink-2 truncate">{deviceName}</p>
          )}
        </DialogHeader>

        <div className="px-5 pb-4">
          {phase !== 'result' && (
            <div className="space-y-3">
              {(busy || phase === 'need_download') && (
                <div className="h-1.5 w-full overflow-hidden rounded-full border border-rule bg-paper">
                  <div
                    className="h-full bg-primary transition-all"
                    style={{ width: `${phase === 'need_download' ? 0 : percent}%` }}
                  />
                </div>
              )}
              <p className="text-center font-mono text-xs text-ink-2">{statusText}</p>
              {error && <p className="text-xs text-destructive">{error}</p>}
            </div>
          )}

          {phase === 'result' && result && (
            <ResultBody result={result} />
          )}
        </div>

        <DialogFooter className="border-t border-rule bg-paper px-5 py-3">
          {phase === 'need_download' && (
            <Button size="sm" onClick={handleDownload}>Tải model DNSMOS</Button>
          )}
          {(phase === 'idle' || phase === 'error') && (
            <Button size="sm" onClick={handleStart}>Bắt đầu ghi âm</Button>
          )}
          {phase === 'result' && (
            <Button variant="outline" size="sm" onClick={handleStart}>
              Thử lại
            </Button>
          )}
          <Button variant="outline" size="sm" onClick={handleClose} disabled={phase === 'analyzing'}>
            {busy ? 'Hủy' : 'Đóng'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ResultBody({ result }: { result: AnalysisResult }) {
  const m = result.metrics;
  return (
    <div className="max-h-[50vh] space-y-3 overflow-y-auto">
      <div
        className={cn(
          'flex items-center justify-between rounded-md border px-3 py-2',
          result.is_ready ? 'border-rule bg-paper' : 'border-destructive/30 bg-paper'
        )}
      >
        <p className={cn('text-sm font-medium', result.is_ready ? 'text-ink' : 'text-destructive')}>
          {result.is_ready ? 'Sẵn sàng cho nhận dạng' : 'Cần cải thiện chất lượng'}
        </p>
        <span
          className={cn(
            'inline-flex h-6 items-center rounded-md px-2 font-mono text-[10px] uppercase tracking-[0.12em]',
            result.is_ready
              ? 'bg-primary text-primary-foreground'
              : 'border border-rule text-ink-2'
          )}
        >
          {result.is_ready ? 'Đạt' : 'Chưa đạt'}
        </span>
      </div>

      {m.duration_analyzed > 0 && (
        <p className="font-mono text-[11px] text-ink-2">
          {m.duration_analyzed.toFixed(1)}s · {m.num_segments} đoạn
        </p>
      )}

      <div>
        <p className="mb-1.5 font-mono text-[10px] uppercase tracking-[0.14em] text-ink-2">
          Chất lượng âm thanh (DNSMOS)
        </p>
        <div className="overflow-hidden rounded-md border border-rule">
          <MetricRow
            label="SIG · Giọng nói"
            value={`${m.dnsmos_sig.toFixed(2)}/5`}
            status={dnsmosLabel(m.dnsmos_sig)}
            colorClass={dnsmosColorClass(m.dnsmos_sig)}
          />
          <MetricRow
            label="BAK · Nhiễu, vang"
            value={`${m.dnsmos_bak.toFixed(2)}/5`}
            status={dnsmosLabel(m.dnsmos_bak)}
            colorClass={dnsmosColorClass(m.dnsmos_bak)}
          />
          <MetricRow
            label="OVRL · Tổng thể"
            value={`${m.dnsmos_ovrl.toFixed(2)}/5`}
            status={dnsmosLabel(m.dnsmos_ovrl)}
            colorClass={dnsmosColorClass(m.dnsmos_ovrl)}
          />
          {m.asr_confidence > 0 && (
            <MetricRow
              label="ASRProxy · Độ tin cậy"
              value={`${(m.asr_confidence * 100).toFixed(1)}%`}
              status={confidenceLabel(m.asr_confidence)}
              colorClass={confidenceColorClass(m.asr_confidence)}
            />
          )}
        </div>
      </div>

      <div>
        <p className="mb-1.5 font-mono text-[10px] uppercase tracking-[0.14em] text-ink-2">
          Bản ghi lúc kiểm tra
        </p>
        <p
          className={cn(
            'rounded-md border border-rule bg-paper px-3 py-2 text-sm leading-relaxed',
            m.sample_text.trim() ? 'text-ink' : 'text-ink-2'
          )}
        >
          {m.sample_text.trim()
            ? m.sample_text.trim()
            : 'Không nhận dạng được chữ. Nói rõ hơn, gần micro hơn, hoặc tải mô hình nhận dạng trong Cài đặt.'}
        </p>
      </div>

      {result.suggestions.length > 0 && (
        <div>
          <p className="mb-1.5 font-mono text-[10px] uppercase tracking-[0.14em] text-ink-2">
            Gợi ý cải thiện
          </p>
          <ul className="overflow-hidden rounded-md border border-rule">
            {result.suggestions.map((s) => (
              <li
                key={s}
                className="border-b border-rule px-3 py-2 text-sm text-ink last:border-b-0"
              >
                {stripSuggestionEmoji(s)}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

function MetricRow({
  label,
  value,
  status,
  colorClass,
}: {
  label: string;
  value: string;
  status: string;
  colorClass: string;
}) {
  return (
    <div className="flex items-center justify-between gap-3 border-b border-rule px-3 py-2 last:border-b-0">
      <span className="text-sm text-ink">{label}</span>
      <span className="shrink-0 font-mono text-xs text-ink">
        {value}{' '}
        <span className={cn('font-medium', colorClass)}>({status})</span>
      </span>
    </div>
  );
}
