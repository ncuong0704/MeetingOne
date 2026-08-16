'use client';

import { useRef, useState, useCallback } from 'react';
import { toast } from 'sonner';
import { ModelConfig, ModelSettingsModal, ModelSettingsModalRef } from '@/components/ModelSettingsModal';
import { Switch } from './ui/switch';
import { useConfig } from '@/contexts/ConfigContext';
import { persistSummaryModelConfig } from '@/lib/summaryModelConfigSync';

export function SummaryModelSettings() {
  const { modelConfig, setModelConfig, isAutoSummary, toggleIsAutoSummary } = useConfig();
  const modelSettingsRef = useRef<ModelSettingsModalRef>(null);
  const [isSaving, setIsSaving] = useState(false);

  const handleSaveModelConfig = async (config: ModelConfig) => {
    try {
      await persistSummaryModelConfig(config);
      setModelConfig(config);
      toast.success('Đã lưu cài đặt mô hình');
    } catch (error) {
      console.error('Error saving model config:', error);
      toast.error('Không lưu được cài đặt mô hình');
    }
  };

  const handleSaveClick = useCallback(async () => {
    setIsSaving(true);
    const ok = await modelSettingsRef.current?.save();
    setIsSaving(false);
    if (ok === false) {
      toast.error('Vui lòng nhập API key và chọn model trước khi lưu');
    }
  }, []);

  return (
    <div className="max-w-xl space-y-6">
      <section>
        <h2 className="text-sm font-semibold text-ink tracking-tight">Tóm tắt tự động</h2>
        <p className="text-xs text-ink-2 mt-0.5 mb-2">
          Tạo tóm tắt sau khi cuộc họp kết thúc.
        </p>
        <div className="app-surface overflow-hidden">
          <div className="flex items-center justify-between gap-4 px-4 py-3">
            <div className="min-w-0">
              <p className="text-sm font-medium text-ink">Bật tóm tắt tự động</p>
              <p className="mt-0.5 text-xs text-ink-2">Tạo tóm tắt ngay khi dừng ghi âm</p>
            </div>
            <Switch checked={isAutoSummary} onCheckedChange={toggleIsAutoSummary} />
          </div>
        </div>
      </section>

      <section>
        <h2 className="text-sm font-semibold text-ink tracking-tight">Mô hình</h2>
        <p className="text-xs text-ink-2 mt-0.5 mb-2">
          Nhà cung cấp và mô hình dùng để tóm tắt.
        </p>
        <div className="app-surface overflow-hidden px-4 py-3 space-y-3">
          <ModelSettingsModal
            ref={modelSettingsRef}
            embedded
            modelConfig={modelConfig}
            setModelConfig={setModelConfig}
            onSave={handleSaveModelConfig}
            skipInitialFetch={true}
          />
          <div className="flex items-center pt-0.5">
            <button
              type="button"
              onClick={() => void handleSaveClick()}
              disabled={isSaving}
              className="inline-flex h-8 items-center rounded-md bg-primary px-2.5 text-xs font-medium text-primary-foreground hover:bg-primary-hover disabled:opacity-50"
            >
              {isSaving ? 'Đang lưu...' : 'Lưu'}
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}
