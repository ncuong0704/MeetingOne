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
import { GpuAPI, GpuSetupStatus } from '@/lib/asr';
import { openReleaseUrl } from '@/lib/appUpdate';
import { usePlatform } from '@/hooks/usePlatform';

const CUDA_ARCHIVE_URL_GENERIC = 'https://developer.nvidia.com/cuda-toolkit-archive';
const CUDNN_URL_GENERIC = 'https://developer.nvidia.com/cudnn';
// Link chốt phiên bản cụ thể, đã xác minh hoạt động (2026-08-06). NVIDIA có thể gỡ bản
// archive cũ theo thời gian — nếu link 404, cập nhật lại phiên bản mới nhất tại
// https://developer.nvidia.com/cuda-toolkit-archive và
// https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/
const CUDA_ARCHIVE_URL_WINDOWS =
  'https://developer.nvidia.com/cuda-12-9-1-download-archive?target_os=Windows&target_arch=x86_64&target_version=11&target_type=exe_local';
const CUDNN_URL_WINDOWS =
  'https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/cudnn-windows-x86_64-9.24.0.43_cuda12-archive.zip';

interface GpuSetupGuidanceProps {
  disabled?: boolean;
}

export default function GpuSetupGuidance({ disabled = false }: GpuSetupGuidanceProps) {
  const [checking, setChecking] = useState(false);
  const [status, setStatus] = useState<GpuSetupStatus | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const platform = usePlatform();
  const isWindows = platform === 'windows';

  const handleCheckGpu = async () => {
    setChecking(true);
    try {
      const result = await GpuAPI.checkSetupStatus();
      setStatus(result);
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

  const cudaUrl = isWindows ? CUDA_ARCHIVE_URL_WINDOWS : CUDA_ARCHIVE_URL_GENERIC;
  const cudnnUrl = isWindows ? CUDNN_URL_WINDOWS : CUDNN_URL_GENERIC;
  const needsCuda = status ? !status.hasCudaRuntime : false;
  const needsCudnn = status ? !status.hasCudnn : false;
  const isReady = status ? status.hasGpu && status.hasCudaRuntime && status.hasCudnn : false;

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
          {!status?.hasGpu ? (
            <DialogHeader>
              <DialogTitle>Không phát hiện GPU NVIDIA</DialogTitle>
              <DialogDescription>
                Máy này không có GPU NVIDIA (hoặc chưa cài driver). Ứng dụng sẽ tiếp tục chạy ở
                chế độ CPU đã được tối ưu — không cần thao tác gì thêm.
              </DialogDescription>
            </DialogHeader>
          ) : isReady ? (
            <DialogHeader>
              <DialogTitle>GPU đã sẵn sàng</DialogTitle>
              <DialogDescription>
                Đã phát hiện đủ CUDA Runtime và cuDNN cần thiết — không cần cài thêm gì.
              </DialogDescription>
            </DialogHeader>
          ) : (
            <>
              <DialogHeader>
                <DialogTitle>Đã phát hiện GPU NVIDIA</DialogTitle>
                <DialogDescription>
                  {needsCuda
                    ? 'Để bật tăng tốc GPU, ứng dụng cần đúng phiên bản CUDA runtime — không phải bản mới nhất trên trang chủ NVIDIA.'
                    : 'Đã có CUDA runtime — chỉ còn thiếu cuDNN 9.'}
                </DialogDescription>
              </DialogHeader>
              <div className="space-y-3 text-sm text-gray-700 dark:text-gray-300">
                {needsCuda && (
                  <p>
                    Cần cài <strong>CUDA 12.x Runtime</strong> (không phải CUDA 13 trở lên) và{' '}
                    <strong>cuDNN 9</strong>.
                  </p>
                )}
                {!needsCuda && needsCudnn && (
                  <p>
                    Chỉ cần cài thêm <strong>cuDNN 9</strong> — CUDA runtime đã có sẵn.
                  </p>
                )}
                <p className="text-xs text-gray-500 dark:text-gray-400">
                  cuDNN cần tài khoản NVIDIA Developer miễn phí để tải.
                </p>
                <p className="text-xs text-gray-500 dark:text-gray-400">
                  Cài xong, khởi động lại MeetingOne để áp dụng.
                </p>
              </div>
              <DialogFooter className="flex-col sm:flex-row gap-2">
                {needsCuda && (
                  <button
                    onClick={() => openReleaseUrl(cudaUrl)}
                    className="px-4 py-2 text-sm rounded-md bg-blue-600 hover:bg-blue-700 text-white font-medium transition-colors"
                  >
                    Tải CUDA Toolkit 12.x
                  </button>
                )}
                {needsCudnn && (
                  <button
                    onClick={() => openReleaseUrl(cudnnUrl)}
                    className="px-4 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 hover:bg-gray-50 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-200 font-medium transition-colors"
                  >
                    Tải cuDNN 9
                  </button>
                )}
                <button
                  onClick={handleRestart}
                  disabled={disabled}
                  className="px-4 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 hover:bg-gray-50 dark:hover:bg-gray-700 disabled:opacity-50 text-gray-700 dark:text-gray-200 font-medium transition-colors"
                >
                  Khởi động lại ngay
                </button>
              </DialogFooter>
            </>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
