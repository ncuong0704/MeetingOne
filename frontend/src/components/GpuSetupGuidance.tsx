'use client';

import { useState } from 'react';
import { toast } from 'sonner';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { GpuAPI } from '@/lib/asr';
import { openReleaseUrl } from '@/lib/appUpdate';

const CUDA_ARCHIVE_URL = 'https://developer.nvidia.com/cuda-toolkit-archive';
const CUDNN_URL = 'https://developer.nvidia.com/cudnn';

interface GpuSetupGuidanceProps {
  disabled?: boolean;
}

export default function GpuSetupGuidance({ disabled = false }: GpuSetupGuidanceProps) {
  const [checking, setChecking] = useState(false);
  const [hasGpu, setHasGpu] = useState<boolean | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);

  const handleCheckGpu = async () => {
    setChecking(true);
    try {
      const result = await GpuAPI.checkNvidiaAvailable();
      setHasGpu(result);
      setDialogOpen(true);
    } catch (e) {
      console.error('Failed to check GPU:', e);
      toast.error('Không kiểm tra được GPU');
    } finally {
      setChecking(false);
    }
  };

  const handleRestart = async () => {
    try {
      const { relaunch } = await import('@tauri-apps/plugin-process');
      await relaunch();
    } catch (e) {
      console.error('Failed to restart app:', e);
      toast.error('Không khởi động lại được ứng dụng');
    }
  };

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-4">
        <div>
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            Tăng tốc GPU (NVIDIA)
          </label>
          <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
            Kiểm tra máy có GPU NVIDIA và xem hướng dẫn bật tăng tốc.
          </p>
        </div>
        <button
          onClick={handleCheckGpu}
          disabled={checking || disabled}
          className="px-4 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 hover:bg-gray-50 dark:hover:bg-gray-700 disabled:opacity-50 text-gray-700 dark:text-gray-200 font-medium transition-colors whitespace-nowrap"
        >
          {checking ? 'Đang kiểm tra...' : 'Chạy GPU'}
        </button>
      </div>

      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent className="max-w-lg">
          {hasGpu ? (
            <>
              <DialogHeader>
                <DialogTitle>Đã phát hiện GPU NVIDIA</DialogTitle>
                <DialogDescription>
                  Để bật tăng tốc GPU, ứng dụng cần đúng phiên bản CUDA runtime — không phải bản
                  mới nhất trên trang chủ NVIDIA.
                </DialogDescription>
              </DialogHeader>
              <div className="space-y-3 text-sm text-gray-700 dark:text-gray-300">
                <p>
                  Cần cài <strong>CUDA 12.x Runtime</strong> (không phải CUDA 13 trở lên) và{' '}
                  <strong>cuDNN 9</strong>. Vào trang CUDA Toolkit Archive và chọn một bản trong
                  dòng 12.x (ví dụ 12.6) — không dùng bản mới nhất trên trang chủ.
                </p>
                <p className="text-xs text-gray-500 dark:text-gray-400">
                  cuDNN cần tài khoản NVIDIA Developer miễn phí để tải.
                </p>
                <p className="text-xs text-gray-500 dark:text-gray-400">
                  Cài xong, khởi động lại MeetingOne để áp dụng.
                </p>
              </div>
              <DialogFooter className="flex-col sm:flex-row gap-2">
                <button
                  onClick={() => openReleaseUrl(CUDA_ARCHIVE_URL)}
                  className="px-4 py-2 text-sm rounded-md bg-blue-600 hover:bg-blue-700 text-white font-medium transition-colors"
                >
                  Tải CUDA Toolkit 12.x
                </button>
                <button
                  onClick={() => openReleaseUrl(CUDNN_URL)}
                  className="px-4 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 hover:bg-gray-50 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-200 font-medium transition-colors"
                >
                  Tải cuDNN 9
                </button>
                <button
                  onClick={handleRestart}
                  className="px-4 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 hover:bg-gray-50 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-200 font-medium transition-colors"
                >
                  Khởi động lại ngay
                </button>
              </DialogFooter>
            </>
          ) : (
            <DialogHeader>
              <DialogTitle>Không phát hiện GPU NVIDIA</DialogTitle>
              <DialogDescription>
                Máy này không có GPU NVIDIA (hoặc chưa cài driver). Ứng dụng sẽ tiếp tục chạy ở
                chế độ CPU đã được tối ưu — không cần thao tác gì thêm.
              </DialogDescription>
            </DialogHeader>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
