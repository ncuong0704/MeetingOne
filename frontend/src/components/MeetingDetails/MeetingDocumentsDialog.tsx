'use client';

import React, { useEffect, useRef } from 'react';
import { Paperclip, Loader2, FileText, Trash2, Upload } from 'lucide-react';
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
  onDocumentsChanged?: (count: number) => void;
}

export function MeetingDocumentsDialog({
  open,
  onOpenChange,
  meetingId,
  onDocumentsChanged,
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

  useEffect(() => {
    onDocumentsChanged?.(documents.length);
  }, [documents.length, onDocumentsChanged]);

  const handleAttach = async () => {
    await selectAndAttach(meetingId);
  };

  const handleRemove = async (documentId: string) => {
    await remove(documentId);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Paperclip className="h-5 w-5 text-blue-600" />
            Tài liệu tham khảo
          </DialogTitle>
          <DialogDescription>
            Đính kèm slide, văn bản (PDF, DOCX, PPTX) được dùng trong cuộc họp để AI tham khảo khi
            tạo báo cáo.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3 py-2">
          {status === 'loading' ? (
            <div className="flex items-center justify-center py-8 text-gray-400 text-sm">
              <Loader2 className="h-4 w-4 mr-2 animate-spin" />
              Đang tải...
            </div>
          ) : documents.length > 0 ? (
            <ul className="space-y-1 max-h-64 overflow-y-auto">
              {documents.map((doc) => (
                <li
                  key={doc.id}
                  className="flex items-center gap-2 text-sm text-gray-700 bg-gray-50 rounded-lg px-3 py-2"
                >
                  <FileText className="h-4 w-4 text-blue-600 shrink-0" />
                  <span className="truncate flex-1">{doc.filename}</span>
                  <span className="text-xs text-gray-400 shrink-0">
                    {doc.char_count.toLocaleString()} ký tự
                  </span>
                  <button
                    onClick={() => handleRemove(doc.id)}
                    disabled={isBusy}
                    className="text-gray-400 hover:text-red-600 disabled:opacity-40 shrink-0"
                    title="Xóa tài liệu"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <div className="border-2 border-dashed border-gray-300 rounded-lg p-6 text-center text-sm text-gray-500">
              Chưa có tài liệu nào được đính kèm
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Đóng
          </Button>
          <Button
            onClick={handleAttach}
            disabled={isBusy}
            className="bg-[#16478e] hover:bg-[#1a55ab]"
          >
            {status === 'attaching' ? (
              <Loader2 className="h-4 w-4 mr-2 animate-spin" />
            ) : (
              <Upload className="h-4 w-4 mr-2" />
            )}
            Thêm tài liệu
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
