'use client';

import React, { useEffect, useState } from 'react';
import { AlertTriangle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { TemplateList } from './TemplateList';
import { TemplateEditor } from './TemplateEditor';
import { useTemplateSettings } from './useTemplateSettings';

function DeleteConfirmDialog({
  templateName,
  onConfirm,
  onCancel,
}: {
  templateName: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center app-modal-overlay">
      <div className="app-surface p-6 max-w-sm w-full mx-4 space-y-4 shadow-[var(--shadow-modal)]">
        <div className="flex items-start gap-3">
          <AlertTriangle className="w-5 h-5 text-destructive shrink-0 mt-0.5" />
          <div>
            <h4 className="text-sm font-semibold text-ink">Xác nhận xóa mẫu</h4>
            <p className="text-sm text-ink-2 mt-1">
              Bạn có chắc muốn xóa mẫu <strong>&quot;{templateName}&quot;</strong>?
              {' '}Hành động này không thể hoàn tác.
            </p>
          </div>
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={onCancel}>
            Huỷ
          </Button>
          <Button variant="destructive" size="sm" onClick={onConfirm}>
            Xóa
          </Button>
        </div>
      </div>
    </div>
  );
}

export function TemplateSettings() {
  const state = useTemplateSettings();
  const [deleteTargetId, setDeleteTargetId] = useState<string | null>(null);
  const { loadTemplates } = state;

  useEffect(() => {
    loadTemplates();
  }, [loadTemplates]);

  const deleteTargetInfo = deleteTargetId
    ? state.templates.find(t => t.id === deleteTargetId)
    : null;

  const selectedInfo = state.selectedId
    ? state.templates.find(t => t.id === state.selectedId)
    : undefined;

  return (
    <div className="flex h-[calc(100vh-180px)] min-h-[400px]">
      {state.editorMode === 'idle' ? (
        <TemplateList
          templates={state.templates}
          selectedId={state.selectedId}
          defaultTemplateId={state.defaultTemplateId}
          isSettingDefault={state.isSettingDefault}
          isLoading={state.isLoadingList}
          onSelect={state.openTemplate}
          onNew={state.startNewTemplate}
          onSetDefault={state.setAsDefault}
        />
      ) : (
        <TemplateEditor
          mode={state.editorMode}
          data={state.editorData}
          selectedInfo={selectedInfo}
          isSaving={state.isSaving}
          isDeleting={state.isDeleting}
          onUpdateMeta={state.updateMeta}
          onAddSection={state.addSection}
          onRemoveSection={state.removeSection}
          onMoveSection={state.moveSection}
          onUpdateSection={state.updateSection}
          onSave={state.saveTemplate}
          onDelete={id => setDeleteTargetId(id)}
          onClone={state.cloneTemplate}
          onCancel={state.cancelEdit}
          onBack={state.closeEditor}
        />
      )}

      {deleteTargetId && deleteTargetInfo && (
        <DeleteConfirmDialog
          templateName={deleteTargetInfo.name}
          onConfirm={async () => {
            setDeleteTargetId(null);
            await state.deleteTemplate(deleteTargetId);
          }}
          onCancel={() => setDeleteTargetId(null)}
        />
      )}
    </div>
  );
}
