'use client';

import React from 'react';
import dynamic from 'next/dynamic';
import { ChevronUp, ChevronDown, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
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
    <div className="text-xs text-gray-400 px-3 py-2 border border-gray-200 rounded-md min-h-[80px]">
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
  return (
    <div className="border border-gray-200 rounded-lg p-4 bg-white space-y-3">
      {/* Header row */}
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-semibold text-gray-500 uppercase tracking-wide">
          Phần {index + 1}
        </span>
        <div className="flex items-center gap-1">
          {!disabled && (
            <>
              <button
                type="button"
                onClick={onMoveUp}
                disabled={index === 0}
                className={cn(
                  'p-1 rounded hover:bg-gray-100 transition-colors',
                  index === 0 && 'opacity-30 cursor-not-allowed'
                )}
                title="Di chuyển lên"
              >
                <ChevronUp className="w-4 h-4 text-gray-500" />
              </button>
              <button
                type="button"
                onClick={onMoveDown}
                disabled={index === total - 1}
                className={cn(
                  'p-1 rounded hover:bg-gray-100 transition-colors',
                  index === total - 1 && 'opacity-30 cursor-not-allowed'
                )}
                title="Di chuyển xuống"
              >
                <ChevronDown className="w-4 h-4 text-gray-500" />
              </button>
              <button
                type="button"
                onClick={onRemove}
                disabled={total <= 1}
                className={cn(
                  'p-1 rounded hover:bg-red-50 transition-colors',
                  total <= 1 && 'opacity-30 cursor-not-allowed'
                )}
                title="Xóa phần này"
              >
                <Trash2 className="w-4 h-4 text-red-400" />
              </button>
            </>
          )}
        </div>
      </div>

      {/* Title */}
      <div className="space-y-1">
        <label className="text-xs font-medium text-gray-600">Tiêu đề</label>
        <Input
          value={section.title}
          onChange={e => onChange('title', e.target.value)}
          placeholder="VD: I. Nội dung cuộc họp"
          className="text-sm"
          disabled={disabled}
        />
      </div>

      {/* Instruction */}
      <div className="space-y-1">
        <label className="text-xs font-medium text-gray-600">Chỉ dẫn cho AI</label>
        <SectionInstructionEditor
          value={section.instruction}
          onChange={md => onChange('instruction', md)}
          disabled={disabled}
        />
      </div>

      {/* Format */}
      <div className="space-y-1">
        <label className="text-xs font-medium text-gray-600">Định dạng</label>
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

      {/* item_format — only when format=list */}
      {section.format === 'list' && (
        <div className="space-y-1">
          <label className="text-xs font-medium text-gray-600">Cột bảng</label>
          <p className="text-xs text-gray-400">
            Thêm các cột và đặt tiêu đề — AI sẽ điền nội dung theo cấu trúc bảng này.
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
