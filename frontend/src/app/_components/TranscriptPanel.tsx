import { VirtualizedTranscriptView } from '@/components/VirtualizedTranscriptView';
import { PermissionWarning } from '@/components/PermissionWarning';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Copy } from 'lucide-react';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useConfig } from '@/contexts/ConfigContext';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { usePermissionCheck } from '@/hooks/usePermissionCheck';
import { ModalType } from '@/hooks/useModalState';
import { useIsLinux } from '@/hooks/usePlatform';
import { useMemo, useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { SpeakerHotkeyDialog } from '@/components/SpeakerHotkeyDialog';
import { useLiveSpeakerHotkeys } from '@/hooks/useLiveSpeakerHotkeys';

/**
 * TranscriptPanel Component
 *
 * Displays transcript content with controls for copying and language settings.
 * Uses TranscriptContext, ConfigContext, and RecordingStateContext internally.
 */

interface TranscriptPanelProps {
  // indicates stop-processing state for transcripts; derived from backend statuses.
  isProcessingStop: boolean;
  isStopping: boolean;
  showModal: (name: ModalType, message?: string) => void;
}

export function TranscriptPanel({
  isProcessingStop,
  isStopping,
  showModal
}: TranscriptPanelProps) {
  // Contexts
  const { transcripts, transcriptContainerRef, copyTranscript, updateTranscriptBySequenceId } = useTranscripts();
  const { transcriptModelConfig } = useConfig();
  const { isRecording, isPaused } = useRecordingState();
  const { checkPermissions, isChecking, hasSystemAudio, hasMicrophone, hasMicrophoneAccess } = usePermissionCheck();
  const isLinux = useIsLinux();
  const { pendingName, pendingColor, reloadHotkeys } = useLiveSpeakerHotkeys(isRecording);
  const [hotkeyOpen, setHotkeyOpen] = useState(false);

  // Convert transcripts to segments for virtualized view
  const segments = useMemo(() =>
    transcripts.map(t => ({
      id: t.id,
      timestamp: t.audio_start_time ?? 0,
      endTime: t.audio_end_time,
      text: t.text,
      confidence: t.confidence,
      sequenceId: t.sequence_id,
      speakerName: t.speaker_name,
      speakerColor: t.speaker_color,
    })),
    [transcripts]
  );

  const handleSegmentEdit = useCallback(
    async (_segmentId: string, newText: string, sequenceId?: number) => {
      try {
        if (isRecording && typeof sequenceId === 'number') {
          await invoke('update_live_transcript_segment', { sequenceId, newText });
          updateTranscriptBySequenceId(sequenceId, newText);
          toast.success('Đã cập nhật bản ghi', { duration: 1500 });
          return;
        }
        if (typeof sequenceId === 'number') {
          updateTranscriptBySequenceId(sequenceId, newText);
        }
      } catch {
        toast.error('Không lưu được chỉnh sửa bản ghi');
        throw new Error('update_transcript_failed');
      }
    },
    [isRecording, updateTranscriptBySequenceId]
  );

  return (
    <div className="flex-1 min-h-0 w-full bg-paper flex flex-col">
      <div className="shrink-0 flex items-center justify-between gap-3 h-11 px-5 border-b border-rule bg-paper-2">
        <div className="min-w-0">
          <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-ink-2">Bản ghi trực tiếp</p>
        </div>
        <ButtonGroup>
          {transcripts?.length > 0 && (
            <Button
              variant="outline"
              size="sm"
              onClick={copyTranscript}
              title="Sao chép bản ghi"
            >
              <Copy />
              <span className='hidden md:inline'>
                Sao chép
              </span>
            </Button>
          )}
          <Button
            variant="outline"
            size="sm"
            onClick={() => setHotkeyOpen(true)}
            title="Cấu hình phím tắt người nói"
          >
            <span className='hidden md:inline'>Người nói 1–9</span>
            <span className='md:hidden'>1–9</span>
          </Button>
        </ButtonGroup>
      </div>

      <div
        ref={transcriptContainerRef}
        className="flex-1 overflow-y-auto"
      >
        <div className="flex justify-center px-6 py-5">
          <div className="w-full max-w-[42rem] min-h-[200px]">
            {!isRecording && !isChecking && !isLinux && (
              <div className="flex justify-center pt-4">
                <PermissionWarning
                  hasMicrophone={hasMicrophone}
                  hasMicrophoneAccess={hasMicrophoneAccess}
                  hasSystemAudio={hasSystemAudio}
                  onRecheck={checkPermissions}
                  isRechecking={isChecking}
                />
              </div>
            )}

            <VirtualizedTranscriptView
              segments={segments}
              onSegmentEdit={handleSegmentEdit}
              isRecording={isRecording}
              isPaused={isPaused}
              isProcessing={isProcessingStop}
              isStopping={isStopping}
              enableStreaming={isRecording}
              showConfidence={true}
              pendingSpeakerName={pendingName}
              pendingSpeakerColor={pendingColor}
            />
          </div>
        </div>
      </div>
      <SpeakerHotkeyDialog
        open={hotkeyOpen}
        onOpenChange={(open) => {
          setHotkeyOpen(open);
          if (!open) reloadHotkeys();
        }}
      />
    </div>
  );
}
