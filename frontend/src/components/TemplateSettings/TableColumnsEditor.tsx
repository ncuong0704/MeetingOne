'use client';

import { useEffect, useState } from 'react';
import { Plus, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { buildItemFormat, parseTableColumns } from './tableItemFormat';

interface TableColumnsEditorProps {
  value: string;
  onChange: (itemFormat: string) => void;
  disabled?: boolean;
}

export function TableColumnsEditor({ value, onChange, disabled = false }: TableColumnsEditorProps) {
  const [columns, setColumns] = useState<string[]>(() => parseTableColumns(value));

  useEffect(() => {
    setColumns(parseTableColumns(value));
  }, [value]);

  const syncChange = (next: string[]) => {
    setColumns(next);
    onChange(buildItemFormat(next));
  };

  const addColumn = () => {
    setColumns(prev => [...prev, '']);
  };

  const updateColumn = (index: number, title: string) => {
    const next = [...columns];
    next[index] = title;
    syncChange(next);
  };

  const removeColumn = (index: number) => {
    syncChange(columns.filter((_, i) => i !== index));
  };

  return (
    <div className="space-y-2 rounded-md border border-rule bg-paper p-3">
      {columns.length === 0 ? (
        <p className="text-xs text-ink-2">
          Chưa có cột nào. Bấm &quot;Thêm cột&quot; để định nghĩa tiêu đề bảng.
        </p>
      ) : (
        <div className="space-y-2">
          {columns.map((column, index) => (
            <div key={index} className="flex items-center gap-2">
              <span className="text-xs text-ink-2 w-5 shrink-0 text-right">{index + 1}.</span>
              <Input
                value={column}
                onChange={e => updateColumn(index, e.target.value)}
                placeholder={`Tiêu đề cột ${index + 1}`}
                className="text-sm flex-1"
                disabled={disabled}
              />
              <button
                type="button"
                onClick={() => removeColumn(index)}
                disabled={disabled}
                className="p-1.5 rounded-md text-ink-2 hover:text-destructive hover:bg-destructive/10 disabled:opacity-40 transition-colors"
                title="Xóa cột"
              >
                <X className="w-3.5 h-3.5" />
              </button>
            </div>
          ))}
        </div>
      )}

      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={addColumn}
        disabled={disabled}
        className="text-xs h-8 gap-1"
      >
        <Plus className="w-3.5 h-3.5" />
        Thêm cột
      </Button>
    </div>
  );
}
