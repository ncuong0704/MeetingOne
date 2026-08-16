'use client';

import React from 'react';
import dynamic from 'next/dynamic';
import { ChevronUp, ChevronDown, Trash2 } from 'lucide-react';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { cn } from '@/lib/utils';
import type { TemplateSection } from './types';
import { TableColumnsEditor } from './TableColumnsEditor';

const SectionInstructionEditor = dynamic(() => import('./SectionInstructionEditor'), {
  ssr: false,
  loading: () => (
    <div className="text-xs text-ink-2 px-3 py-2 border border-rule rounded-md min-h-[80px]">
      Đang tải trình soạn thảo...
    </div>
  ),
});

interface SectionEditorProps {
  section: TemplateSection;
  index: number;
  total: number;
  disabled?: boolean;
  onChange: (field: keyof TemplateSection, value: string) => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onRemove: () => void;
}

export function SectionEditor({
  section,
  index,
  total,
  disabled = false,
  onChange,
  onMoveUp,
  onMoveDown,
  onRemove,
}: SectionEditorProps) {
  const iconBtn =
    'inline-flex h-8 w-8 items-center justify-center rounded-md text-ink-2 hover:bg-secondary hover:text-ink disabled:opacity-30 disabled:cursor-not-allowed';

  return (
    <div className="px-4 py-3 space-y-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm font-medium text-ink">Phần {index + 1}</span>
        {!disabled && (
          <div className="flex items-center">
            <button type="button" onClick={onMoveUp} disabled={index === 0} className={iconBtn} title="Di chuyển lên">
              <ChevronUp className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              onClick={onMoveDown}
              disabled={index === total - 1}
              className={iconBtn}
              title="Di chuyển xuống"
            >
              <ChevronDown className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              onClick={onRemove}
              disabled={total <= 1}
              className={cn(iconBtn, 'hover:bg-destructive/10 hover:text-destructive')}
              title="Xóa phần này"
            >
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          </div>
        )}
      </div>

      <div className="space-y-1.5">
        <label className="text-sm font-medium text-ink">Tiêu đề</label>
        <Input
          value={section.title}
          onChange={e => onChange('title', e.target.value)}
          placeholder="VD: I. Nội dung cuộc họp"
          className="h-9 text-sm"
          disabled={disabled}
        />
      </div>

      <div className="space-y-1.5">
        <label className="text-sm font-medium text-ink">Chỉ dẫn cho AI</label>
        <SectionInstructionEditor
          value={section.instruction}
          onChange={md => onChange('instruction', md)}
          disabled={disabled}
        />
      </div>

      <div className="space-y-1.5">
        <label className="text-sm font-medium text-ink">Định dạng</label>
        <Select
          value={section.format}
          onValueChange={val => onChange('format', val)}
          disabled={disabled}
        >
          <SelectTrigger className="text-sm h-9">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="paragraph">Đoạn văn (paragraph)</SelectItem>
            <SelectItem value="list">Bảng (table)</SelectItem>
            <SelectItem value="string">Chuỗi ngắn (string)</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {section.format === 'list' && (
        <div className="space-y-1.5">
          <label className="text-sm font-medium text-ink">Cột bảng</label>
          <p className="text-xs text-ink-2">
            Thêm cột và đặt tiêu đề. AI điền nội dung theo bảng này.
          </p>
          <TableColumnsEditor
            value={section.item_format ?? section.example_item_format ?? ''}
            onChange={itemFormat => onChange('item_format', itemFormat)}
            disabled={disabled}
          />
        </div>
      )}
    </div>
  );
}
