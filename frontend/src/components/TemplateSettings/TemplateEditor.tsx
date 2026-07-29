'use client';

import React from 'react';
import { Plus, Save, Trash2, Copy, ArrowLeft } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { ScrollArea } from '@/components/ui/scroll-area';
import { SectionEditor } from './SectionEditor';
import type { TemplateData, TemplateInfo, TemplateSection } from './types';

interface TemplateEditorProps {
  mode: 'edit' | 'new';
  data: TemplateData | null;
  editingId: string;
  selectedInfo: TemplateInfo | undefined;
  isSaving: boolean;
  isDeleting: boolean;
  onEditingIdChange: (id: string) => void;
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

function IdField({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  const isValid = /^[a-zA-Z0-9_-]*$/.test(value);
  return (
    <div className="space-y-1">
      <label className="text-xs font-medium text-gray-600">
        ID mẫu
        <span className="ml-1 font-normal text-gray-400">
          — dùng làm tên file, chỉ dùng chữ/số/gạch dưới/gạch ngang
        </span>
      </label>
      <Input
        value={value}
        onChange={e => onChange(e.target.value.toLowerCase().replace(/\s/g, '_'))}
        placeholder="vd: mau_ket_luan_hop"
        className={`text-sm font-mono ${!isValid && value ? 'border-red-400 focus-visible:ring-red-400' : ''}`}
      />
      {!isValid && value && (
        <p className="text-xs text-red-500">ID chỉ được chứa chữ cái, số, dấu gạch dưới hoặc dấu gạch ngang</p>
      )}
    </div>
  );
}

export function TemplateEditor({
  mode,
  data,
  editingId,
  selectedInfo,
  isSaving,
  isDeleting,
  onEditingIdChange,
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

  const isBuiltin = selectedInfo
    ? !selectedInfo.is_custom && !selectedInfo.has_custom_override
    : false;
  const hasOverride = selectedInfo?.has_custom_override ?? false;
  const isCustomOnly = selectedInfo?.is_custom ?? false;

  const idValid = /^[a-zA-Z0-9_-]+$/.test(editingId);
  const canSave = idValid && data.name.trim() && data.description.trim() && data.sections.length > 0;

  return (
    <div className="flex-1 flex flex-col border border-gray-200 rounded-xl overflow-hidden bg-white min-w-0">
      {/* Header */}
      <div className="flex items-center justify-between px-5 py-3 border-b border-gray-100 shrink-0">
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onBack}
            className="p-1 rounded hover:bg-gray-100 transition-colors"
            title="Quay lại danh sách"
          >
            <ArrowLeft className="w-4 h-4 text-gray-500" />
          </button>
          <h3 className="text-sm font-semibold text-gray-800">
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

          {mode === 'edit' && isBuiltin && (
            <Button
              variant="outline"
              size="sm"
              onClick={() => selectedInfo && onClone(selectedInfo.id)}
              className="text-xs gap-1"
            >
              <Copy className="w-3.5 h-3.5" />
              Sao chép & Chỉnh sửa
            </Button>
          )}

          {mode === 'edit' && (hasOverride || isCustomOnly) && (
            <>
              {hasOverride && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => selectedInfo && onClone(selectedInfo.id)}
                  className="text-xs gap-1"
                >
                  <Copy className="w-3.5 h-3.5" />
                  Sao chép
                </Button>
              )}
              <Button
                variant="destructive"
                size="sm"
                onClick={() => selectedInfo && onDelete(selectedInfo.id)}
                disabled={isDeleting}
                className="text-xs gap-1"
              >
                <Trash2 className="w-3.5 h-3.5" />
                {isDeleting ? 'Đang xóa...' : hasOverride ? 'Xóa bản tuỳ chỉnh' : 'Xóa'}
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

      {/* Note for built-in view-only */}
      {mode === 'edit' && isBuiltin && (
        <div className="mx-5 mt-3 px-3 py-2 bg-gray-50 border border-gray-200 rounded-lg text-xs text-gray-500">
          Đây là mẫu mặc định (không thể xóa). Dùng &quot;Sao chép & Chỉnh sửa&quot; để tạo bản tùy chỉnh.
        </div>
      )}

      {/* Form */}
      <ScrollArea className="flex-1">
        <div className="px-5 py-4 space-y-4">
          {/* ID — only in new mode */}
          {mode === 'new' && (
            <IdField value={editingId} onChange={onEditingIdChange} />
          )}

          {/* Name */}
          <div className="space-y-1">
            <label className="text-xs font-medium text-gray-600">Tên mẫu</label>
            <Input
              value={data.name}
              onChange={e => onUpdateMeta('name', e.target.value)}
              placeholder="VD: Mẫu kết luận giao ban ACT"
              className="text-sm"
              disabled={mode === 'edit' && isBuiltin}
            />
          </div>

          {/* Description */}
          <div className="space-y-1">
            <label className="text-xs font-medium text-gray-600">Mô tả</label>
            <Textarea
              value={data.description}
              onChange={e => onUpdateMeta('description', e.target.value)}
              placeholder="Mô tả ngắn về mục đích của mẫu này..."
              className="text-sm min-h-[60px] resize-y"
              disabled={mode === 'edit' && isBuiltin}
            />
          </div>

          {/* Sections */}
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <label className="text-xs font-medium text-gray-600">
                Các phần ({data.sections.length})
              </label>
              {!(mode === 'edit' && isBuiltin) && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={onAddSection}
                  className="text-xs h-7 px-2 gap-1"
                >
                  <Plus className="w-3.5 h-3.5" />
                  Thêm phần
                </Button>
              )}
            </div>

            <div className="space-y-2">
              {data.sections.map((section, idx) => (
                <SectionEditor
                  key={section._key ?? idx}
                  section={section}
                  index={idx}
                  total={data.sections.length}
                  disabled={mode === 'edit' && isBuiltin}
                  onChange={(field, value) => onUpdateSection(idx, field, value)}
                  onMoveUp={() => onMoveSection(idx, 'up')}
                  onMoveDown={() => onMoveSection(idx, 'down')}
                  onRemove={() => onRemoveSection(idx)}
                />
              ))}
            </div>

            {data.sections.length === 0 && (
              <p className="text-xs text-gray-400 text-center py-4">
                Chưa có phần nào. Thêm phần đầu tiên.
              </p>
            )}
          </div>
        </div>
      </ScrollArea>
    </div>
  );
}
