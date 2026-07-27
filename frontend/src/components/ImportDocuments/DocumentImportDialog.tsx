'use client';

import React, { useState, useEffect, useRef } from 'react';
import { Upload, Loader2, FileText } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Button } from '../ui/button';
import { Textarea } from '../ui/textarea';
import { toast } from 'sonner';
import { useRouter } from 'next/navigation';
import { useSidebar } from '../Sidebar/SidebarProvider';
import { useImportDocuments } from '@/hooks/useImportDocuments';

interface DocumentImportDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function filenameFromPath(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

export function DocumentImportDialog({ open, onOpenChange }: DocumentImportDialogProps) {
  const router = useRouter();
  const { refetchMeetings } = useSidebar();

  const [title, setTitle] = useState('');
  const [titleModifiedByUser, setTitleModifiedByUser] = useState(false);
  const prevOpenRef = useRef(false);

  const { status, selectedPaths, error, selectFiles, importDocuments, reset } = useImportDocuments();

  // Reset state only when the dialog transitions from closed to open
  useEffect(() => {
    const wasOpen = prevOpenRef.current;
    prevOpenRef.current = open;

    if (open && !wasOpen) {
      reset();
      setTitle('');
      setTitleModifiedByUser(false);
    }
  }, [open, reset]);

  useEffect(() => {
    if (error) {
      toast.error('Nhập tài liệu thất bại', { description: error });
    }
  }, [error]);

  const handleSelectFiles = async () => {
    const paths = await selectFiles();
    if (paths.length > 0 && !titleModifiedByUser) {
      setTitle(filenameFromPath(paths[0]));
    }
  };

  const handleImport = async () => {
    if (selectedPaths.length === 0) return;
    const finalTitle = title.trim() || filenameFromPath(selectedPaths[0]);

    const result = await importDocuments(selectedPaths, finalTitle);
    if (result) {
      toast.success(`Đã tạo cuộc họp từ ${result.files_count} tài liệu`);
      refetchMeetings();
      onOpenChange(false);
      router.push(`/meeting-details?id=${result.meeting_id}&source=import`);
    }
  };

  const isImporting = status === 'importing';

  const handleOpenChange = (newOpen: boolean) => {
    if (!newOpen && isImporting) return;
    onOpenChange(newOpen);
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {isImporting ? (
              <>
                <Loader2 className="h-5 w-5 animate-spin text-blue-600" />
                Đang nhập tài liệu...
              </>
            ) : (
              <>
                <Upload className="h-5 w-5 text-blue-600" />
                Tải tài liệu lên
              </>
            )}
          </DialogTitle>
          <DialogDescription>
            Chọn một hoặc nhiều file PDF, DOCX, TXT, SRT hoặc VTT để tạo cuộc họp mới từ nội dung có sẵn
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          {selectedPaths.length > 0 ? (
            <div className="bg-gray-50 rounded-lg p-4 space-y-3">
              <ul className="space-y-1 max-h-32 overflow-y-auto">
                {selectedPaths.map((path) => (
                  <li key={path} className="flex items-center gap-2 text-sm text-gray-700">
                    <FileText className="h-4 w-4 text-blue-600 shrink-0" />
                    <span className="truncate">{filenameFromPath(path)}</span>
                  </li>
                ))}
              </ul>

              <div className="space-y-1">
                <label className="text-sm font-medium text-gray-700">Tiêu đề cuộc họp</label>
                <Textarea
                  value={title}
                  onChange={(e) => {
                    setTitle(e.target.value);
                    setTitleModifiedByUser(true);
                  }}
                  placeholder="Nhập tiêu đề cuộc họp"
                  rows={2}
                />
              </div>

              <Button variant="outline" size="sm" onClick={handleSelectFiles} className="w-full">
                Chọn file khác
              </Button>
            </div>
          ) : (
            <div className="border-2 border-dashed border-gray-300 rounded-lg p-8 text-center">
              <FileText className="h-12 w-12 text-gray-400 mx-auto mb-4" />
              <Button onClick={handleSelectFiles} disabled={status === 'selecting'}>
                {status === 'selecting' ? (
                  <>
                    <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                    Đang chọn...
                  </>
                ) : (
                  <>
                    <Upload className="h-4 w-4 mr-2" />
                    Chọn tài liệu
                  </>
                )}
              </Button>
              <p className="text-sm text-gray-500 mt-2">PDF, DOCX, TXT, SRT, VTT</p>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={isImporting}>
            Hủy
          </Button>
          <Button
            onClick={handleImport}
            className="bg-[#16478e] hover:bg-[#1a55ab]"
            disabled={selectedPaths.length === 0 || isImporting}
          >
            {isImporting ? (
              <Loader2 className="h-4 w-4 mr-2 animate-spin" />
            ) : (
              <Upload className="h-4 w-4 mr-2" />
            )}
            Tạo cuộc họp
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
