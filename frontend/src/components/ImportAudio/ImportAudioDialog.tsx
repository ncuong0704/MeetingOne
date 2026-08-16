import React, { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import {
  Upload,
  Loader2,
  AlertCircle,
  CheckCircle2,
  X,
  FileAudio,
  Clock,
  HardDrive,
} from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Button } from '../ui/button';
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
        className="sm:max-w-[500px]"
        onEscapeKeyDown={handleEscapeKeyDown}
        onInteractOutside={handleInteractOutside}
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {isProcessing ? (
              <>
                <Loader2 className="h-5 w-5 animate-spin text-primary" />
                Đang nhập âm thanh...
              </>
            ) : error ? (
              <>
                <AlertCircle className="h-5 w-5 text-red-600" />
                Nhập file thất bại
              </>
            ) : status === 'complete' ? (
              <>
                <CheckCircle2 className="h-5 w-5 text-green-600" />
                Nhập file hoàn tất
              </>
            ) : (
              <>
                <Upload className="h-5 w-5 text-primary" />
                Nhập file âm thanh
              </>
            )}
          </DialogTitle>
          <DialogDescription>
            {isProcessing
              ? progress?.message || 'Đang xử lý âm thanh...'
              : error
              ? 'Đã xảy ra lỗi khi nhập file'
              : 'Chọn file âm thanh để tạo cuộc họp mới kèm bản ghi'}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          {/* File selection / info */}
          {!isProcessing && !error && (
            <>
              {fileInfo ? (
                <div className="bg-paper rounded-md border border-rule p-4 space-y-3">
                  <div className="flex items-start gap-3">
                    <FileAudio className="h-8 w-8 text-primary flex-shrink-0" />
                    <div className="flex-1 min-w-0">
                      <p className="font-medium text-ink">{fileInfo.filename}</p>
                      <div className="flex items-center gap-4 text-sm text-ink-2 mt-1">
                        <span className="flex items-center gap-1">
                          <Clock className="h-3.5 w-3.5" />
                          {formatDuration(fileInfo.duration_seconds)}
                        </span>
                        <span className="flex items-center gap-1">
                          <HardDrive className="h-3.5 w-3.5" />
                          {formatFileSize(fileInfo.size_bytes)}
                        </span>
                        <span className="text-primary font-medium">{fileInfo.format}</span>
                      </div>
                    </div>
                  </div>

                  {/* Editable title */}
                  <div className="space-y-1">
                    <label className="text-sm font-medium text-ink">Tiêu đề cuộc họp</label>
                    <Textarea
                      value={title}
                      onChange={(e) => {
                        setTitle(e.target.value);
                        setTitleModifiedByUser(true);
                      }}
                      placeholder="Nhập tiêu đề cuộc họp"
                      rows={2}
                    />
                  </div>

                  <Button variant="outline" size="sm" onClick={handleSelectFile} className="w-full">
                    Chọn file khác
                  </Button>
                </div>
              ) : (
                <div className="border-2 border-dashed border-rule rounded-md p-8 text-center">
                  <FileAudio className="h-12 w-12 text-ink-2 mx-auto mb-4" />
                  <Button onClick={handleSelectFile} disabled={status === 'validating'}>
                    {status === 'validating' ? (
                      <>
                        <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                        Đang kiểm tra...
                      </>
                    ) : (
                      <>
                        <Upload className="h-4 w-4 mr-2" />
                        Chọn file âm thanh
                      </>
                    )}
                  </Button>
                  <p className="text-sm text-ink-2 mt-2">MP4, WAV, MP3, FLAC, OGG, MKV, WebM, WMA</p>
                </div>
              )}

              <div className="space-y-3 pt-1">
                <label className="flex items-start gap-2 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={diarizationEnabled}
                    onChange={(e) => setDiarizationEnabled(e.target.checked)}
                    className="mt-1 accent-blue-600"
                  />
                  <span>
                    <span className="block text-sm font-medium text-ink">
                      Phân biệt người nói
                    </span>
                    <span className="block text-xs text-ink-2 mt-0.5">
                      Dùng Senko CAM++. Chỉ áp dụng cho file này.
                    </span>
                  </span>
                </label>
                {diarizationEnabled && (
                  <div className="space-y-1 pl-6">
                    <label className="block text-sm font-medium text-ink">
                      Số người nói (tuỳ chọn)
                    </label>
                    <input
                      type="number"
                      min={1}
                      max={20}
                      value={diarizationNumSpeakers}
                      onChange={(e) => setDiarizationNumSpeakers(e.target.value)}
                      placeholder="Tự đoán"
                      className="w-28 px-3 py-1.5 text-sm rounded-md border border-input bg-paper-2 text-ink focus:outline-none focus:ring-2 focus-visible:ring-ring"
                    />
                    <p className="text-xs text-ink-2">Để trống để tự đoán (1–20).</p>
                  </div>
                )}
              </div>
            </>
          )}

          {/* Progress display */}
          {isProcessing && (
            <div className="space-y-2">
              <div className="relative">
                <div className="w-full bg-paper-3 rounded-md h-3">
                  <div
                    className="bg-primary h-3 rounded-md transition-all duration-300 ease-out"
                    style={{
                      width: `${Math.min(progress?.progress_percentage ?? 5, 100)}%`,
                    }}
                  />
                </div>
                <div className="flex justify-between text-xs text-ink-2 mt-1">
                  <span>{progress?.stage ?? 'processing'}</span>
                  <span>{Math.round(progress?.progress_percentage ?? 0)}%</span>
                </div>
              </div>
              <p className="text-sm text-muted-foreground text-center">
                {progress?.message ?? 'Đang xử lý âm thanh...'}
              </p>
            </div>
          )}

          {/* Error display */}
          {error && (
            <div className="bg-red-50 border border-red-200 rounded-lg p-3">
              <p className="text-sm text-red-800">{error}</p>
            </div>
          )}
        </div>

        <DialogFooter>
          {!isProcessing && !error && (
            <>
              <Button variant="outline" onClick={() => onOpenChange(false)}>
                Hủy
              </Button>
              <Button
                onClick={handleStartImport}
                className="bg-primary hover:bg-primary-hover"
                disabled={!fileInfo}
              >
                <Upload className="h-4 w-4 mr-2" />
                Nhập
              </Button>
            </>
          )}
          {isProcessing && (
            <Button variant="outline" onClick={handleCancel}>
              <X className="h-4 w-4 mr-2" />
              Hủy
            </Button>
          )}
          {error && (
            <>
              <Button variant="outline" onClick={() => onOpenChange(false)}>
                Đóng
              </Button>
              <Button onClick={reset} variant="outline">
                Thử lại
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
