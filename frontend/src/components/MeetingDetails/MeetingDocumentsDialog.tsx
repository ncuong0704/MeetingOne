'use client';

import React, { useEffect, useRef } from 'react';
import { Loader2, Trash2 } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Button } from '../ui/button';
import { toast } from 'sonner';
import { useMeetingDocuments } from '@/hooks/useMeetingDocuments';

interface MeetingDocumentsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  meetingId: string;
}

export function MeetingDocumentsDialog({
  open,
  onOpenChange,
  meetingId,
}: MeetingDocumentsDialogProps) {
  const { documents, status, error, isBusy, refetch, selectAndAttach, remove } =
    useMeetingDocuments();
  const prevOpenRef = useRef(false);

  useEffect(() => {
    const wasOpen = prevOpenRef.current;
    prevOpenRef.current = open;

    if (open && !wasOpen) {
      refetch(meetingId);
    }
  }, [open, meetingId, refetch]);

  useEffect(() => {
    if (error) {
      toast.error('Lỗi tài liệu tham khảo', { description: error });
    }
  }, [error]);

  const handleAttach = async () => {
    await selectAndAttach(meetingId);
  };

  const handleRemove = async (documentId: string) => {
    await remove(documentId);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-0 overflow-hidden p-0 sm:max-w-md">
        <DialogHeader className="space-y-0 px-5 pb-4 pt-5 pr-12 text-left">
          <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-ink-2">
            Báo cáo AI
          </p>
          <DialogTitle className="mt-1 text-base font-semibold tracking-[-0.02em] text-ink">
            Tài liệu tham khảo
          </DialogTitle>
          <DialogDescription className="mt-1.5 text-xs text-ink-2">
            Đính kèm slide, văn bản hoặc phụ đề để AI tham khảo khi tạo báo cáo.
          </DialogDescription>
        </DialogHeader>

        <div className="px-5 pb-4">
          {status === 'loading' ? (
            <div className="flex items-center justify-center rounded-md border border-rule py-8 text-xs text-ink-2">
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              Đang tải...
            </div>
          ) : documents.length > 0 ? (
            <ul className="max-h-64 overflow-y-auto rounded-md border border-rule">
              {documents.map((doc, index) => (
                <li
                  key={doc.id}
                  className="flex items-center gap-2.5 border-b border-rule px-2.5 py-2 last:border-b-0"
                >
                  <span className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-rule bg-paper font-mono text-[11px] font-medium text-ink-2">
                    {String(index + 1).padStart(2, '0')}
                  </span>
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium text-ink">{doc.filename}</p>
                    <p className="mt-0.5 font-mono text-[11px] text-ink-2">
                      {doc.char_count.toLocaleString()} ký tự
                    </p>
                  </div>
                  <button
                    type="button"
                    onClick={() => handleRemove(doc.id)}
                    disabled={isBusy}
                    className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-ink-2 hover:bg-secondary hover:text-destructive disabled:opacity-40"
                    title="Xóa tài liệu"
                    aria-label="Xóa tài liệu"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <div className="rounded-md border border-dashed border-rule px-3 py-8 text-center">
              <p className="text-sm text-ink-2">Chưa có tài liệu nào được đính kèm</p>
              <p className="mt-1.5 font-mono text-[11px] text-ink-2">
                PDF, DOCX, PPTX, TXT, SRT, VTT
              </p>
            </div>
          )}
        </div>

        <DialogFooter className="border-t border-rule bg-paper px-5 py-3">
          <Button variant="outline" size="sm" onClick={() => onOpenChange(false)}>
            Đóng
          </Button>
          <Button size="sm" onClick={handleAttach} disabled={isBusy}>
            {status === 'attaching' ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                Thêm tài liệu
              </>
            ) : (
              'Thêm tài liệu'
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
