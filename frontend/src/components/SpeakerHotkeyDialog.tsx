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
        className="max-w-md"
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
        <DialogHeader>
          <DialogTitle>Cấu hình phím tắt người nói</DialogTitle>
          <DialogDescription>
            Trong khi ghi âm trực tiếp, bấm phím số tương ứng để gán người đang nói. Để trống = ẩn phím.
          </DialogDescription>
        </DialogHeader>
        <div className="grid grid-cols-[2.5rem_3.5rem_1fr] gap-2 items-center text-xs font-medium text-ink-2">
          <span>STT</span>
          <span>Phím</span>
          <span>Tên người nói</span>
        </div>
        <div className="space-y-2 max-h-[50vh] overflow-y-auto pr-1">
          {Array.from({ length: 9 }, (_, i) => String(i + 1)).map((key) => (
            <div key={key} className="grid grid-cols-[2.5rem_3.5rem_1fr] gap-2 items-center">
              <span className="text-center text-sm text-ink-2">{key}</span>
              <span className="text-center text-sm font-semibold text-primary">Num {key}</span>
              <SpeakerNameCombobox
                value={slots[key] ?? ''}
                placeholder={`Nhập tên cho phím ${key}...`}
                people={people}
                onChange={(next) =>
                  setSlots((prev) => ({ ...prev, [key]: next }))
                }
              />
            </div>
          ))}
        </div>
        {error && <p className="text-xs text-red-500">{error}</p>}
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={saving}>
            Hủy
          </Button>
          <Button onClick={handleSave} disabled={saving}>
            {saving ? 'Đang lưu...' : 'Lưu'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
