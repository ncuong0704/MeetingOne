'use client';

import { useEffect, useRef, useState } from 'react';
import { ChevronDown, Play, X } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { SpeakerNameCombobox } from '@/components/SpeakerNameCombobox';
import { cn } from '@/lib/utils';
import type { MeetingSpeaker } from '@/lib/asr';
import { mergeTargets } from '@/lib/speakerPreview';
import {
  directoryFilterQuery,
  formatDirectorySpeakerLabel,
  getSpeakerDirectory,
  type DirectorySpeaker,
} from '@/lib/speakerDirectory';

export type SpeakerListRow = {
  id: string;
  displayName: string;
  color: string;
  previewStart: number | null;
};

export function speakersFromApi(rows: MeetingSpeaker[]): SpeakerListRow[] {
  return rows.map((row) => ({
    id: row.id,
    displayName: row.display_name,
    color: row.color,
    previewStart: row.preview_start,
  }));
}

function MergeTargetCombobox({
  targets,
  value,
  onChange,
  disabled,
  onOpenChange,
}: {
  targets: SpeakerListRow[];
  value: string;
  onChange: (id: string) => void;
  disabled?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const selected = targets.find((target) => target.id === value);
  const onOpenChangeRef = useRef(onOpenChange);
  onOpenChangeRef.current = onOpenChange;

  useEffect(() => {
    onOpenChangeRef.current?.(open);
  }, [open]);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('pointerdown', onPointerDown);
    return () => document.removeEventListener('pointerdown', onPointerDown);
  }, []);

  return (
    <div
      ref={rootRef}
      className={cn('relative min-w-0', open && 'z-[calc(var(--z-modal)+20)]')}
    >
      <button
        type="button"
        disabled={disabled}
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-label="Gộp vào người nói"
        onClick={() => setOpen((prev) => !prev)}
        className={cn(
          'flex h-8 w-full min-w-0 items-center gap-2 rounded-md border bg-paper px-2.5 pr-8 text-left text-sm text-ink',
          'transition-[border-color,background-color] duration-[var(--dur-micro)] ease-[var(--ease-out)]',
          'focus:outline-none focus-visible:ring-1 focus-visible:ring-ring',
          'disabled:cursor-not-allowed disabled:opacity-50',
          open ? 'border-primary/40 bg-paper-2' : 'border-rule',
        )}
      >
        {selected ? (
          <span
            className="h-2 w-2 shrink-0 rounded-full"
            style={{ backgroundColor: selected.color }}
            aria-hidden
          />
        ) : null}
        <span className="min-w-0 flex-1 truncate">
          {selected?.displayName || 'Chọn người nói'}
        </span>
      </button>
      <ChevronDown
        aria-hidden
        className={cn(
          'pointer-events-none absolute right-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-ink-2',
          'transition-transform duration-[var(--dur-micro)] ease-[var(--ease-out)]',
          open && 'rotate-180',
        )}
      />
      {open && (
        <ul
          role="listbox"
          data-speaker-suggestions=""
          className="absolute left-0 right-0 top-[calc(100%+4px)] z-[calc(var(--z-modal)+20)] max-h-52 overflow-y-auto overscroll-contain rounded-md border border-rule bg-paper py-1 shadow-[var(--shadow-whisper)]"
        >
          {targets.map((target) => {
            const active = target.id === value;
            return (
              <li key={target.id} role="option" aria-selected={active}>
                <button
                  type="button"
                  onPointerDown={(event) => {
                    event.preventDefault();
                    onChange(target.id);
                    setOpen(false);
                  }}
                  className={cn(
                    'flex w-full min-w-0 items-center gap-2 border-l-2 px-2.5 py-1.5 text-left',
                    'transition-colors duration-[var(--dur-micro)] ease-[var(--ease-out)]',
                    active ? 'border-primary bg-primary/10' : 'border-transparent hover:bg-paper-2',
                  )}
                >
                  <span
                    className="h-2 w-2 shrink-0 rounded-full"
                    style={{ backgroundColor: target.color }}
                    aria-hidden
                  />
                  <span className="min-w-0 truncate text-sm text-ink">{target.displayName}</span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

interface SpeakerListDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  speakers: SpeakerListRow[];
  canPlay: boolean;
  onRename: (speakerId: string, displayName: string) => Promise<void>;
  onPreview: (start: number) => void;
  onMerge: (sourceId: string, targetId: string) => Promise<void>;
}

export function SpeakerListDialog({
  open,
  onOpenChange,
  speakers,
  canPlay,
  onRename,
  onPreview,
  onMerge,
}: SpeakerListDialogProps) {
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [people, setPeople] = useState<DirectorySpeaker[]>([]);
  const [mergingFrom, setMergingFrom] = useState<string | null>(null);
  const [mergeTarget, setMergeTarget] = useState('');
  const [busyId, setBusyId] = useState<string | null>(null);
  const [openNameId, setOpenNameId] = useState<string | null>(null);
  const [mergeListOpen, setMergeListOpen] = useState(false);

  useEffect(() => {
    if (!open) {
      setMergingFrom(null);
      setMergeTarget('');
      setOpenNameId(null);
      setMergeListOpen(false);
      return;
    }
    const next: Record<string, string> = {};
    for (const speaker of speakers) {
      next[speaker.id] = speaker.displayName;
    }
    setDrafts(next);
  }, [open, speakers]);

  useEffect(() => {
    if (!open) return;
    getSpeakerDirectory()
      .then(setPeople)
      .catch(() => setPeople([]));
  }, [open]);

  const saveRename = async (speaker: SpeakerListRow, raw?: string) => {
    const trimmed = (raw ?? drafts[speaker.id] ?? '').trim();
    if (!trimmed) {
      setDrafts((prev) => ({ ...prev, [speaker.id]: speaker.displayName }));
      return;
    }
    if (trimmed === speaker.displayName) return;
    setBusyId(speaker.id);
    try {
      await onRename(speaker.id, trimmed);
    } finally {
      setBusyId(null);
    }
  };

  const confirmMerge = async (sourceId: string) => {
    if (!mergeTarget || mergeTarget === sourceId) return;
    setBusyId(sourceId);
    try {
      await onMerge(sourceId, mergeTarget);
      setMergingFrom(null);
      setMergeTarget('');
      setMergeListOpen(false);
    } finally {
      setBusyId(null);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        // Ignore Radix dismiss from combobox/merge remounts. Close only via X or Escape.
        if (next) onOpenChange(true);
      }}
    >
      <DialogContent
        className="w-[min(32rem,calc(100vw-2rem))] gap-0 overflow-visible p-0 sm:max-w-lg [&>button:last-child]:hidden"
        onCloseAutoFocus={(event) => event.preventDefault()}
        onOpenAutoFocus={(event) => event.preventDefault()}
        onPointerDownOutside={(event) => event.preventDefault()}
        onFocusOutside={(event) => event.preventDefault()}
        onInteractOutside={(event) => event.preventDefault()}
        onEscapeKeyDown={(event) => {
          event.preventDefault();
          onOpenChange(false);
        }}
      >
        <button
          type="button"
          onClick={() => onOpenChange(false)}
          className="absolute right-4 top-4 z-[calc(var(--z-modal)+30)] rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
          aria-label="Đóng"
        >
          <X className="h-4 w-4" />
          <span className="sr-only">Đóng</span>
        </button>
        <DialogHeader className="space-y-0 px-5 pb-4 pt-5 pr-12 text-left">
          <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-ink-2">
            Bản ghi
          </p>
          <DialogTitle className="mt-1 text-base font-semibold tracking-[-0.02em] text-ink">
            Người nói
          </DialogTitle>
          <DialogDescription className="mt-1.5 text-xs text-ink-2">
            Đổi tên, nghe đoạn đầu (~15 giây) hoặc gộp hai cụm bị tách nhầm thành một người.
          </DialogDescription>
        </DialogHeader>

        <div className="min-w-0 px-5 pb-5">
          {speakers.length === 0 ? (
            <p className="rounded-md border border-rule px-3 py-8 text-center text-xs text-ink-2">
              Chưa phát hiện người nói.
            </p>
          ) : (
            <ul className="isolate min-w-0 overflow-visible rounded-md border border-rule">
              {speakers.map((speaker) => {
                const targets = mergeTargets(speakers, speaker.id);
                const merging = mergingFrom === speaker.id;
                const draft = drafts[speaker.id] ?? speaker.displayName;
                return (
                  <li
                    key={speaker.id}
                    className={cn(
                      'relative z-0 min-w-0 space-y-2 overflow-visible border-b border-rule px-2.5 py-2 last:border-b-0',
                      (openNameId === speaker.id || (merging && mergeListOpen)) &&
                        'z-[calc(var(--z-modal)+20)]',
                    )}
                  >
                    <div
                      className={cn(
                        'relative z-20 flex min-w-0 items-center gap-2',
                        openNameId === speaker.id && 'z-[calc(var(--z-modal)+20)]',
                      )}
                    >
                      <span
                        className="h-2.5 w-2.5 shrink-0 rounded-full"
                        style={{ backgroundColor: speaker.color }}
                        aria-hidden
                      />
                      <div className="min-w-0 flex-1">
                        <SpeakerNameCombobox
                          value={draft}
                          people={people}
                          disabled={busyId === speaker.id}
                          placeholder="Gõ tên hoặc chọn từ danh sách"
                          filterQuery={directoryFilterQuery(draft, speaker.displayName, people)}
                          onChange={(next) =>
                            setDrafts((prev) => ({ ...prev, [speaker.id]: next }))
                          }
                          onCommit={() => void saveRename(speaker)}
                          onSelectPerson={(person) =>
                            void saveRename(speaker, formatDirectorySpeakerLabel(person))
                          }
                          onOpenChange={(nameOpen) =>
                            setOpenNameId(nameOpen ? speaker.id : (current) =>
                              current === speaker.id ? null : current,
                            )
                          }
                        />
                      </div>
                      <div className="flex shrink-0 items-center gap-1">
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          className="h-8 w-8 px-0"
                          title="Phát đoạn nhận dạng (~15 giây)"
                          disabled={
                            !canPlay || speaker.previewStart == null || busyId === speaker.id
                          }
                          onClick={() => {
                            if (speaker.previewStart != null) onPreview(speaker.previewStart);
                          }}
                        >
                          <Play className="h-3.5 w-3.5" fill="currentColor" />
                        </Button>
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          className="h-8 px-2.5 text-xs"
                          disabled={targets.length === 0 || busyId === speaker.id}
                          onClick={() => {
                            setOpenNameId(null);
                            setMergeListOpen(false);
                            setMergingFrom(merging ? null : speaker.id);
                            setMergeTarget(targets[0]?.id ?? '');
                          }}
                        >
                          Gộp
                        </Button>
                      </div>
                    </div>
                    {merging && targets.length > 0 && (
                      <div
                        className={cn(
                          'relative z-10 grid min-w-0 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-2 pl-[18px]',
                          mergeListOpen && 'z-[calc(var(--z-modal)+20)]',
                        )}
                      >
                        <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-ink-2">
                          Vào
                        </span>
                        <MergeTargetCombobox
                          targets={targets}
                          value={mergeTarget}
                          disabled={busyId === speaker.id}
                          onChange={setMergeTarget}
                          onOpenChange={setMergeListOpen}
                        />
                        <Button
                          type="button"
                          size="sm"
                          className="h-8 shrink-0 px-3 text-xs"
                          disabled={!mergeTarget}
                          onClick={() => void confirmMerge(speaker.id)}
                        >
                          Xác nhận
                        </Button>
                      </div>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
