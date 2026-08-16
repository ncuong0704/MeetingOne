'use client';

import React from 'react';
import { Plus, Save, Trash2, Copy, ArrowLeft } from 'lucide-react';
import { Button } from '@/components/ui/button';
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
  if (!data) return null;

  const canSave = data.name.trim() && data.description.trim() && data.sections.length > 0;

  return (
    <div className="flex-1 flex flex-col app-surface overflow-hidden min-w-0">
      {/* Header */}
      <div className="flex items-center justify-between px-5 py-3 border-b border-rule shrink-0">
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onBack}
            className="p-1 rounded-md hover:bg-secondary transition-colors"
            title="Quay lại danh sách"
          >
            <ArrowLeft className="w-4 h-4 text-ink-2" />
          </button>
          <h3 className="text-sm font-semibold text-ink">
            {mode === 'new' ? 'Tạo mẫu mới' : 'Chỉnh sửa mẫu'}
          </h3>
        </div>

        {/* Action bar */}
        <div className="flex items-center gap-2">
          {mode === 'new' && (
            <>
              <Button variant="ghost" size="sm" onClick={onCancel} className="text-xs">
                Huỷ
              </Button>
              <Button
                variant="blue"
                size="sm"
                onClick={onSave}
                disabled={!canSave || isSaving}
                className="text-xs gap-1"
              >
                <Save className="w-3.5 h-3.5" />
                {isSaving ? 'Đang lưu...' : 'Tạo mẫu'}
              </Button>
            </>
          )}

          {mode === 'edit' && (
            <>
              <Button
                variant="outline"
                size="sm"
                onClick={() => selectedInfo && onClone(selectedInfo.id)}
                className="text-xs gap-1"
              >
                <Copy className="w-3.5 h-3.5" />
                Sao chép
              </Button>
              <Button
                variant="destructive"
                size="sm"
                onClick={() => selectedInfo && onDelete(selectedInfo.id)}
                disabled={isDeleting}
                className="text-xs gap-1"
              >
                <Trash2 className="w-3.5 h-3.5" />
                {isDeleting ? 'Đang xóa...' : 'Xóa'}
              </Button>
              <Button
                variant="blue"
                size="sm"
                onClick={onSave}
                disabled={!canSave || isSaving}
                className="text-xs gap-1"
              >
                <Save className="w-3.5 h-3.5" />
                {isSaving ? 'Đang lưu...' : 'Lưu'}
              </Button>
            </>
          )}
        </div>
      </div>

      {/* Form */}
      <div className="flex-1 min-h-0 overflow-y-auto">
        <div className="px-5 py-4 space-y-4">
          {/* Name */}
          <div className="space-y-1">
            <label className="text-xs font-medium text-ink-2">Tên mẫu</label>
            <Input
              value={data.name}
              onChange={e => onUpdateMeta('name', e.target.value)}
              placeholder="VD: Mẫu kết luận giao ban ACT"
              className="text-sm"
            />
          </div>

          {/* Description */}
          <div className="space-y-1">
            <label className="text-xs font-medium text-ink-2">Mô tả</label>
            <Textarea
              value={data.description}
              onChange={e => onUpdateMeta('description', e.target.value)}
              placeholder="Mô tả ngắn về mục đích của mẫu này..."
              className="text-sm min-h-[60px] resize-y"
            />
          </div>

          {/* Sections */}
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <label className="text-xs font-medium text-ink-2">
                Các phần ({data.sections.length})
              </label>
              <Button
                variant="outline"
                size="sm"
                onClick={onAddSection}
                className="text-xs h-7 px-2 gap-1"
              >
                <Plus className="w-3.5 h-3.5" />
                Thêm phần
              </Button>
            </div>

            <div className="space-y-2">
              {data.sections.map((section, idx) => (
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
              ))}
            </div>

            {data.sections.length === 0 && (
              <p className="text-xs text-ink-2 text-center py-4">
                Chưa có phần nào. Thêm phần đầu tiên.
              </p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
