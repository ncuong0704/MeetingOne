'use client';

import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
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
}: SpeakerNameComboboxProps) {
  const listId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState(0);
  const [menuBox, setMenuBox] = useState<{ top: number; left: number; width: number } | null>(null);

  const suggestions = useMemo(
    () => suggestDirectorySpeakers(filterQuery ?? value, people, 8),
    [filterQuery, people, value],
  );

  const updateMenuBox = () => {
    const rect = inputRef.current?.getBoundingClientRect();
    if (!rect) return;
    setMenuBox({ top: rect.bottom + 4, left: rect.left, width: rect.width });
  };

  useLayoutEffect(() => {
    if (!open) return;
    updateMenuBox();
    window.addEventListener('resize', updateMenuBox);
    window.addEventListener('scroll', updateMenuBox, true);
    return () => {
      window.removeEventListener('resize', updateMenuBox);
      window.removeEventListener('scroll', updateMenuBox, true);
    };
  }, [open, value]);

  useEffect(() => {
    setHighlight(0);
  }, [value, open]);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        const menu = document.getElementById(listId);
        if (menu?.contains(event.target as Node)) return;
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
    <div ref={rootRef} className="relative">
      <input
        ref={inputRef}
        type="text"
        value={value}
        disabled={disabled}
        placeholder={placeholder}
        autoComplete="off"
        aria-autocomplete="list"
        aria-expanded={open}
        aria-controls={listId}
        onFocus={() => {
          setOpen(true);
          requestAnimationFrame(updateMenuBox);
        }}
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
        className="w-full px-3 py-1.5 text-sm rounded-md border border-gray-300 bg-white text-gray-900 focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
      />
      {open && menuBox && createPortal(
        <ul
          id={listId}
          role="listbox"
          style={{ top: menuBox.top, left: menuBox.left, width: menuBox.width }}
          data-speaker-suggestions=""
          className="pointer-events-auto fixed z-[100] max-h-48 overflow-y-auto rounded-md border border-gray-200 bg-white py-1 shadow-lg"
        >
          {suggestions.length === 0 ? (
            <li className="px-3 py-2 text-xs text-gray-400">
              {people.length === 0
                ? 'Chưa có danh sách. Thêm trong Cài đặt → Danh sách.'
                : 'Không khớp. Có thể gõ tên tự do.'}
            </li>
          ) : (
            suggestions.map((person, index) => {
              const meta = [person.title, person.department].filter(Boolean).join(' · ');
              return (
                <li key={person.id} role="option" aria-selected={index === highlight}>
                  <button
                    type="button"
                    onPointerDown={(event) => {
                      event.preventDefault();
                      pick(person);
                    }}
                    className={`w-full px-3 py-1.5 text-left ${
                      index === highlight ? 'bg-blue-50' : 'hover:bg-gray-50'
                    }`}
                  >
                    <span className="block text-sm text-gray-900">{person.fullName}</span>
                    {meta ? (
                      <span className="block text-[11px] text-gray-500">{meta}</span>
                    ) : null}
                  </button>
                </li>
              );
            })
          )}
        </ul>,
        document.body,
      )}
    </div>
  );
}
