'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { TranscriptSegmentData } from '@/types';
import { Popover, PopoverAnchor, PopoverContent } from './ui/popover';
import { usePlaybackFollowScroll } from '@/hooks/usePlaybackFollowScroll';

export interface FlowingTranscriptViewProps {
    segments: TranscriptSegmentData[];
    onSegmentEdit?: (segmentId: string, newText: string, sequenceId?: number) => Promise<void>;
    activeSegmentId?: string | null;
    onSegmentClick?: (segment: TranscriptSegmentData) => void;
    playbackFollow?: boolean;
    hasMore?: boolean;
    isLoadingMore?: boolean;
    totalCount?: number;
    loadedCount?: number;
    onLoadMore?: () => void;
}

// Remove filler words and repetitions (same rule as VirtualizedTranscriptView).
function cleanStopWords(text: string): string {
    const stopWords = ['uh', 'um', 'er', 'ah', 'hmm', 'hm', 'eh', 'oh'];
    let cleanedText = text;
    stopWords.forEach(word => {
        const pattern = new RegExp(`\\b${word}\\b[,\\s]*`, 'gi');
        cleanedText = cleanedText.replace(pattern, ' ');
    });
    return cleanedText.replace(/\s+/g, ' ').trim();
}

// A sentence ending in '.', '?', or '!' (optionally followed by a closing quote)
// starts the next segment on a new line — mirrors normal paragraph flow.
function endsSentence(text: string): boolean {
    return /[.?!][)"'”]?\s*$/.test(text.trim());
}

function FlowingSegment({
    segment,
    isActive,
    isLastInParagraph,
    onEdit,
    onSeek,
}: {
    segment: TranscriptSegmentData;
    isActive: boolean;
    isLastInParagraph: boolean;
    onEdit?: (segmentId: string, newText: string, sequenceId?: number) => Promise<void>;
    onSeek?: () => void;
}) {
    const [isEditing, setIsEditing] = useState(false);
    const [editValue, setEditValue] = useState(segment.text);
    const [isSaving, setIsSaving] = useState(false);
    const [optimisticText, setOptimisticText] = useState<string | null>(null);
    const textareaRef = useRef<HTMLTextAreaElement>(null);
    const clickTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

    const displayText = cleanStopWords(optimisticText ?? segment.text);
    const shownText = displayText || (segment.text.trim() === '' ? '[Im lặng]' : displayText);

    const openEdit = useCallback(() => {
        setEditValue(optimisticText ?? segment.text);
        setIsEditing(true);
    }, [optimisticText, segment.text]);

    const handleSave = useCallback(async () => {
        const trimmed = editValue.trim();
        if (!trimmed || trimmed === (optimisticText ?? segment.text)) {
            setIsEditing(false);
            return;
        }
        setIsSaving(true);
        setOptimisticText(trimmed);
        setIsEditing(false);
        try {
            await onEdit?.(segment.id, trimmed, segment.sequenceId);
        } catch {
            setOptimisticText(null);
        } finally {
            setIsSaving(false);
        }
    }, [editValue, optimisticText, segment.id, segment.text, segment.sequenceId, onEdit]);

    const handleCancel = useCallback(() => {
        setEditValue(optimisticText ?? segment.text);
        setIsEditing(false);
    }, [optimisticText, segment.text]);

    const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
        if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            handleSave();
        }
        if (e.key === 'Escape') handleCancel();
    };

    // Single click seeks audio; double click edits. The single-click handler is
    // delayed so a second click within the window cancels it instead of both firing.
    const handleClick = () => {
        if (!onSeek) return;
        if (clickTimerRef.current) clearTimeout(clickTimerRef.current);
        clickTimerRef.current = setTimeout(() => {
            onSeek();
            clickTimerRef.current = null;
        }, 220);
    };

    const handleDoubleClick = () => {
        if (clickTimerRef.current) {
            clearTimeout(clickTimerRef.current);
            clickTimerRef.current = null;
        }
        if (onEdit) openEdit();
    };

    return (
        <Popover open={isEditing} onOpenChange={(open) => !open && handleCancel()}>
            <PopoverAnchor asChild>
                <span
                    id={`segment-${segment.id}`}
                    onClick={handleClick}
                    onDoubleClick={handleDoubleClick}
                    className={`rounded transition-colors ${onSeek || onEdit ? 'cursor-pointer' : ''} ${
                        isActive
                            ? 'bg-[rgba(255,215,0,0.35)]'
                            : 'hover:bg-gray-100'
                    } ${isSaving ? 'text-gray-400' : ''}`}
                >
                    {shownText}
                    {isSaving && <span className="ml-1 text-xs text-gray-400">Đang lưu...</span>}
                    {' '}
                    {isLastInParagraph && <br />}
                </span>
            </PopoverAnchor>
            <PopoverContent className="w-96" onOpenAutoFocus={(e) => {
                e.preventDefault();
                textareaRef.current?.focus();
                textareaRef.current?.select();
            }}>
                <textarea
                    ref={textareaRef}
                    value={editValue}
                    onChange={(e) => setEditValue(e.target.value)}
                    onKeyDown={handleKeyDown}
                    className="w-full resize-none rounded-md border border-[rgba(22,71,142,0.3)] bg-[rgba(22,71,142,0.03)] px-3 py-2 text-sm text-gray-800 leading-relaxed focus:outline-none focus:ring-2 focus:ring-[rgba(22,71,142,0.2)]"
                    rows={3}
                />
                <div className="mt-2 flex items-center justify-end gap-1">
                    <span className="mr-auto text-[10px] text-gray-400">Enter để lưu · Esc để huỷ</span>
                    <button
                        onClick={handleCancel}
                        className="rounded-md px-2 py-1 text-xs text-gray-500 hover:bg-gray-100 hover:text-gray-700"
                    >
                        Huỷ
                    </button>
                    <button
                        onClick={handleSave}
                        className="rounded-md bg-[#16478e] px-2 py-1 text-xs text-white hover:bg-[#1a55ab]"
                    >
                        Lưu
                    </button>
                </div>
            </PopoverContent>
        </Popover>
    );
}

