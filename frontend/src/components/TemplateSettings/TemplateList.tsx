'use client';

import React from 'react';
import { Plus, Star } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import type { TemplateInfo } from './types';

interface TemplateListProps {
  templates: TemplateInfo[];
  selectedId: string | null;
  defaultTemplateId: string;
  isSettingDefault: boolean;
  isLoading: boolean;
  onSelect: (id: string) => void;
  onNew: () => void;
  onSetDefault: (id: string, name: string) => void;
}

export function TemplateList({
  templates,
  selectedId,
  defaultTemplateId,
  isSettingDefault,
  isLoading,
  onSelect,
  onNew,
  onSetDefault,
}: TemplateListProps) {
  return (
    <div className="flex flex-col w-full min-h-0 app-surface overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-rule">
        <span className="text-sm font-semibold text-ink">
          Danh sách mẫu
        </span>
        <Button
          variant="blue"
          size="sm"
          onClick={onNew}
          className="h-7 px-2 text-xs gap-1"
        >
          <Plus className="w-3.5 h-3.5" />
          Tạo mới
        </Button>
      </div>

      {/* List */}
      <div className="flex-1 min-h-0 overflow-y-auto">
        {isLoading ? (
          <div className="px-4 py-8 text-center text-sm text-ink-2">Đang tải...</div>
        ) : templates.length === 0 ? (
          <div className="px-4 py-8 text-center text-sm text-ink-2">
            Chưa có mẫu nào
          </div>
        ) : (
          <div className="p-2 space-y-1">
            {templates.map((t) => {
              const isActive = selectedId === t.id;
              const isDefault = defaultTemplateId === t.id;
              return (
                <div key={t.id} className="group/item relative">
                  <button
                    type="button"
                    onClick={() => onSelect(t.id)}
                    className={cn(
                      'w-full text-left px-3 py-2.5 rounded-lg transition-colors',
                      isActive
                        ? 'bg-primary/10 border border-primary/30'
                        : 'hover:bg-secondary border border-transparent'
                    )}
                  >
                    <div className="flex items-start justify-between gap-2">
                      <div className="flex items-center gap-1.5 min-w-0">
                        {isDefault && (
                          <Star className="w-3 h-3 text-amber-500 fill-amber-400 shrink-0" />
                        )}
                        <span
                          className={cn(
                            'text-sm font-medium leading-snug truncate',
                            isActive ? 'text-primary' : 'text-ink'
                          )}
                        >
                          {t.name}
                        </span>
                      </div>
                    </div>
                    {isDefault && (
                      <p className="text-[10px] text-amber-600 font-medium mt-0.5">
                        Đang dùng làm mặc định
                      </p>
                    )}
                    <p className="text-xs text-ink-2 mt-0.5 line-clamp-2 leading-relaxed">
                      {t.description}
                    </p>
                  </button>

                  {/* Set-as-default button — shown on hover */}
                  {!isDefault && (
                    <button
                      type="button"
                      disabled={isSettingDefault}
                      onClick={e => {
                        e.stopPropagation();
                        onSetDefault(t.id, t.name);
                      }}
                      className={cn(
                        'absolute right-2 bottom-2 opacity-0 group-hover/item:opacity-100 transition-opacity z-10',
                        'text-[10px] px-1.5 py-0.5 rounded-md border border-rule',
                        'bg-paper-2 text-ink-2 hover:text-amber-700 hover:border-amber-300',
                        'flex items-center gap-1 whitespace-nowrap',
                        isSettingDefault && 'cursor-not-allowed'
                      )}
                      title="Đặt làm mẫu mặc định"
                    >
                      <Star className="w-2.5 h-2.5" />
                      Đặt mặc định
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
