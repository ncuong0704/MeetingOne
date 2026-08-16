'use client';

import { useCallback, useRef, useReducer, startTransition, useEffect, useState, memo } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useAutoScroll } from "@/hooks/useAutoScroll";
import { usePlaybackFollowScroll } from "@/hooks/usePlaybackFollowScroll";
import { useTranscriptStreaming } from "@/hooks/useTranscriptStreaming";
import { ConfidenceIndicator } from "./ConfidenceIndicator";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { RecordingStatusBar } from "./RecordingStatusBar";
import { motion, AnimatePresence } from "framer-motion";
import { TranscriptSegmentData } from "@/types";
import { Pencil, Check, X } from "lucide-react";

export interface VirtualizedTranscriptViewProps {
    /** Transcript segments to display */
    segments: TranscriptSegmentData[];
    /** Called when user saves an edit to a segment. Return promise; component handles optimistic update. */
    onSegmentEdit?: (segmentId: string, newText: string, sequenceId?: number) => Promise<void>;
    /** Whether recording is in progress */
    isRecording?: boolean;
    /** Whether recording is paused */
    isPaused?: boolean;
    /** Whether processing/finalizing transcription */
    isProcessing?: boolean;
    /** Whether stopping */
    isStopping?: boolean;
    /** Enable streaming effect for latest segment */
    enableStreaming?: boolean;
    /** Show confidence indicators */
    showConfidence?: boolean;
    /** Completely disable auto-scroll behavior (for meeting details page) */
    disableAutoScroll?: boolean;

    // Pagination props (infinite scroll)
    hasMore?: boolean;
    isLoadingMore?: boolean;
    totalCount?: number;
    loadedCount?: number;
    onLoadMore?: () => void;

    /** Segment highlighted during audio playback */
    activeSegmentId?: string | null;
    /** Click whole row to seek audio */
    onSegmentClick?: (segment: TranscriptSegmentData) => void;
    /** Scroll to active segment only when out of viewport */
    playbackFollow?: boolean;
    /** Show the [MM:SS] label next to each segment. Off for post-hoc meeting/file-import
     * review (matches the reference app's plain-text transcript); on during live
     * recording, where it's useful for tracking progress. */
    showTimestamps?: boolean;
    pendingSpeakerName?: string | null;
    pendingSpeakerColor?: string | null;
}

// Threshold for enabling virtualization (below this, use simple rendering)
const VIRTUALIZATION_THRESHOLD = 10;

// Helper function to format seconds as recording-relative time [MM:SS]
function formatRecordingTime(seconds: number | undefined): string {
    if (seconds === undefined) return '[--:--]';

    const totalSeconds = Math.floor(seconds);
    const minutes = Math.floor(totalSeconds / 60);
    const secs = totalSeconds % 60;

    return `[${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}]`;
}

// Helper function to remove filler words and repetitions
function cleanStopWords(text: string): string {
    const stopWords = ['uh', 'um', 'er', 'ah', 'hmm', 'hm', 'eh', 'oh'];

    let cleanedText = text;
    stopWords.forEach(word => {
        const pattern = new RegExp(`\\b${word}\\b[,\\s]*`, 'gi');
        cleanedText = cleanedText.replace(pattern, ' ');
    });

    return cleanedText.replace(/\s+/g, ' ').trim();
}

