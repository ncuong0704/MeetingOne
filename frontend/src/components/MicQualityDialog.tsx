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

type Phase = 'idle' | 'need_download' | 'downloading' | 'recording' | 'analyzing' | 'result' | 'error';

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

  const busy = phase === 'recording' || phase === 'analyzing' || phase === 'downloading';
  const statusText =
    phase === 'downloading'
      ? `Đang tải model DNSMOS... ${percent}%`
      : phase === 'recording'
        ? `Đang ghi âm... ${percent}%`
        : phase === 'analyzing'
          ? 'Đang phân tích...'
          : phase === 'need_download'
            ? 'Cần tải model DNSMOS (~5MB)'
            : 'Sẵn sàng';

  return (
    <Dialog open={open} onOpenChange={(next) => { if (!next) handleClose(); }}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>
            {phase === 'result' ? 'Kết quả đánh giá chất lượng âm thanh' : 'Đánh giá Microphone'}
          </DialogTitle>
          <DialogDescription>
            {phase === 'result'
              ? result?.is_ready
                ? 'Sẵn sàng cho nhận dạng'
                : 'Cần cải thiện chất lượng'
              : 'Đứng tại vị trí phát biểu và nói nội dung bất kỳ (ví dụ: «Một hai ba bốn năm»). Thời gian ghi âm: 8–10 giây.'}
          </DialogDescription>
        </DialogHeader>

        {phase !== 'result' && (
          <div className="space-y-3">
            {(busy || phase === 'need_download') && (
              <div className="h-2 w-full overflow-hidden rounded-full bg-gray-200">
                <div
                  className="h-full bg-amber-400 transition-all"
                  style={{ width: `${phase === 'need_download' ? 0 : percent}%` }}
                />
              </div>
            )}
            <p className="text-center text-sm text-gray-500">{statusText}</p>
            {error && <p className="text-sm text-red-600">{error}</p>}
          </div>
        )}

        {phase === 'result' && result && (
          <ResultBody result={result} />
        )}

        <DialogFooter>
          {phase === 'need_download' && (
            <Button onClick={handleDownload}>Tải model DNSMOS</Button>
          )}
          {(phase === 'idle' || phase === 'error') && (
            <Button onClick={handleStart}>Bắt đầu ghi âm</Button>
          )}
          {phase === 'result' && (
            <Button variant="outline" onClick={handleStart}>
              Thử lại
            </Button>
          )}
          <Button variant="outline" onClick={handleClose} disabled={phase === 'analyzing'}>
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
    <div className="space-y-3 text-sm">
      <p className={`font-semibold ${result.is_ready ? 'text-green-600' : 'text-red-600'}`}>
        {result.is_ready ? '✓ Sẵn sàng cho nhận dạng' : '⚠ Cần cải thiện chất lượng'}
      </p>
      {m.duration_analyzed > 0 && (
        <p className="text-xs text-gray-500">
          Đã phân tích: {m.duration_analyzed.toFixed(1)}s, {m.num_segments} đoạn
        </p>
      )}
      <p className="text-xs font-semibold uppercase tracking-wide text-gray-500">
        Chất lượng âm thanh (DNSMOS)
      </p>
      <MetricRow
        label="SIG - Chất lượng giọng nói"
        value={`${m.dnsmos_sig.toFixed(2)}/5`}
        status={dnsmosLabel(m.dnsmos_sig)}
        colorClass={dnsmosColorClass(m.dnsmos_sig)}
      />
      <MetricRow
        label="BAK - Chất lượng nhiễu, vang nền"
        value={`${m.dnsmos_bak.toFixed(2)}/5`}
        status={dnsmosLabel(m.dnsmos_bak)}
        colorClass={dnsmosColorClass(m.dnsmos_bak)}
      />
      <MetricRow
        label="OVRL - Tổng thể"
        value={`${m.dnsmos_ovrl.toFixed(2)}/5`}
        status={dnsmosLabel(m.dnsmos_ovrl)}
        colorClass={dnsmosColorClass(m.dnsmos_ovrl)}
      />
      {m.asr_confidence > 0 && (
        <MetricRow
          label="ASRProxy - Độ tự tin nhận dạng"
          value={`${(m.asr_confidence * 100).toFixed(1)}%`}
          status={confidenceLabel(m.asr_confidence)}
          colorClass={confidenceColorClass(m.asr_confidence)}
        />
      )}
      {m.sample_text && (
        <div>
          <p className="mb-1 text-xs font-semibold uppercase tracking-wide text-gray-500">
            Chữ nhận dạng được
          </p>
          <p className="rounded-md border border-gray-200 bg-gray-50 px-3 py-2 text-gray-800">
            {m.sample_text}
          </p>
        </div>
      )}
      {result.suggestions.length > 0 && (
        <div>
          <p className="mb-1 text-xs font-semibold uppercase tracking-wide text-gray-500">
            Gợi ý cải thiện
          </p>
          <ul className="space-y-1 text-gray-700">
            {result.suggestions.map((s) => (
              <li key={s}>• {stripSuggestionEmoji(s)}</li>
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
    <div className="flex items-baseline justify-between gap-2">
      <span className="text-gray-700">{label}:</span>
      <span className="shrink-0">
        {value} <span className={`font-semibold ${colorClass}`}>({status})</span>
      </span>
    </div>
  );
}
