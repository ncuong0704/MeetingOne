'use client';

import React from 'react';
import { Plus, Star } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import type { TemplateInfo } from './types';
import { TOUR_TARGETS } from '@/components/UserGuide/tourTargets';
import { BUILTIN_ACT_TEMPLATE_ID } from '@/components/UserGuide/templateTourNavigation';

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
  const firstNonDefaultId = templates.find((t) => t.id !== defaultTemplateId)?.id;
  const firstClonedId = templates.find((t) => t.name.includes('(bản sao)'))?.id;

  return (
    <div className="flex flex-col w-full min-h-0 border border-gray-200 rounded-xl overflow-hidden bg-white">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-gray-100">
        <span
          className="text-sm font-semibold text-gray-700"
          data-tour={
            templates.length > 0 && !firstNonDefaultId
              ? TOUR_TARGETS.SETTINGS_TEMPLATE_SET_DEFAULT
              : undefined
          }
        >
          Danh sách mẫu
        </span>
        <Button
          variant="blue"
          size="sm"
          onClick={onNew}
          data-tour={TOUR_TARGETS.TEMPLATE_CREATE}
          className="h-7 px-2 text-xs gap-1"
        >
          <Plus className="w-3.5 h-3.5" />
          Tạo mới
        </Button>
      </div>

      {/* List */}
      <div className="flex-1 min-h-0 overflow-y-auto">
        {isLoading ? (
          <div className="px-4 py-8 text-center text-sm text-gray-400">Đang tải...</div>
        ) : templates.length === 0 ? (
          <div
            className="px-4 py-8 text-center text-sm text-gray-400"
            data-tour={TOUR_TARGETS.SETTINGS_TEMPLATE_ITEM}
          >
            Chưa có mẫu nào
          </div>
        ) : (
          <div className="p-2 space-y-1">
            {templates.map((t, index) => {
              const isActive = selectedId === t.id;
              const isDefault = defaultTemplateId === t.id;
              return (
                <div
                  key={t.id}
                  className="group/item relative"
                  data-tour={
                    t.id === firstNonDefaultId
                      ? TOUR_TARGETS.SETTINGS_TEMPLATE_SET_DEFAULT
                      : undefined
                  }
                >
                  <button
                    type="button"
                    onClick={() => onSelect(t.id)}
                    data-tour={
                      t.id === BUILTIN_ACT_TEMPLATE_ID
                        ? TOUR_TARGETS.TEMPLATE_BUILTIN_ACT
                        : t.id === firstClonedId
                          ? TOUR_TARGETS.TEMPLATE_CLONED_ITEM
                          : index === 0
                            ? TOUR_TARGETS.SETTINGS_TEMPLATE_ITEM
                            : undefined
                    }
                    className={cn(
                      'w-full text-left px-3 py-2.5 rounded-lg transition-colors',
                      isActive
                        ? 'bg-blue-50 border border-[#16478e]/30'
                        : 'hover:bg-gray-50 border border-transparent'
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
                            isActive ? 'text-[#16478e]' : 'text-gray-800'
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
                    <p className="text-xs text-gray-400 mt-0.5 line-clamp-2 leading-relaxed">
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
                        'text-[10px] px-1.5 py-0.5 rounded-full border border-gray-200',
                        'bg-white text-gray-500 hover:text-amber-600 hover:border-amber-300',
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
