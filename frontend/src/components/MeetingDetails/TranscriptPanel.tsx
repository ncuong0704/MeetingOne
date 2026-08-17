"use client";

import { Transcript, TranscriptSegmentData } from '@/types';
import { TranscriptView } from '@/components/TranscriptView';
import { FlowingTranscriptView } from '@/components/FlowingTranscriptView';
import { TranscriptButtonGroup } from './TranscriptButtonGroup';
import { AudioPlayer, type AudioPlayerControls } from './AudioPlayer';
import { SpeakerListDialog, speakersFromApi } from './SpeakerListDialog';
import { useMemo, useState, useCallback, useRef, useEffect } from 'react';
import { Headphones, Users } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useTranscriptAudioSync } from '@/hooks/useTranscriptAudioSync';
import { DiarizationAPI, type MeetingSpeaker } from '@/lib/asr';
import { previewStopTime, shouldShowSpeakerButton } from '@/lib/speakerPreview';

interface TranscriptPanelProps {
  transcripts: Transcript[];
  onCopyTranscript: () => void;
  onOpenMeetingFolder: () => Promise<void>;
  isRecording: boolean;
  disableAutoScroll?: boolean;

  // Optional pagination props (when using virtualization)
  usePagination?: boolean;
  segments?: TranscriptSegmentData[];
  hasMore?: boolean;
  isLoadingMore?: boolean;
  totalCount?: number;
  loadedCount?: number;
  onLoadMore?: () => void;

  // Retranscription props
  meetingId?: string;
  meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>;
}

