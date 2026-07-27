'use client';

import { createContext, useContext, useCallback, ReactNode } from 'react';

interface DocumentImportDialogContextType {
  openDocumentImportDialog: () => void;
}

const DocumentImportDialogContext = createContext<DocumentImportDialogContextType | null>(null);

export const useDocumentImportDialog = () => {
  const ctx = useContext(DocumentImportDialogContext);
  if (!ctx) {
    throw new Error('useDocumentImportDialog must be used within DocumentImportDialogProvider');
  }
  return ctx;
};

interface DocumentImportDialogProviderProps {
  children: ReactNode;
  onOpen: () => void;
}

export function DocumentImportDialogProvider({ children, onOpen }: DocumentImportDialogProviderProps) {
  const openDocumentImportDialog = useCallback(() => {
    onOpen();
  }, [onOpen]);

  return (
    <DocumentImportDialogContext.Provider value={{ openDocumentImportDialog }}>
      {children}
    </DocumentImportDialogContext.Provider>
  );
}
