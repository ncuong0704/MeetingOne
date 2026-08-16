'use client';

import React, { useEffect, useRef } from 'react';
import { Plus, Save, Trash2, Copy, ArrowLeft } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { SectionEditor } from './SectionEditor';
import type { TemplateData, TemplateInfo, TemplateSection } from './types';

interface TemplateEditorProps {
  mode: 'edit' | 'new';
  data: TemplateData | null;
  selectedInfo: TemplateInfo | undefined;
  isSaving: boolean;
  isDeleting: boolean;
  onUpdateMeta: (field: 'name' | 'description', value: string) => void;
  onAddSection: () => void;
  onRemoveSection: (index: number) => void;
  onMoveSection: (index: number, direction: 'up' | 'down') => void;
  onUpdateSection: (index: number, field: keyof TemplateSection, value: string) => void;
  onSave: () => void;
  onDelete: (id: string) => void;
  onClone: (id: string) => void;
  onCancel: () => void;
  onBack: () => void;
}

const btnPrimary =
  'inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-2.5 text-xs font-medium text-primary-foreground hover:bg-primary-hover disabled:opacity-50';
const btnOutline =
  'inline-flex h-8 items-center gap-1.5 rounded-md border border-rule bg-paper-2 px-2.5 text-xs font-medium text-ink-2 hover:bg-secondary hover:text-ink disabled:opacity-50';
const btnDanger =
  'inline-flex h-8 items-center gap-1.5 rounded-md bg-destructive px-2.5 text-xs font-medium text-destructive-foreground hover:bg-destructive/90 disabled:opacity-50';

export function TemplateEditor({
  mode,
  data,
  selectedInfo,
  isSaving,
  isDeleting,
  onUpdateMeta,
  onAddSection,
  onRemoveSection,
  onMoveSection,
  onUpdateSection,
  onSave,
  onDelete,
  onClone,
  onCancel,
  onBack,
}: TemplateEditorProps) {
  const descriptionRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const el = descriptionRef.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${el.scrollHeight}px`;
  }, [data?.description]);

  if (!data) return null;

  const canSave = data.name.trim() && data.description.trim() && data.sections.length > 0;

  return (
    <div className="space-y-6">
      <section>
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={onBack}
            className="inline-flex h-8 w-8 items-center justify-center rounded-md text-ink-2 hover:bg-secondary hover:text-ink"
            title="Quay lại danh sách"
          >
            <ArrowLeft className="h-3.5 w-3.5" />
          </button>
          <h2 className="text-sm font-semibold text-ink tracking-tight">
            {mode === 'new' ? 'Tạo mẫu mới' : 'Chỉnh sửa mẫu'}
          </h2>
        </div>
        <p className="text-xs text-ink-2 mt-0.5 mb-2">
          {mode === 'new'
            ? 'Đặt tên, mô tả và thêm các phần trước khi lưu.'
            : 'Sửa tên, mô tả và các phần của mẫu.'}
        </p>
        <div className="app-surface overflow-hidden px-4 py-3 space-y-3">
          <div className="space-y-1.5">
            <label className="text-sm font-medium text-ink">Tên mẫu</label>
            <Input
              value={data.name}
              onChange={e => onUpdateMeta('name', e.target.value)}
              placeholder="VD: Mẫu kết luận giao ban ACT"
              className="h-9 text-sm"
            />
          </div>
          <div className="space-y-1.5">
            <label className="text-sm font-medium text-ink">Mô tả</label>
            <Textarea
              ref={descriptionRef}
              value={data.description}
              onChange={e => onUpdateMeta('description', e.target.value)}
              placeholder="Mô tả ngắn về mục đích của mẫu này..."
              rows={1}
              className="text-sm min-h-9 resize-none overflow-hidden"
            />
          </div>
        </div>
      </section>

      <section>
        <div className="flex items-center justify-between gap-3">
          <h2 className="text-sm font-semibold text-ink tracking-tight">
            Các phần ({data.sections.length})
          </h2>
          <button type="button" onClick={onAddSection} className={btnOutline}>
            <Plus className="h-3.5 w-3.5" />
            Thêm phần
          </button>
        </div>
        <p className="text-xs text-ink-2 mt-0.5 mb-2">
          Mỗi phần có tiêu đề, chỉ dẫn cho AI và định dạng.
        </p>
        <div className="app-surface overflow-hidden divide-y divide-rule">
          {data.sections.length === 0 ? (
            <p className="px-4 py-3 text-xs text-ink-2">
              Chưa có phần nào. Thêm phần đầu tiên.
            </p>
          ) : (
            data.sections.map((section, idx) => (
              <SectionEditor
                key={section._key ?? idx}
                section={section}
                index={idx}
                total={data.sections.length}
                onChange={(field, value) => onUpdateSection(idx, field, value)}
                onMoveUp={() => onMoveSection(idx, 'up')}
                onMoveDown={() => onMoveSection(idx, 'down')}
                onRemove={() => onRemoveSection(idx)}
              />
            ))
          )}
        </div>
      </section>

      <div className="flex items-center justify-end gap-1.5">
        {mode === 'new' && (
          <>
            <button type="button" onClick={onCancel} className={btnOutline}>
              Huỷ
            </button>
            <button type="button" onClick={onSave} disabled={!canSave || isSaving} className={btnPrimary}>
              <Save className="h-3.5 w-3.5" />
              {isSaving ? 'Đang lưu...' : 'Tạo mẫu'}
            </button>
          </>
        )}
        {mode === 'edit' && (
          <>
            <button
              type="button"
              onClick={() => selectedInfo && onClone(selectedInfo.id)}
              className={btnOutline}
            >
              <Copy className="h-3.5 w-3.5" />
              Sao chép
            </button>
            <button
              type="button"
              onClick={() => selectedInfo && onDelete(selectedInfo.id)}
              disabled={isDeleting}
              className={btnDanger}
            >
              <Trash2 className="h-3.5 w-3.5" />
              {isDeleting ? 'Đang xóa...' : 'Xóa'}
            </button>
            <button type="button" onClick={onSave} disabled={!canSave || isSaving} className={btnPrimary}>
              <Save className="h-3.5 w-3.5" />
              {isSaving ? 'Đang lưu...' : 'Lưu'}
            </button>
          </>
        )}
      </div>
    </div>
  );
}