export function TranscriptPanel({
  transcripts,
  onCopyTranscript,
  onOpenMeetingFolder,
  isRecording,
  disableAutoScroll = false,
  usePagination = false,
  segments,
  hasMore,
  isLoadingMore,
  totalCount,
  loadedCount,
  onLoadMore,
  meetingId,
  meetingFolderPath,
  onRefetchTranscripts,
}: TranscriptPanelProps) {
  // Convert transcripts to segments if pagination is not used but we want virtualization
  const convertedSegments = useMemo(() => {
    if (usePagination && segments) {
      return segments;
    }
    // Convert transcripts to segments for virtualization
    return transcripts.map(t => ({
      id: t.id,
      timestamp: t.audio_start_time ?? 0,
      endTime: t.audio_end_time,
      text: t.text,
      confidence: t.confidence,
      sequenceId: t.sequence_id,
      speakerId: t.speaker_id,
      speakerName: t.speaker_name,
      speakerColor: t.speaker_color,
    }));
  }, [transcripts, usePagination, segments]);

  const segmentCount = usePagination ? (totalCount ?? convertedSegments.length) : (transcripts?.length || 0);

  const [showAudioPlayer, setShowAudioPlayer] = useState(false);
  const [speakerDialogOpen, setSpeakerDialogOpen] = useState(false);
  const [meetingSpeakers, setMeetingSpeakers] = useState<MeetingSpeaker[]>([]);
  const seekRef = useRef<((time: number) => void) | null>(null);
  const playerControlsRef = useRef<AudioPlayerControls | null>(null);
  const pendingSeekRef = useRef<number | null>(null);
  const pendingPreviewRef = useRef<number | null>(null);
  const previewUntilRef = useRef<number | null>(null);
  const [currentTime, setCurrentTime] = useState(0);
  const [audioLoaded, setAudioLoaded] = useState(false);

  const speakerRows = useMemo(() => speakersFromApi(meetingSpeakers), [meetingSpeakers]);
  const showSpeakerButton = shouldShowSpeakerButton(speakerRows);

  const loadMeetingSpeakers = useCallback(async () => {
    if (!meetingId) {
      setMeetingSpeakers([]);
      return;
    }
    try {
      const rows = await DiarizationAPI.listSpeakers(meetingId);
      setMeetingSpeakers(rows);
    } catch {
      setMeetingSpeakers([]);
    }
  }, [meetingId]);

  useEffect(() => {
    void loadMeetingSpeakers();
  }, [loadMeetingSpeakers]);

  useEffect(() => {
    if (speakerDialogOpen) void loadMeetingSpeakers();
  }, [speakerDialogOpen, loadMeetingSpeakers]);

  const applyPendingPreview = useCallback(() => {
    const start = pendingPreviewRef.current;
    const controls = playerControlsRef.current;
    if (start === null || !controls) return;
    pendingPreviewRef.current = null;
    previewUntilRef.current = previewStopTime(start);
    controls.seek(start);
    void controls.play();
  }, []);

  const handleTimeUpdate = useCallback((t: number) => {
    setCurrentTime(t);
    setAudioLoaded(true);
    const until = previewUntilRef.current;
    if (until !== null && t >= until) {
      previewUntilRef.current = null;
      playerControlsRef.current?.pause();
    }
  }, []);

  const handlePlayerReady = useCallback(() => {
    setAudioLoaded(true);
    applyPendingPreview();
  }, [applyPendingPreview]);

  const handleSpeakerPreview = useCallback(
    (start: number) => {
      if (!meetingFolderPath) return;
      pendingPreviewRef.current = start;
      if (!showAudioPlayer) {
        setShowAudioPlayer(true);
        return;
      }
      applyPendingPreview();
    },
    [meetingFolderPath, showAudioPlayer, applyPendingPreview],
  );

  const handleSpeakerRename = useCallback(
    async (speakerId: string, displayName: string) => {
      await DiarizationAPI.renameSpeaker(speakerId, displayName);
      await loadMeetingSpeakers();
      await onRefetchTranscripts?.();
      setSpeakerDialogOpen(true);
    },
    [loadMeetingSpeakers, onRefetchTranscripts],
  );

  const handleSpeakerMerge = useCallback(
    async (sourceId: string, targetId: string) => {
      await DiarizationAPI.mergeSpeakers(sourceId, targetId);
      await loadMeetingSpeakers();
      await onRefetchTranscripts?.();
      setSpeakerDialogOpen(true);
      toast.success('Đã gộp người nói');
    },
    [loadMeetingSpeakers, onRefetchTranscripts],
  );

  useEffect(() => {
    setAudioLoaded(false);
    setCurrentTime(0);
  }, [meetingFolderPath]);

  useEffect(() => {
    if (!showAudioPlayer) {
      setAudioLoaded(false);
      setCurrentTime(0);
      pendingSeekRef.current = null;
      pendingPreviewRef.current = null;
      previewUntilRef.current = null;
    }
  }, [showAudioPlayer]);

  useEffect(() => {
    if (!audioLoaded || pendingSeekRef.current === null) return;
    const t = pendingSeekRef.current;
    pendingSeekRef.current = null;
    seekRef.current?.(t);
  }, [audioLoaded]);

  const isPlaybackActive = showAudioPlayer && audioLoaded;

  const { activeSegmentId, handleSegmentClick } = useTranscriptAudioSync({
    segments: convertedSegments,
    currentTime,
    isPlaybackActive,
    seekRef,
    hasMore,
    onLoadMore,
  });

  const onSegmentClick = useCallback(
    (segment: TranscriptSegmentData) => {
      const t = segment.timestamp ?? 0;
      if (!showAudioPlayer) setShowAudioPlayer(true);
      if (!seekRef.current) pendingSeekRef.current = t;
      handleSegmentClick(segment);
    },
    [showAudioPlayer, handleSegmentClick],
  );

  const handleSegmentEdit = useCallback(async (segmentId: string, newText: string, _sequenceId?: number) => {
    await invoke('api_update_transcript_text', { transcriptId: segmentId, newText });
    toast.success('Đã cập nhật bản ghi', { duration: 1500 });
  }, []);

  return (
    <div className="relative flex h-full min-h-0 w-full min-w-0 flex-col bg-paper-2">
      <div className="flex shrink-0 items-center justify-between gap-2 border-b border-rule h-11 px-4 min-w-0">
        <div className="flex items-center gap-2 shrink-0">
          <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-ink-2">Bản ghi</span>
          {segmentCount > 0 && (
            <span className="inline-flex items-center rounded-md bg-paper px-1.5 py-0.5 font-mono text-[11px] tabular-nums text-ink-2">
              {segmentCount}
            </span>
          )}
        </div>
        <div className="flex items-center gap-1.5 shrink-0">
          {showSpeakerButton && (
            <button
              type="button"
              onClick={() => setSpeakerDialogOpen(true)}
              title="Người nói"
              className={`flex h-7 w-7 items-center justify-center rounded-md border transition-colors ${
                speakerDialogOpen
                  ? 'border-primary/35 bg-primary/10 text-primary'
                  : 'border-rule bg-paper-2 text-muted-foreground hover:bg-secondary hover:text-foreground'
              }`}
            >
              <Users className="h-3.5 w-3.5" />
            </button>
          )}
          {/* Audio player toggle — shown when meeting has a recording folder */}
          {meetingFolderPath && (
            <button
              onClick={() => setShowAudioPlayer(v => !v)}
              title={showAudioPlayer ? 'Ẩn trình phát audio' : 'Phát audio ghi âm'}
              className={`flex h-7 w-7 items-center justify-center rounded-md border transition-colors ${
                showAudioPlayer
                  ? 'border-primary/35 bg-primary/10 text-primary'
                  : 'border-rule bg-paper-2 text-muted-foreground hover:bg-secondary hover:text-foreground'
              }`}
            >
              <Headphones className="h-3.5 w-3.5" />
            </button>
          )}
          {/* [&_span]:hidden forces icon-only buttons regardless of viewport width */}
          <div className="[&_span]:hidden">
            <TranscriptButtonGroup
              transcriptCount={segmentCount}
              onCopyTranscript={onCopyTranscript}
              onOpenMeetingFolder={onOpenMeetingFolder}
              meetingId={meetingId}
              meetingFolderPath={meetingFolderPath}
              onRefetchTranscripts={onRefetchTranscripts}
            />
          </div>
        </div>
      </div>

      {/* Audio player — shown when toggled */}
      {showAudioPlayer && meetingFolderPath && (
        <AudioPlayer
          meetingFolderPath={meetingFolderPath}
          seekRef={seekRef}
          controlsRef={playerControlsRef}
          onTimeUpdate={handleTimeUpdate}
          onReady={handlePlayerReady}
        />
      )}

      {/* Transcript content */}
      <div className="min-h-0 flex-1 overflow-hidden">
        <FlowingTranscriptView
          segments={convertedSegments}
          onSegmentEdit={handleSegmentEdit}
          hasMore={hasMore}
          isLoadingMore={isLoadingMore}
          totalCount={totalCount}
          loadedCount={loadedCount}
          onLoadMore={onLoadMore}
          activeSegmentId={activeSegmentId}
          onSegmentClick={meetingFolderPath ? onSegmentClick : undefined}
          playbackFollow={showAudioPlayer && !!meetingFolderPath}
          onSpeakersChanged={async () => {
            await onRefetchTranscripts?.();
            await loadMeetingSpeakers();
          }}
        />
      </div>

      {meetingId && (
        <SpeakerListDialog
          open={speakerDialogOpen}
          onOpenChange={setSpeakerDialogOpen}
          speakers={speakerRows}
          canPlay={!!meetingFolderPath}
          onRename={handleSpeakerRename}
          onPreview={handleSpeakerPreview}
          onMerge={handleSpeakerMerge}
        />
      )}
    </div>
  );
}