// Memoized transcript segment component with inline editing
const TranscriptSegment = memo(function TranscriptSegment({
    id,
    timestamp,
    text,
    fullText,
    confidence,
    isStreaming,
    showConfidence,
    showTimestamps,
    sequenceId,
    onEdit,
    isActive,
    onRowClick,
    speakerName,
    speakerColor,
    showSpeakerLabel,
}: {
    id: string;
    timestamp: number;
    text: string;
    fullText: string;
    confidence?: number;
    isStreaming: boolean;
    showConfidence: boolean;
    showTimestamps: boolean;
    sequenceId?: number;
    onEdit?: (segmentId: string, newText: string, sequenceId?: number) => Promise<void>;
    isActive?: boolean;
    onRowClick?: () => void;
    speakerName?: string | null;
    speakerColor?: string | null;
    showSpeakerLabel?: boolean;
}) {
    const displayText = cleanStopWords(text) || (text.trim() === '' ? '[Im lặng]' : text);
    const [isEditing, setIsEditing] = useState(false);
    const [editValue, setEditValue] = useState(text);
    const [isSaving, setIsSaving] = useState(false);
    const [optimisticText, setOptimisticText] = useState<string | null>(null);
    const textareaRef = useRef<HTMLTextAreaElement>(null);

    // Auto-resize textarea
    useEffect(() => {
        if (isEditing && textareaRef.current) {
            textareaRef.current.style.height = 'auto';
            textareaRef.current.style.height = `${textareaRef.current.scrollHeight}px`;
            textareaRef.current.focus();
            textareaRef.current.select();
        }
    }, [isEditing]);

    const handleStartEdit = () => {
        setEditValue(optimisticText ?? fullText);
        setIsEditing(true);
    };

    const handleSave = async () => {
        const trimmed = editValue.trim();
        if (!trimmed || trimmed === (optimisticText ?? text)) {
            setIsEditing(false);
            return;
        }
        setIsSaving(true);
        setOptimisticText(trimmed); // optimistic update
        setIsEditing(false);
        try {
            await onEdit?.(id, trimmed, sequenceId);
        } catch {
            setOptimisticText(null); // rollback on error
        } finally {
            setIsSaving(false);
        }
    };

    const handleCancel = () => {
        setEditValue(optimisticText ?? text);
        setIsEditing(false);
    };

    const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
        if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            handleSave();
        }
        if (e.key === 'Escape') handleCancel();
    };

    const shownText = optimisticText ?? displayText;

    return (
        <div
            id={`segment-${id}`}
            className={`mb-3 group/seg rounded-md transition-colors ${
                isActive ? 'bg-[rgba(255,215,0,0.25)]' : ''
            } ${onRowClick && !isEditing ? 'cursor-pointer hover:bg-secondary' : ''}`}
            onClick={() => {
                if (!isEditing) onRowClick?.();
            }}
        >
            <div className="flex items-start gap-2">
                {/* Timestamp */}
                {showTimestamps && (
                <Tooltip>
                    <TooltipTrigger>
                        <span
                            className={`text-xs mt-1 flex-shrink-0 min-w-[50px] tabular-nums ${
                                isActive ? 'text-primary font-semibold' : 'text-ink-2'
                            }`}
                        >
                            {formatRecordingTime(timestamp)}
                        </span>
                    </TooltipTrigger>
                    <TooltipContent>
                        {confidence !== undefined && showConfidence && (
                            <ConfidenceIndicator confidence={confidence} showIndicator={showConfidence} />
                        )}
                    </TooltipContent>
                </Tooltip>
                )}

                {/* Content */}
                <div className="flex-1 min-w-0">
                    {showSpeakerLabel && speakerName && (
                        <div
                            className="mb-1 pt-1 border-t border-dashed text-xs font-semibold"
                            style={{
                                color: speakerColor || '#2563EB',
                                borderColor: speakerColor || '#2563EB',
                            }}
                        >
                            {speakerName}
                        </div>
                    )}
                    {isEditing ? (
                        // Edit mode
                        <div className="rounded-lg border border-primary/40 bg-primary/5 ring-2 ring-primary/20 overflow-hidden">
                            <textarea
                                ref={textareaRef}
                                value={editValue}
                                onChange={e => {
                                    setEditValue(e.target.value);
                                    e.target.style.height = 'auto';
                                    e.target.style.height = `${e.target.scrollHeight}px`;
                                }}
                                onKeyDown={handleKeyDown}
                                className="w-full px-3 py-2 text-base text-ink leading-relaxed bg-transparent resize-none focus:outline-none"
                                rows={1}
                            />
                            <div className="flex items-center justify-end gap-1 px-2 pb-1.5 pt-0">
                                <span className="text-[10px] text-ink-2 mr-auto">Enter để lưu · Esc để huỷ</span>
                                <button
                                    onClick={handleCancel}
                                    className="flex items-center gap-1 px-2 py-0.5 text-xs text-ink-2 hover:text-ink rounded-md hover:bg-secondary transition-colors"
                                >
                                    <X className="w-3 h-3" /> Huỷ
                                </button>
                                <button
                                    onClick={handleSave}
                                    className="flex items-center gap-1 px-2 py-0.5 text-xs text-primary-foreground bg-primary hover:bg-primary-hover rounded-md transition-colors"
                                >
                                    <Check className="w-3 h-3" /> Lưu
                                </button>
                            </div>
                        </div>
                    ) : (
                        // View mode — show edit button on hover, streaming or not
                        <div className={`flex items-start gap-1.5${isStreaming ? ' bg-paper-3 border border-rule rounded-md px-3 py-2' : ''}`}>
                            <p className={`flex-1 text-base leading-relaxed ${isSaving ? 'text-ink-2' : 'text-ink'}`}>
                                {shownText}
                                {isSaving && <span className="ml-1.5 text-xs text-ink-2">Đang lưu...</span>}
                            </p>
                            {onEdit && (
                                <button
                                    onClick={(e) => {
                                        e.stopPropagation();
                                        handleStartEdit();
                                    }}
                                    title="Chỉnh sửa đoạn này"
                                    className="opacity-0 group-hover/seg:opacity-100 mt-1 flex-shrink-0 p-1 rounded-md text-ink-2 hover:text-primary hover:bg-primary/10 transition-all"
                                >
                                    <Pencil className="w-3.5 h-3.5" />
                                </button>
                            )}
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
});

export const VirtualizedTranscriptView: React.FC<VirtualizedTranscriptViewProps> = ({
    segments,
    onSegmentEdit,
    isRecording = false,
    isPaused = false,
    isProcessing = false,
    isStopping = false,
    enableStreaming = false,
    showConfidence = true,
    disableAutoScroll = false,
    hasMore = false,
    isLoadingMore = false,
    totalCount = 0,
    loadedCount = 0,
    onLoadMore,
    activeSegmentId = null,
    onSegmentClick,
    playbackFollow = false,
    showTimestamps = true,
    pendingSpeakerName = null,
    pendingSpeakerColor = null,
}) => {
    // Create scroll ref first - shared between virtualizer and auto-scroll hook
    const scrollRef = useRef<HTMLDivElement>(null);
    // Ref for infinite scroll trigger element
    const loadMoreTriggerRef = useRef<HTMLDivElement>(null);

    // Force re-render without flushSync (avoids React warning)
    const [, rerender] = useReducer((x: number) => x + 1, 0);

    // Setup virtualizer for efficient rendering of large lists
    const virtualizer = useVirtualizer({
        count: segments.length,
        getScrollElement: () => scrollRef.current,
        estimateSize: () => 60, // Estimated height per segment
        overscan: 10, // Render extra items above/below viewport
        onChange: () => {
            startTransition(() => {
                rerender();
            });
        },
    });

    // Custom hook for auto-scrolling (supports both virtualized and non-virtualized)
    useAutoScroll({
        scrollRef,
        segments,
        isRecording,
        isPaused,
        virtualizer,
        virtualizationThreshold: VIRTUALIZATION_THRESHOLD,
        disableAutoScroll,
    });

    // Streaming text effect hook (typewriter animation for new transcripts)
    const { streamingSegmentId, getDisplayText } = useTranscriptStreaming(
        segments,
        isRecording,
        enableStreaming
    );

    // Infinite scroll: IntersectionObserver to trigger loading more
    useEffect(() => {
        if (!onLoadMore || !hasMore || isLoadingMore || isRecording || segments.length === 0) {
            return;
        }

        const triggerElement = loadMoreTriggerRef.current;
        if (!triggerElement) return;

        const observer = new IntersectionObserver(
            (entries) => {
                if (entries[0].isIntersecting && hasMore && !isLoadingMore) {
                    onLoadMore();
                }
            },
            {
                root: null,
                rootMargin: '100px',
                threshold: 0,
            }
        );

        observer.observe(triggerElement);

        return () => observer.disconnect();
    }, [hasMore, isLoadingMore, onLoadMore, isRecording, segments.length]);

    // Scroll-based fallback for fast scrolling
    useEffect(() => {
        if (!onLoadMore || !hasMore || isLoadingMore || isRecording) return;

        const scrollElement = scrollRef.current;
        if (!scrollElement) return;

        let ticking = false;

        const handleScroll = () => {
            if (ticking || isLoadingMore || !hasMore) return;

            ticking = true;
            requestAnimationFrame(() => {
                const { scrollTop, scrollHeight, clientHeight } = scrollElement;
                const scrollBottom = scrollHeight - scrollTop - clientHeight;

                // Trigger load when within 200px of bottom
                if (scrollBottom < 200 && hasMore && !isLoadingMore) {
                    onLoadMore();
                }
                ticking = false;
            });
        };

        scrollElement.addEventListener('scroll', handleScroll, { passive: true });
        return () => scrollElement.removeEventListener('scroll', handleScroll);
    }, [onLoadMore, hasMore, isLoadingMore, isRecording]);

    // Use simple rendering for small lists, virtualization for large lists
    const useVirtualization = segments.length >= VIRTUALIZATION_THRESHOLD;

    usePlaybackFollowScroll({
        enabled: playbackFollow,
        activeSegmentId,
        segments,
        scrollRef,
        virtualizer,
        useVirtualization,
    });

    return (
        <div ref={scrollRef} className="flex h-full min-h-0 flex-col overflow-y-auto px-4 py-2">
            {/* Recording Status Bar - Sticky at top, always visible when recording */}
            <AnimatePresence>
                {isRecording && (
                    <div className="sticky top-0 z-10 bg-paper pb-2">
                        <RecordingStatusBar isPaused={isPaused} />
                    </div>
                )}
            </AnimatePresence>

            {/* Content - add padding when recording to prevent overlap */}
            <div className={isRecording ? 'pt-2' : ''}>
            {segments.length === 0 ? (
                // Empty state
                <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    className="text-center text-ink-2 mt-8"
                >
                    {isRecording ? (
                        <>
                            <div className="flex items-center justify-center mb-3">
                                <div className={`w-3 h-3 rounded-full ${isPaused ? 'bg-orange-500' : 'bg-primary animate-pulse'}`}></div>
                            </div>
                            <p className="text-sm text-ink-2">
                                {isPaused ? 'Đã tạm dừng ghi âm' : 'Đang lắng nghe giọng nói...'}
                            </p>
                            <p className="text-xs mt-1 text-ink-2">
                                {isPaused ? 'Nhấn tiếp tục để ghi âm lại' : 'Nói để xem bản ghi trực tiếp'}
                            </p>
                            {pendingSpeakerName && (
                                <p
                                    className="text-sm font-semibold mt-4 pt-2 border-t border-dashed inline-block"
                                    style={{
                                        color: pendingSpeakerColor || '#2563EB',
                                        borderColor: pendingSpeakerColor || '#2563EB',
                                    }}
                                >
                                    {pendingSpeakerName}
                                </p>
                            )}
                        </>
                    ) : (
                        <>
                            <p className="text-lg font-semibold">Chào mừng đến ACT MeetingOne!</p>
                            <p className="text-xs mt-1">Bắt đầu ghi âm để xem bản ghi trực tiếp</p>
                        </>
                    )}
                </motion.div>
            ) : useVirtualization ? (
                // Virtualized rendering for large lists
                <>
                    <div
                        style={{
                            height: virtualizer.getTotalSize(),
                            width: "100%",
                            position: "relative",
                        }}
                    >
                        {virtualizer.getVirtualItems().map((virtualRow) => {
                            const segment = segments[virtualRow.index];
                            const isStreaming = streamingSegmentId === segment.id;

                            return (
                                <div
                                    key={segment.id}
                                    data-index={virtualRow.index}
                                    ref={virtualizer.measureElement}
                                    style={{
                                        position: "absolute",
                                        top: 0,
                                        left: 0,
                                        width: "100%",
                                        transform: `translateY(${virtualRow.start}px)`,
                                    }}
                                >
                                    <TranscriptSegment
                                        id={segment.id}
                                        timestamp={segment.timestamp}
                                        text={getDisplayText(segment)}
                                        fullText={segment.text}
                                        confidence={segment.confidence}
                                        isStreaming={isStreaming}
                                        showConfidence={showConfidence}
                                        showTimestamps={showTimestamps}
                                        sequenceId={segment.sequenceId}
                                        onEdit={onSegmentEdit}
                                        isActive={segment.id === activeSegmentId}
                                        onRowClick={
                                            onSegmentClick ? () => onSegmentClick(segment) : undefined
                                        }
                                        speakerName={segment.speakerName}
                                        speakerColor={segment.speakerColor}
                                        showSpeakerLabel={
                                            !!segment.speakerName &&
                                            segment.speakerName !==
                                                segments[virtualRow.index - 1]?.speakerName
                                        }
                                    />
                                </div>
                            );
                        })}
                    </div>

                    {/* Infinite scroll trigger and loading indicator */}
                    {(hasMore || isLoadingMore) && !isRecording && segments.length > 0 && (
                        <div ref={loadMoreTriggerRef} className="flex justify-center items-center py-4 mt-2">
                            {isLoadingMore ? (
                                <div className="flex items-center gap-2 text-ink-2">
                                    <div className="w-4 h-4 border-2 border-rule border-t-ink rounded-full animate-spin" />
                                    <span className="text-sm">Đang tải thêm...</span>
                                </div>
                            ) : hasMore && totalCount > 0 ? (
                                <span className="text-sm text-ink-2">
                                    Hiển thị {loadedCount} / {totalCount} đoạn
                                </span>
                            ) : null}
                        </div>
                    )}

                    {/* Listening indicator when recording */}
                    {!isStopping && isRecording && !isPaused && !isProcessing && segments.length > 0 && (
                        <motion.div
                            initial={{ opacity: 0 }}
                            animate={{ opacity: 1 }}
                            exit={{ opacity: 0 }}
                            className="flex items-center gap-2 mt-4 text-ink-2"
                        >
                            <div className="w-2 h-2 bg-primary rounded-full animate-pulse"></div>
                            <span className="text-sm">Đang lắng nghe...</span>
                        </motion.div>
                    )}
                    {isRecording && pendingSpeakerName && (
                        <div
                            className="mt-3 pt-2 border-t border-dashed text-sm font-semibold"
                            style={{
                                color: pendingSpeakerColor || '#2563EB',
                                borderColor: pendingSpeakerColor || '#2563EB',
                            }}
                        >
                            {pendingSpeakerName}
                        </div>
                    )}
                </>
            ) : (
                // Simple rendering for small lists (better animations)
                <>
                    <div className="space-y-1">
                        {segments.map((segment, idx) => {
                            const isStreaming = streamingSegmentId === segment.id;

                            return (
                                <motion.div
                                    key={segment.id}
                                    initial={{ opacity: 0, y: 5 }}
                                    animate={{ opacity: 1, y: 0 }}
                                    transition={{ duration: 0.15 }}
                                >
                                    <TranscriptSegment
                                        id={segment.id}
                                        timestamp={segment.timestamp}
                                        text={getDisplayText(segment)}
                                        fullText={segment.text}
                                        confidence={segment.confidence}
                                        isStreaming={isStreaming}
                                        showConfidence={showConfidence}
                                        showTimestamps={showTimestamps}
                                        sequenceId={segment.sequenceId}
                                        onEdit={onSegmentEdit}
                                        isActive={segment.id === activeSegmentId}
                                        onRowClick={
                                            onSegmentClick ? () => onSegmentClick(segment) : undefined
                                        }
                                        speakerName={segment.speakerName}
                                        speakerColor={segment.speakerColor}
                                        showSpeakerLabel={
                                            !!segment.speakerName &&
                                            segment.speakerName !== segments[idx - 1]?.speakerName
                                        }
                                    />
                                </motion.div>
                            );
                        })}
                    </div>

                    {/* Infinite scroll trigger (for small lists that grow) */}
                    {(hasMore || isLoadingMore) && !isRecording && segments.length > 0 && (
                        <div ref={loadMoreTriggerRef} className="flex justify-center items-center py-4 mt-2">
                            {isLoadingMore ? (
                                <div className="flex items-center gap-2 text-ink-2">
                                    <div className="w-4 h-4 border-2 border-rule border-t-ink rounded-full animate-spin" />
                                    <span className="text-sm">Đang tải thêm...</span>
                                </div>
                            ) : hasMore && totalCount > 0 ? (
                                <span className="text-sm text-ink-2">
                                    Hiển thị {loadedCount} / {totalCount} đoạn
                                </span>
                            ) : null}
                        </div>
                    )}

                    {/* Listening indicator when recording */}
                    {!isStopping && isRecording && !isPaused && !isProcessing && segments.length > 0 && (
                        <motion.div
                            initial={{ opacity: 0 }}
                            animate={{ opacity: 1 }}
                            exit={{ opacity: 0 }}
                            className="flex items-center gap-2 mt-4 text-ink-2"
                        >
                            <div className="w-2 h-2 bg-primary rounded-full animate-pulse"></div>
                            <span className="text-sm">Đang lắng nghe...</span>
                        </motion.div>
                    )}
                    {isRecording && pendingSpeakerName && (
                        <div
                            className="mt-3 pt-2 border-t border-dashed text-sm font-semibold"
                            style={{
                                color: pendingSpeakerColor || '#2563EB',
                                borderColor: pendingSpeakerColor || '#2563EB',
                            }}
                        >
                            {pendingSpeakerName}
                        </div>
                    )}
                </>
            )}
            </div>
        </div>
    );
};
