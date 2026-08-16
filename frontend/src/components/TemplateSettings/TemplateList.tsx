'use client';

import React from 'react';
import { Plus, Star } from 'lucide-react';
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
    <section>
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-sm font-semibold text-ink tracking-tight">Danh sách mẫu</h2>
        <button
          type="button"
          onClick={onNew}
          className="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-2.5 text-xs font-medium text-primary-foreground hover:bg-primary-hover"
        >
          <Plus className="h-3.5 w-3.5" />
          Tạo mới
        </button>
      </div>
      <p className="text-xs text-ink-2 mt-0.5 mb-2">
        Chọn mẫu để sửa. Một mẫu mặc định dùng khi tạo tóm tắt.
      </p>
      <div className="app-surface overflow-hidden">
        {isLoading ? (
          <p className="px-4 py-3 text-xs text-ink-2">Đang tải...</p>
        ) : templates.length === 0 ? (
          <p className="px-4 py-3 text-xs text-ink-2">Chưa có mẫu nào</p>
        ) : (
          <ul className="divide-y divide-rule">
            {templates.map((t) => {
              const isActive = selectedId === t.id;
              const isDefault = defaultTemplateId === t.id;
              return (
                <li key={t.id} className="flex items-center gap-3 px-4 py-3">
                  <button
                    type="button"
                    onClick={() => onSelect(t.id)}
                    className={cn(
                      'min-w-0 flex-1 text-left rounded-md px-2 py-1 -mx-2 transition-colors',
                      isActive
                        ? 'bg-primary text-primary-foreground'
                        : 'hover:bg-secondary'
                    )}
                  >
                    <div className="flex items-center gap-1.5 min-w-0">
                      {isDefault && (
                        <Star className={cn(
                          'h-3 w-3 shrink-0 fill-current',
                          isActive ? 'text-primary-foreground' : 'text-primary'
                        )} />
                      )}
                      <span className={cn(
                        'text-sm font-medium truncate',
                        isActive ? 'text-primary-foreground' : 'text-ink'
                      )}>
                        {t.name}
                      </span>
                    </div>
                    {isDefault && (
                      <p className="text-xs mt-0.5 text-amber-600">
                        Mặc định
                      </p>
                    )}
                    <p className={cn(
                      'text-xs mt-0.5 line-clamp-2 leading-snug',
                      isActive ? 'text-primary-foreground/80' : 'text-ink-2'
                    )}>
                      {t.description}
                    </p>
                  </button>
                  {!isDefault && (
                    <button
                      type="button"
                      disabled={isSettingDefault}
                      onClick={() => onSetDefault(t.id, t.name)}
                      className="inline-flex h-8 shrink-0 items-center gap-1 rounded-md border border-rule bg-paper-2 px-2 text-xs font-medium text-ink-2 hover:bg-secondary hover:text-ink disabled:cursor-not-allowed disabled:opacity-50"
                      title="Đặt mặc định"
                    >
                      <Star className="h-3 w-3" />
                      Mặc định
                    </button>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </section>
  );
}
