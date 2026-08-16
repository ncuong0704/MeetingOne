import React, { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { Upload, Loader2 } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Button } from '../ui/button';
import { Switch } from '../ui/switch';
import { Textarea } from '../ui/textarea';
import { toast } from 'sonner';
import { useConfig } from '@/contexts/ConfigContext';
import { useImportAudio, ImportResult } from '@/hooks/useImportAudio';
import { useRouter } from 'next/navigation';
import { useSidebar } from '../Sidebar/SidebarProvider';
import { useTranscriptionModels, ModelOption } from '@/hooks/useTranscriptionModels';
import { TranscriptConfigAPI } from '@/lib/asr';


interface ImportAudioDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  preselectedFile?: string | null;
  onComplete?: () => void;
}

function formatDuration(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);

  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
  }
  return `${minutes}:${secs.toString().padStart(2, '0')}`;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function parseNumSpeakers(raw: string): number | null {
  const parsed = raw.trim() ? Number(raw.trim()) : null;
  if (parsed !== null && Number.isFinite(parsed) && parsed >= 1 && parsed <= 20) {
    return Math.floor(parsed);
  }
  return null;
}

export function ImportAudioDialog({
  open,
  onOpenChange,
  preselectedFile,
  onComplete,
}: ImportAudioDialogProps) {
  const router = useRouter();
  const { refetchMeetings } = useSidebar();
  const { transcriptModelConfig } = useConfig();

  const [title, setTitle] = useState('');
  const [titleModifiedByUser, setTitleModifiedByUser] = useState(false);
  const [diarizationEnabled, setDiarizationEnabled] = useState(false);
  const [diarizationNumSpeakers, setDiarizationNumSpeakers] = useState('');

  // Always start as false — represents "dialog has not yet been opened".
  // Do NOT initialize from the `open` prop: if the component mounts with open=true
  // (e.g. drag-drop path), we still need the initialization effect to run.
  const prevOpenRef = useRef(false);

  // Use centralized model fetching hook
  const {
    availableModels,
    selectedModelKey,
    fetchModels,
    resetSelection,
  } = useTranscriptionModels(transcriptModelConfig);

  const handleImportComplete = useCallback((result: ImportResult) => {
    toast.success(`Nhập xong! Đã tạo ${result.segments_count} đoạn bản ghi.`);

    // Refresh meetings list then navigate to the imported meeting
    refetchMeetings();
    onComplete?.();
    onOpenChange(false);
    router.push(`/meeting-details?id=${result.meeting_id}&source=import`);
  }, [router, refetchMeetings, onComplete, onOpenChange]);

  const handleImportError = useCallback((error: string) => {
    toast.error('Nhập file thất bại', { description: error });
  }, []);

  const {
    status,
    fileInfo,
    progress,
    error,
    isProcessing,
    isBusy,
    selectFile,
    validateFile,
    startImport,
    cancelImport,
    reset,
  } = useImportAudio({
    onComplete: handleImportComplete,
    onError: handleImportError,
  });

  // Reset state only when dialog transitions from closed to open
  // This prevents re-initialization when config changes while dialog is already open (Bug #4 & #5)
  useEffect(() => {
    const wasOpen = prevOpenRef.current;
    prevOpenRef.current = open;

    // Only initialize when transitioning from closed (false) to open (true)
    if (open && !wasOpen) {
      reset();
      resetSelection();
      setTitle('');
      setTitleModifiedByUser(false);
      setDiarizationEnabled(false);
      setDiarizationNumSpeakers('');
      TranscriptConfigAPI.get()
        .then((bundle) => {
          setDiarizationEnabled(Boolean(bundle.shared?.diarizationEnabled));
          setDiarizationNumSpeakers(
            typeof bundle.shared?.diarizationNumSpeakers === 'number'
              ? String(bundle.shared.diarizationNumSpeakers)
              : '',
          );
        })
        .catch(() => undefined);

      // Validate preselected file if provided
      if (preselectedFile) {
        validateFile(preselectedFile).then((info) => {
          if (info) {
            setTitle(info.filename);
          }
        });
      }

      // Fetch available models using centralized hook
      fetchModels();
    }
  }, [open, preselectedFile, transcriptModelConfig, reset, resetSelection, validateFile, fetchModels]);

  // Update title when fileInfo changes
  useEffect(() => {
    if (fileInfo && !title && !titleModifiedByUser) {
      setTitle(fileInfo.filename);
    }
  }, [fileInfo, title, titleModifiedByUser]);

  const selectedModel = useMemo((): ModelOption | undefined => {
    if (!selectedModelKey) return undefined;
    const colonIndex = selectedModelKey.indexOf(':');
    if (colonIndex === -1) return undefined;
    const provider = selectedModelKey.slice(0, colonIndex);
    const name = selectedModelKey.slice(colonIndex + 1);
    return availableModels.find((m) => m.provider === provider && m.name === name);
  }, [selectedModelKey, availableModels]);

  const handleSelectFile = async () => {
    const info = await selectFile();
    if (info) {
      setTitle(info.filename);
    }
  };

  const handleStartImport = async () => {
    if (!fileInfo) return;

    await startImport(
      fileInfo.path,
      title || fileInfo.filename,
      null,
      selectedModel?.name || null,
      selectedModel?.provider || null,
      diarizationEnabled,
      diarizationEnabled ? parseNumSpeakers(diarizationNumSpeakers) : null
    );
  };

  const handleCancel = async () => {
    if (isProcessing) {
      await cancelImport();
      toast.info('Đã hủy nhập file');
    }
    onOpenChange(false);
  };

  // Prevent closing during processing
  const handleOpenChange = (newOpen: boolean) => {
    if (!newOpen && isProcessing) {
      return;
    }
    onOpenChange(newOpen);
  };

  const handleEscapeKeyDown = (event: KeyboardEvent) => {
    if (isProcessing) {
      event.preventDefault();
    }
  };

  const handleInteractOutside = (event: Event) => {
    if (isProcessing) {
      event.preventDefault();
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        className="gap-0 overflow-hidden p-0 sm:max-w-md"
        onEscapeKeyDown={handleEscapeKeyDown}
        onInteractOutside={handleInteractOutside}
      >
        <DialogHeader className="space-y-0 px-5 pb-4 pt-5 pr-12 text-left">
          <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-ink-2">
            {isProcessing ? 'Đang xử lý' : error ? 'Lỗi' : status === 'complete' ? 'Hoàn tất' : 'Âm thanh'}
          </p>
          <DialogTitle className="mt-1 text-base font-semibold tracking-[-0.02em] text-ink">
            {isProcessing
              ? 'Đang nhập âm thanh...'
              : error
                ? 'Nhập file thất bại'
                : status === 'complete'
                  ? 'Nhập file hoàn tất'
                  : 'Nhập file âm thanh'}
          </DialogTitle>
          <DialogDescription className="mt-1.5 text-xs text-ink-2">
            {isProcessing
              ? progress?.message || 'Đang xử lý âm thanh...'
              : error
                ? 'Đã xảy ra lỗi khi nhập file'
                : 'Chọn file âm thanh để tạo cuộc họp mới kèm bản ghi'}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3 px-5 pb-4">
          {!isProcessing && !error && (
            <>
              {fileInfo ? (
                <div className="overflow-hidden rounded-md border border-rule">
                  <div className="flex items-start justify-between gap-2 border-b border-rule px-3 py-2.5">
                    <div className="min-w-0">
                      <p className="truncate text-sm font-medium text-ink">{fileInfo.filename}</p>
                      <p className="mt-0.5 font-mono text-[11px] text-ink-2">
                        {formatDuration(fileInfo.duration_seconds)} · {formatFileSize(fileInfo.size_bytes)} · {fileInfo.format}
                      </p>
                    </div>
                    <Button variant="outline" size="sm" onClick={handleSelectFile}>
                      Đổi file
                    </Button>
                  </div>
                  <div className="px-3 py-2.5">
                    <label className="text-xs font-medium text-ink">Tiêu đề cuộc họp</label>
                    <Textarea
                      value={title}
                      onChange={(e) => {
                        setTitle(e.target.value);
                        setTitleModifiedByUser(true);
                      }}
                      placeholder="Nhập tiêu đề cuộc họp"
                      rows={2}
                      className="mt-1.5 min-h-0"
                    />
                  </div>
                </div>
              ) : (
                <div className="rounded-md border border-dashed border-rule px-3 py-8 text-center">
                  <Button size="sm" onClick={handleSelectFile} disabled={status === 'validating'}>
                    {status === 'validating' ? (
                      <>
                        <Loader2 className="h-4 w-4 animate-spin" />
                        Đang kiểm tra...
                      </>
                    ) : (
                      <>
                        <Upload className="h-4 w-4" />
                        Chọn file âm thanh
                      </>
                    )}
                  </Button>
                  <p className="mt-2 font-mono text-[11px] text-ink-2">
                    MP4, WAV, MP3, FLAC, OGG, MKV, WebM, WMA
                  </p>
                </div>
              )}

              <div className="rounded-md border border-rule px-3 py-2.5">
                <div className="flex items-center justify-between gap-3">
                  <div className="min-w-0">
                    <p className="text-sm font-medium text-ink">Phân biệt người nói</p>
                    <p className="mt-0.5 text-xs text-ink-2">Senko CAM++. Chỉ áp dụng cho file này.</p>
                  </div>
                  <Switch
                    checked={diarizationEnabled}
                    onCheckedChange={setDiarizationEnabled}
                  />
                </div>
                {diarizationEnabled && (
                  <div className="mt-2.5 border-t border-rule pt-2.5">
                    <label className="text-xs font-medium text-ink">Số người nói (tuỳ chọn)</label>
                    <input
                      type="number"
                      min={1}
                      max={20}
                      value={diarizationNumSpeakers}
                      onChange={(e) => setDiarizationNumSpeakers(e.target.value)}
                      placeholder="Tự đoán"
                      className="mt-1.5 w-28 rounded-md border border-rule bg-paper px-3 py-1.5 font-mono text-sm text-ink focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    />
                    <p className="mt-1 text-xs text-ink-2">Để trống để tự đoán (1–20).</p>
                  </div>
                )}
              </div>
            </>
          )}

          {isProcessing && (
            <div className="space-y-2">
              <div className="h-1.5 w-full overflow-hidden rounded-full border border-rule bg-paper">
                <div
                  className="h-full bg-primary transition-all"
                  style={{
                    width: `${Math.min(progress?.progress_percentage ?? 5, 100)}%`,
                  }}
                />
              </div>
              <div className="flex justify-between font-mono text-[11px] text-ink-2">
                <span>{progress?.stage ?? 'processing'}</span>
                <span>{Math.round(progress?.progress_percentage ?? 0)}%</span>
              </div>
              <p className="text-center text-xs text-ink-2">
                {progress?.message ?? 'Đang xử lý âm thanh...'}
              </p>
            </div>
          )}

          {error && (
            <div className="rounded-md border border-destructive/30 bg-paper px-3 py-2">
              <p className="text-sm text-destructive">{error}</p>
            </div>
          )}
        </div>

        <DialogFooter className="border-t border-rule bg-paper px-5 py-3">
          {!isProcessing && !error && (
            <>
              <Button variant="outline" size="sm" onClick={() => onOpenChange(false)}>
                Hủy
              </Button>
              <Button size="sm" onClick={handleStartImport} disabled={!fileInfo}>
                Nhập
              </Button>
            </>
          )}
          {isProcessing && (
            <Button variant="outline" size="sm" onClick={handleCancel}>
              Hủy
            </Button>
          )}
          {error && (
            <>
              <Button variant="outline" size="sm" onClick={() => onOpenChange(false)}>
                Đóng
              </Button>
              <Button variant="outline" size="sm" onClick={reset}>
                Thử lại
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
