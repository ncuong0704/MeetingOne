import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface DocumentImportResult {
  meeting_id: string;
  title: string;
  files_count: number;
}

export type DocumentImportStatus = 'idle' | 'selecting' | 'importing' | 'error';

export interface UseImportDocumentsReturn {
  status: DocumentImportStatus;
  selectedPaths: string[];
  error: string | null;
  isBusy: boolean;
  selectFiles: () => Promise<string[]>;
  importDocuments: (paths: string[], title: string) => Promise<DocumentImportResult | null>;
  reset: () => void;
}

function extractErrorMessage(err: unknown, fallback: string): string {
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  return fallback;
}

export function useImportDocuments(): UseImportDocumentsReturn {
  const [status, setStatus] = useState<DocumentImportStatus>('idle');
  const [selectedPaths, setSelectedPaths] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  const selectFiles = useCallback(async (): Promise<string[]> => {
    setStatus('selecting');
    setError(null);

    try {
      const paths = await invoke<string[]>('api_select_document_files');
      setSelectedPaths(paths);
      setStatus('idle');
      return paths;
    } catch (err) {
      const errorMsg = extractErrorMessage(err, 'Không thể chọn file');
      setStatus('error');
      setError(errorMsg);
      return [];
    }
  }, []);

  const importDocuments = useCallback(
    async (paths: string[], title: string): Promise<DocumentImportResult | null> => {
      setStatus('importing');
      setError(null);

      try {
        const result = await invoke<DocumentImportResult>('api_import_documents', { paths, title });
        setStatus('idle');
        return result;
      } catch (err) {
        const errorMsg = extractErrorMessage(err, 'Nhập tài liệu thất bại');
        setStatus('error');
        setError(errorMsg);
        return null;
      }
    },
    []
  );

  const reset = useCallback(() => {
    setStatus('idle');
    setSelectedPaths([]);
    setError(null);
  }, []);

  return {
    status,
    selectedPaths,
    error,
    isBusy: status === 'selecting' || status === 'importing',
    selectFiles,
    importDocuments,
    reset,
  };
}