/** Flowing, continuous-paragraph transcript rendering for post-hoc meeting/file-import
 * review — segments join inline (space-separated, no per-segment box/border/margin), with
 * a line break only after a segment that ends a sentence. Matches the reference app's
 * plain-paragraph layout. Editing moves to a double-click-triggered popover instead of an
 * always-visible block, so it doesn't break the flow. Not virtualized — inline text flow
 * is incompatible with item-based virtualization (segments can share a visual line), and
 * imported-file transcripts are small enough (tens to low hundreds of segments) that this
 * is fine. */
export function FlowingTranscriptView({
    segments,
    onSegmentEdit,
    activeSegmentId = null,
    onSegmentClick,
    playbackFollow = false,
    hasMore = false,
    isLoadingMore = false,
    totalCount = 0,
    loadedCount = 0,
    onLoadMore,
}: FlowingTranscriptViewProps) {
    const scrollRef = useRef<HTMLDivElement>(null);
    const loadMoreTriggerRef = useRef<HTMLDivElement>(null);

    usePlaybackFollowScroll({
        enabled: playbackFollow,
        activeSegmentId,
        segments,
        scrollRef,
        useVirtualization: false,
    });

    // Infinite scroll: load more segments as the user scrolls near the bottom.
    useEffect(() => {
        if (!onLoadMore || !hasMore || isLoadingMore || segments.length === 0) return;
        const triggerElement = loadMoreTriggerRef.current;
        if (!triggerElement) return;

        const observer = new IntersectionObserver(
            (entries) => {
                if (entries[0].isIntersecting && hasMore && !isLoadingMore) onLoadMore();
            },
            { root: null, rootMargin: '100px', threshold: 0 },
        );
        observer.observe(triggerElement);
        return () => observer.disconnect();
    }, [hasMore, isLoadingMore, onLoadMore, segments.length]);

    if (segments.length === 0) {
        return (
            <div className="mt-8 text-center text-gray-500">
                <p className="text-lg font-semibold">Chào mừng đến ACT MeetingOne!</p>
            </div>
        );
    }

    return (
        <div ref={scrollRef} className="h-full overflow-y-auto px-4 py-3">
            <p className="text-base leading-relaxed text-gray-800">
                {segments.map((segment, i) => (
                    <FlowingSegment
                        key={segment.id}
                        segment={segment}
                        isActive={segment.id === activeSegmentId}
                        isLastInParagraph={i < segments.length - 1 && endsSentence(segment.text)}
                        onEdit={onSegmentEdit}
                        onSeek={onSegmentClick ? () => onSegmentClick(segment) : undefined}
                    />
                ))}
            </p>

            {(hasMore || isLoadingMore) && (
                <div ref={loadMoreTriggerRef} className="flex items-center justify-center py-4">
                    {isLoadingMore ? (
                        <div className="flex items-center gap-2 text-gray-500">
                            <div className="h-4 w-4 animate-spin rounded-full border-2 border-gray-300 border-t-gray-600" />
                            <span className="text-sm">Đang tải thêm...</span>
                        </div>
                    ) : hasMore && totalCount > 0 ? (
                        <span className="text-sm text-gray-400">
                            Hiển thị {loadedCount} / {totalCount} đoạn
                        </span>
                    ) : null}
                </div>
            )}
        </div>
    );
}
