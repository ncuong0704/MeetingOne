'use client';

import { useRef, useState, useCallback } from 'react';
import { toast } from 'sonner';
import { ModelConfig, ModelSettingsModal, ModelSettingsModalRef } from '@/components/ModelSettingsModal';
import { Switch } from './ui/switch';
import { Button } from './ui/button';
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
    <div className="flex flex-col gap-4">
      <div className="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
        <div className="px-5 py-4 border-b border-gray-50">
          <h3 className="text-base font-semibold text-gray-900">Tóm tắt tự động</h3>
          <p className="text-sm text-gray-500 mt-1">Tự động tạo tóm tắt sau khi kết thúc cuộc họp.</p>
        </div>
        <div className="flex items-center justify-between px-5 py-4">
          <div>
            <p className="text-base font-medium text-gray-800">Bật tóm tắt tự động</p>
            <p className="text-sm text-gray-500 mt-0.5">Tạo tóm tắt ngay khi dừng ghi âm</p>
          </div>
          <Switch checked={isAutoSummary} onCheckedChange={toggleIsAutoSummary} />
        </div>
      </div>

      <div className="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
        <div className="px-5 py-4 border-b border-gray-50">
          <h3 className="text-base font-semibold text-gray-900">Cấu hình mô hình tóm tắt</h3>
          <p className="text-sm text-gray-500 mt-1">Chọn mô hình AI dùng để tạo tóm tắt cuộc họp.</p>
        </div>
        <div className="px-5 py-5">
          <ModelSettingsModal
            ref={modelSettingsRef}
            embedded
            modelConfig={modelConfig}
            setModelConfig={setModelConfig}
            onSave={handleSaveModelConfig}
            skipInitialFetch={true}
          />
          <div className="mt-6 flex justify-end border-t border-gray-100 pt-4">
            <Button
              type="button"
              onClick={handleSaveClick}
              disabled={isSaving}
              className="bg-[#16478e] hover:bg-[#1a55ab] text-white px-4"
            >
              {isSaving ? 'Đang lưu…' : 'Lưu'}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
