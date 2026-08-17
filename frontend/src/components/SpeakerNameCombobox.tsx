'use client';

import { useEffect, useId, useMemo, useRef, useState } from 'react';
import { ChevronDown } from 'lucide-react';
import { cn } from '@/lib/utils';
import { formatDirectorySpeakerLabel, suggestDirectorySpeakers, type DirectorySpeaker } from '@/lib/speakerDirectory';

interface SpeakerNameComboboxProps {
  value: string;
  onChange: (value: string) => void;
  people: DirectorySpeaker[];
  placeholder?: string;
  disabled?: boolean;
  /** Filter list with this instead of `value` (e.g. empty to show all people). */
  filterQuery?: string;
  /** Enter when not picking a suggestion. */
  onCommit?: () => void;
  /** After choosing a directory row. */
  onSelectPerson?: (person: DirectorySpeaker) => void;
  onOpenChange?: (open: boolean) => void;
}

function initialsFromName(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return `${parts[0][0]}${parts[parts.length - 1][0]}`.toUpperCase();
}

export function SpeakerNameCombobox({
  value,
  onChange,
  people,
  placeholder,
  disabled = false,
  filterQuery,
  onCommit,
  onSelectPerson,
  onOpenChange,
}: SpeakerNameComboboxProps) {
  const listId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const highlightRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState(0);
  const onOpenChangeRef = useRef(onOpenChange);
  onOpenChangeRef.current = onOpenChange;

  const suggestions = useMemo(
    () => suggestDirectorySpeakers(filterQuery ?? value, people, 8),
    [filterQuery, people, value],
  );

  useEffect(() => {
    onOpenChangeRef.current?.(open);
  }, [open]);

  useEffect(() => {
    setHighlight(0);
  }, [value, open]);

  useEffect(() => {
    highlightRef.current?.scrollIntoView({ block: 'nearest' });
  }, [highlight]);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('pointerdown', onPointerDown);
    return () => document.removeEventListener('pointerdown', onPointerDown);
  }, []);

  const pick = (person: DirectorySpeaker) => {
    const label = formatDirectorySpeakerLabel(person);
    onChange(label);
    setOpen(false);
    onSelectPerson?.(person);
  };

  return (
    <div
      ref={rootRef}
      className={cn('relative', open && 'z-[calc(var(--z-modal)+20)]')}
    >
      <input
        type="text"
        value={value}
        disabled={disabled}
        placeholder={placeholder}
        autoComplete="off"
        aria-autocomplete="list"
        aria-expanded={open}
        aria-controls={listId}
        onFocus={() => setOpen(true)}
        onChange={(event) => {
          onChange(event.target.value);
          setOpen(true);
        }}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            setOpen(false);
            return;
          }
          if (event.key === 'ArrowDown') {
            if (!open) {
              setOpen(true);
              return;
            }
            if (suggestions.length === 0) return;
            event.preventDefault();
            setHighlight((i) => (i + 1) % suggestions.length);
            return;
          }
          if (event.key === 'ArrowUp') {
            if (!open || suggestions.length === 0) return;
            event.preventDefault();
            setHighlight((i) => (i - 1 + suggestions.length) % suggestions.length);
            return;
          }
          if (event.key === 'Enter') {
            if (open && suggestions.length > 0) {
              event.preventDefault();
              pick(suggestions[highlight]);
              return;
            }
            if (onCommit) {
              event.preventDefault();
              onCommit();
              return;
            }
            if (!open) setOpen(true);
          }
        }}
        className={cn(
          'h-8 w-full rounded-md border bg-paper px-2.5 pr-8 text-sm text-ink',
          'placeholder:text-ink-2/70',
          'transition-[border-color,background-color] duration-[var(--dur-micro)] ease-[var(--ease-out)]',
          'focus:outline-none focus-visible:ring-1 focus-visible:ring-ring',
          'disabled:cursor-not-allowed disabled:opacity-50',
          open ? 'border-primary/40 bg-paper-2' : 'border-rule',
        )}
      />
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
          id={listId}
          role="listbox"
          data-speaker-suggestions=""
          className="absolute left-0 right-0 top-[calc(100%+4px)] z-[calc(var(--z-modal)+20)] max-h-52 overflow-y-auto overscroll-contain rounded-md border border-rule bg-paper py-1 shadow-[var(--shadow-whisper)]"
        >
          {suggestions.length === 0 ? (
            <li className="px-3 py-3 text-center text-xs leading-relaxed text-ink-2">
              {people.length === 0
                ? 'Chưa có danh sách. Thêm trong Cài đặt → Danh sách.'
                : 'Không khớp. Có thể gõ tên tự do.'}
            </li>
          ) : (
            suggestions.map((person, index) => {
              const meta = [person.title, person.department].filter(Boolean).join(' · ');
              const active = index === highlight;
              return (
                <li key={person.id} role="option" aria-selected={active}>
                  <button
                    type="button"
                    ref={active ? highlightRef : undefined}
                    onPointerDown={(event) => {
                      event.preventDefault();
                      pick(person);
                    }}
                    className={cn(
                      'flex w-full items-center gap-2.5 border-l-2 px-2.5 py-1.5 text-left',
                      'transition-colors duration-[var(--dur-micro)] ease-[var(--ease-out)]',
                      active
                        ? 'border-primary bg-primary/10'
                        : 'border-transparent hover:bg-paper-2',
                    )}
                  >
                    <span
                      aria-hidden
                      className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-primary/10 font-mono text-[10px] font-medium text-primary"
                    >
                      {initialsFromName(person.fullName)}
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm font-medium text-ink">
                        {person.fullName}
                      </span>
                      {meta ? (
                        <span className="mt-0.5 block truncate text-[11px] leading-snug text-ink-2">
                          {meta}
                        </span>
                      ) : null}
                    </span>
                  </button>
                </li>
              );
            })
          )}
        </ul>
      )}
    </div>
  );
}
