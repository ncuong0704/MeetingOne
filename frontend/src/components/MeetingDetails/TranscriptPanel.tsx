"use client";

import { Transcript, TranscriptSegmentData } from '@/types';
import { TranscriptView } from '@/components/TranscriptView';
import { VirtualizedTranscriptView } from '@/components/VirtualizedTranscriptView';
import { TranscriptButtonGroup } from './TranscriptButtonGroup';
import { AudioPlayer } from './AudioPlayer';
import { useMemo, useState, useCallback } from 'react';
import { Headphones } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

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
    }));
  }, [transcripts, usePagination, segments]);

  const segmentCount = usePagination ? (totalCount ?? convertedSegments.length) : (transcripts?.length || 0);

  const [showAudioPlayer, setShowAudioPlayer] = useState(false);

  const handleSegmentEdit = useCallback(async (segmentId: string, newText: string, _sequenceId?: number) => {
    await invoke('api_update_transcript_text', { transcriptId: segmentId, newText });
    toast.success('Đã cập nhật bản ghi', { duration: 1500 });
  }, []);

  return (
    <div className="relative hidden h-full min-h-0 w-full min-w-0 flex-col bg-white md:flex">
      {/* Header */}
      <div className="flex shrink-0 items-center justify-between gap-2 border-b border-gray-100 px-4 py-3 min-w-0">
        <div className="flex items-center gap-2 shrink-0">
          <span className="text-sm font-semibold text-gray-800">Bản ghi</span>
          {segmentCount > 0 && (
            <span className="inline-flex items-center rounded-full bg-gray-100 px-2 py-0.5 text-xs font-medium text-gray-500">
              {segmentCount}
            </span>
          )}
        </div>
        <div className="flex items-center gap-1.5 shrink-0">
          {/* Audio player toggle — shown when meeting has a recording folder */}
          {meetingFolderPath && (
            <button
              onClick={() => setShowAudioPlayer(v => !v)}
              title={showAudioPlayer ? 'Ẩn trình phát audio' : 'Phát audio ghi âm'}
              className={`flex h-7 w-7 items-center justify-center rounded-md border transition-colors ${
                showAudioPlayer
                  ? 'border-[rgba(22,71,142,0.35)] bg-[rgba(22,71,142,0.12)] text-[#16478e]'
                  : 'border-gray-200 bg-white text-gray-500 hover:bg-gray-50 hover:text-gray-700'
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
        <AudioPlayer meetingFolderPath={meetingFolderPath} />
      )}

      {/* Transcript content */}
      <div className="min-h-0 flex-1 overflow-hidden">
        <VirtualizedTranscriptView
          segments={convertedSegments}
          onSegmentEdit={handleSegmentEdit}
          isRecording={isRecording}
          isPaused={false}
          isProcessing={false}
          isStopping={false}
          enableStreaming={false}
          showConfidence={true}
          disableAutoScroll={disableAutoScroll}
          hasMore={hasMore}
          isLoadingMore={isLoadingMore}
          totalCount={totalCount}
          loadedCount={loadedCount}
          onLoadMore={onLoadMore}
        />
      </div>
    </div>
  );
}
