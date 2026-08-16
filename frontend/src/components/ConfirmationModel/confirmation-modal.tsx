import React from 'react';

interface ConfirmationModalProps {
  onConfirm: () => void;
  onCancel: () => void;
  text: string;
  isOpen: boolean;
}

export function ConfirmationModal({ onConfirm, onCancel, text, isOpen }: ConfirmationModalProps) {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 app-modal-overlay flex items-center justify-center z-[var(--z-modal)]">
      <div className="bg-paper-2 rounded-md p-6 max-w-md w-full mx-4 border border-rule shadow-[var(--shadow-modal)]">
        <h2 className="text-xl font-semibold mb-4 text-foreground">Xóa cuộc họp</h2>
        <p className="text-muted-foreground mb-6">{text}</p>
        <div className="flex justify-end space-x-4">
          <button
            onClick={onCancel}
            className="px-4 py-2 text-muted-foreground hover:bg-secondary rounded-md transition-colors"
          >
            Hủy
          </button>
          <button
            onClick={onConfirm}
            className="px-4 py-2 bg-destructive text-destructive-foreground hover:bg-destructive/90 rounded-md transition-colors"
          >
            Xóa
          </button>
        </div>
      </div>
    </div>
  );
}
