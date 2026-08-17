'use client';

import { useEffect, useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import {
  emptySpeakerHotkeys,
  getSpeakerHotkeys,
  saveSpeakerHotkeys,
  SpeakerHotkeys,
} from '@/lib/speakerHotkeys';
import { getSpeakerDirectory, type DirectorySpeaker } from '@/lib/speakerDirectory';
import { SpeakerNameCombobox } from '@/components/SpeakerNameCombobox';

interface SpeakerHotkeyDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function SpeakerHotkeyDialog({ open, onOpenChange }: SpeakerHotkeyDialogProps) {
  const [slots, setSlots] = useState<SpeakerHotkeys>(emptySpeakerHotkeys);
  const [people, setPeople] = useState<DirectorySpeaker[]>([]);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setError(null);
    getSpeakerHotkeys()
      .then(setSlots)
      .catch((e) => setError(String(e)));
    getSpeakerDirectory()
      .then(setPeople)
      .catch(() => setPeople([]));
  }, [open]);

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      const saved = await saveSpeakerHotkeys(slots);
      setSlots(saved);
      onOpenChange(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="gap-0 overflow-visible p-0 sm:max-w-md"
        onPointerDownOutside={(event) => {
          if ((event.target as HTMLElement | null)?.closest('[data-speaker-suggestions]')) {
            event.preventDefault();
          }
        }}
        onFocusOutside={(event) => {
          if ((event.target as HTMLElement | null)?.closest('[data-speaker-suggestions]')) {
            event.preventDefault();
          }
        }}
        onInteractOutside={(event) => {
          if ((event.target as HTMLElement | null)?.closest('[data-speaker-suggestions]')) {
            event.preventDefault();
          }
        }}
      >
        <DialogHeader className="space-y-0 px-5 pb-4 pt-5 pr-12 text-left">
          <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-ink-2">
            Ghi trực tiếp
          </p>
          <DialogTitle className="mt-1 text-base font-semibold tracking-[-0.02em] text-ink">
            Cấu hình phím tắt người nói
          </DialogTitle>
          <DialogDescription className="mt-1.5 text-xs text-ink-2">
            Bấm phím số 1–9 khi đang ghi để gán người đang nói. Ô trống thì ẩn phím đó.
          </DialogDescription>
        </DialogHeader>

        <div className="px-5 pb-4">
          <div className="rounded-md border border-rule">
            {Array.from({ length: 9 }, (_, i) => String(i + 1)).map((key) => {
              const filled = Boolean((slots[key] ?? '').trim());
              return (
                <div
                  key={key}
                  className="flex items-center gap-2.5 border-b border-rule px-2.5 py-1.5 last:border-b-0"
                >
                  <span
                    className={cn(
                      'inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md font-mono text-sm font-medium',
                      filled
                        ? 'bg-primary text-primary-foreground'
                        : 'border border-rule bg-paper text-ink-2'
                    )}
                  >
                    {key}
                  </span>
                  <div className="min-w-0 flex-1">
                    <SpeakerNameCombobox
                      value={slots[key] ?? ''}
                      placeholder="Tên người nói"
                      people={people}
                      onChange={(next) =>
                        setSlots((prev) => ({ ...prev, [key]: next }))
                      }
                    />
                  </div>
                </div>
              );
            })}
          </div>
          {error && <p className="mt-2 text-xs text-destructive">{error}</p>}
        </div>

        <DialogFooter className="border-t border-rule bg-paper px-5 py-3">
          <Button
            variant="outline"
            size="sm"
            onClick={() => onOpenChange(false)}
            disabled={saving}
          >
            Hủy
          </Button>
          <Button size="sm" onClick={handleSave} disabled={saving}>
            {saving ? 'Đang lưu...' : 'Lưu'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
