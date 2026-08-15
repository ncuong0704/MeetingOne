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

interface SpeakerHotkeyDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function SpeakerHotkeyDialog({ open, onOpenChange }: SpeakerHotkeyDialogProps) {
  const [slots, setSlots] = useState<SpeakerHotkeys>(emptySpeakerHotkeys);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setError(null);
    getSpeakerHotkeys()
      .then(setSlots)
      .catch((e) => setError(String(e)));
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
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>Cấu hình phím tắt người nói</DialogTitle>
          <DialogDescription>
            Trong khi ghi âm trực tiếp, bấm phím số tương ứng để gán người đang nói. Để trống = ẩn phím.
          </DialogDescription>
        </DialogHeader>
        <div className="grid grid-cols-[2.5rem_3.5rem_1fr] gap-2 items-center text-xs font-medium text-gray-500">
          <span>STT</span>
          <span>Phím</span>
          <span>Tên người nói</span>
        </div>
        <div className="space-y-2 max-h-[50vh] overflow-y-auto pr-1">
          {Array.from({ length: 9 }, (_, i) => String(i + 1)).map((key) => (
            <div key={key} className="grid grid-cols-[2.5rem_3.5rem_1fr] gap-2 items-center">
              <span className="text-center text-sm text-gray-600">{key}</span>
              <span className="text-center text-sm font-semibold text-blue-600">Num {key}</span>
              <input
                type="text"
                value={slots[key] ?? ''}
                placeholder={`Nhập tên cho phím ${key}...`}
                onChange={(e) =>
                  setSlots((prev) => ({ ...prev, [key]: e.target.value }))
                }
                className="w-full px-3 py-1.5 text-sm rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500"
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
