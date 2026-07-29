import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface MeetingDocumentInfo {
  id: string;
  filename: string;
  char_count: number;
  created_at: string;
}

export type MeetingDocumentsStatus = 'idle' | 'loading' | 'attaching' | 'error';

export interface UseMeetingDocumentsReturn {
  documents: MeetingDocumentInfo[];
  status: MeetingDocumentsStatus;
  error: string | null;
  isBusy: boolean;
  refetch: (meetingId: string) => Promise<void>;
  selectAndAttach: (meetingId: string) => Promise<void>;
  remove: (documentId: string) => Promise<void>;
}

function extractErrorMessage(err: unknown, fallback: string): string {
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  return fallback;
}

export function useMeetingDocuments(): UseMeetingDocumentsReturn {
  const [documents, setDocuments] = useState<MeetingDocumentInfo[]>([]);
  const [status, setStatus] = useState<MeetingDocumentsStatus>('idle');
  const [error, setError] = useState<string | null>(null);

  const refetch = useCallback(async (meetingId: string): Promise<void> => {
    setStatus('loading');
    setError(null);

    try {
      const result = await invoke<MeetingDocumentInfo[]>('api_list_meeting_documents', {
        meetingId,
      });
      setDocuments(result);
      setStatus('idle');
    } catch (err) {
      const errorMsg = extractErrorMessage(err, 'Không tải được danh sách tài liệu');
      setStatus('error');
      setError(errorMsg);
    }
  }, []);

  const selectAndAttach = useCallback(async (meetingId: string): Promise<void> => {
    setStatus('attaching');
    setError(null);

    try {
      const paths = await invoke<string[]>('api_select_meeting_document_files');
      if (paths.length === 0) {
        setStatus('idle');
        return;
      }

      for (const path of paths) {
        const doc = await invoke<MeetingDocumentInfo>('api_attach_meeting_document', {
          meetingId,
          path,
        });
        setDocuments((prev) => [...prev, doc]);
      }
      setStatus('idle');
    } catch (err) {
      const errorMsg = extractErrorMessage(err, 'Không đính kèm được tài liệu');
      setStatus('error');
      setError(errorMsg);
    }
  }, []);

  const remove = useCallback(async (documentId: string): Promise<void> => {
    setError(null);

    try {
      await invoke('api_delete_meeting_document', { documentId });
      setDocuments((prev) => prev.filter((d) => d.id !== documentId));
    } catch (err) {
      const errorMsg = extractErrorMessage(err, 'Không xóa được tài liệu');
      setStatus('error');
      setError(errorMsg);
    }
  }, []);

  return {
    documents,
    status,
    error,
    isBusy: status === 'loading' || status === 'attaching',
    refetch,
    selectAndAttach,
    remove,
  };
}
