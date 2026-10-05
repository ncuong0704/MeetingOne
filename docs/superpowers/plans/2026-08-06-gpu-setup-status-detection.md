# Phát hiện trạng thái CUDA/cuDNN — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Thay `check_nvidia_gpu_available` (chỉ trả `bool`) bằng `check_gpu_setup_status` (trả `{hasGpu, hasCudaRuntime, hasCudnn}`), cập nhật dialog "Chạy GPU" hiển thị đúng 1 trong 4 trạng thái và chỉ hiện nút tải cho phần còn thiếu, dùng link tải trực tiếp (Windows x86_64, pre-chọn sẵn kiến trúc) thay vì link chung dễ chọn nhầm.

**Architecture:** Đổi Tauri command hiện có sang trả struct 3 trường (kiểm tra `nvidia-smi` + tìm 2 file DLL trên `PATH`) + đổi `GpuAPI` tương ứng + viết lại logic hiển thị trong `GpuSetupGuidance.tsx` theo trạng thái thay vì boolean đơn.

**Tech Stack:** Rust (`std::env::split_paths`, `serde` rename), TypeScript/React, `@tauri-apps/plugin-os` (đã dùng sẵn trong `appUpdate.ts`).

Spec đầy đủ: [docs/superpowers/specs/2026-08-06-gpu-setup-status-detection-design.md](../specs/2026-08-06-gpu-setup-status-detection-design.md)

---

### Task 1: Rust command `check_gpu_setup_status`

**Files:**
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] **Step 1: Viết test thất bại cho `find_dll_in_dirs`**

Ở cuối file `frontend/src-tauri/src/lib.rs` (dòng 691 hiện tại, sau toàn bộ code), thêm:

```rust
#[cfg(test)]
mod gpu_setup_status_tests {
    use super::find_dll_in_dirs;

    #[test]
    fn finds_dll_when_present_in_one_of_the_dirs() {
        let dir = std::env::temp_dir().join(format!("gpu-setup-test-found-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("cudart64_12.dll"), b"stub").unwrap();

        let missing_dir = std::env::temp_dir().join("gpu-setup-test-does-not-exist");
        let dirs = vec![missing_dir, dir.clone()].into_iter();

        assert!(find_dll_in_dirs("cudart64_12.dll", dirs));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn returns_false_when_dll_not_in_any_dir() {
        let dir_a = std::env::temp_dir().join("gpu-setup-test-a-does-not-exist");
        let dir_b = std::env::temp_dir().join("gpu-setup-test-b-does-not-exist");
        let dirs = vec![dir_a, dir_b].into_iter();

        assert!(!find_dll_in_dirs("cudnn64_9.dll", dirs));
    }
}
```

- [ ] **Step 2: Chạy test, xác nhận fail vì thiếu hàm**

Run: `cd frontend/src-tauri && cargo test gpu_setup_status_tests 2>&1 | tail -30`
Expected: lỗi biên dịch `cannot find function \`find_dll_in_dirs\` in this scope` (hàm chưa tồn tại).

- [ ] **Step 3: Xoá command cũ, thêm struct + hàm + command mới**

Tìm khối sau trong `frontend/src-tauri/src/lib.rs` (dòng 211-214 hiện tại):

```rust
#[tauri::command]
fn check_nvidia_gpu_available() -> bool {
    which::which("nvidia-smi").is_ok()
}
```

Thay bằng:

```rust
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct GpuSetupStatus {
    has_gpu: bool,
    has_cuda_runtime: bool,
    has_cudnn: bool,
}

#[tauri::command]
fn check_gpu_setup_status() -> GpuSetupStatus {
    let has_gpu = which::which("nvidia-smi").is_ok();
    let (has_cuda_runtime, has_cudnn) = if cfg!(target_os = "windows") {
        (
            dll_findable_on_path("cudart64_12.dll"),
            dll_findable_on_path("cudnn64_9.dll"),
        )
    } else {
        (false, false)
    };
    GpuSetupStatus {
        has_gpu,
        has_cuda_runtime,
        has_cudnn,
    }
}

fn find_dll_in_dirs(filename: &str, dirs: impl Iterator<Item = std::path::PathBuf>) -> bool {
    dirs.into_iter().any(|dir| dir.join(filename).is_file())
}

fn dll_findable_on_path(filename: &str) -> bool {
    match std::env::var_os("PATH") {
        Some(path) => find_dll_in_dirs(filename, std::env::split_paths(&path)),
        None => false,
    }
}
```

(`find_dll_in_dirs` nhận iterator để test được với thư mục giả, không phụ thuộc `PATH` thật của máy chạy test. `dll_findable_on_path` là lớp mỏng đọc `PATH` thật rồi gọi hàm thuần ở trên — không cần unit-test riêng lớp này, đã được `find_dll_in_dirs`'s test phủ phần logic.)

- [ ] **Step 4: Chạy test, xác nhận pass**

Run: `cd frontend/src-tauri && cargo test gpu_setup_status_tests 2>&1 | tail -20`
Expected: `test result: ok. 2 passed`.

- [ ] **Step 5: Cập nhật `generate_handler!`**

Tìm dòng `check_nvidia_gpu_available,` trong khối `tauri::generate_handler![` (dòng 463 hiện tại), đổi thành `check_gpu_setup_status,`.

- [ ] **Step 6: Build toàn bộ, xác nhận không lỗi**

Run: `cd frontend/src-tauri && cargo build --lib 2>&1 | tail -30`
Expected: build thành công, không `error[E...]`.

- [ ] **Step 7: Commit**

```bash
git add frontend/src-tauri/src/lib.rs
git commit -m "feat(gpu): detect CUDA runtime and cuDNN presence via PATH DLL search"
```

---

### Task 2: API wrapper TypeScript `GpuAPI.checkSetupStatus`

**Files:**
- Modify: `frontend/src/lib/asr.ts`

- [ ] **Step 1: Thay `GpuAPI` cũ**

Tìm khối sau (dòng 176-178 hiện tại):

```typescript
export const GpuAPI = {
  checkNvidiaAvailable: (): Promise<boolean> => invoke('check_nvidia_gpu_available'),
};
```

Thay bằng:

```typescript
export interface GpuSetupStatus {
  hasGpu: boolean;
  hasCudaRuntime: boolean;
  hasCudnn: boolean;
}

export const GpuAPI = {
  checkSetupStatus: (): Promise<GpuSetupStatus> => invoke('check_gpu_setup_status'),
};
```

- [ ] **Step 2: Kiểm tra type-check**

Run: `cd frontend && pnpm exec tsc --noEmit 2>&1 | head -40`
Expected: sẽ **có lỗi** ở `GpuSetupGuidance.tsx` (vẫn gọi `checkNvidiaAvailable` cũ) — đây là lỗi mong đợi ở bước này, Task 3 sẽ sửa. Xác nhận lỗi CHỈ ở `GpuSetupGuidance.tsx`, không ở file nào khác.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/asr.ts
git commit -m "feat(gpu): replace checkNvidiaAvailable with checkSetupStatus API"
```

---

### Task 3: Viết lại `GpuSetupGuidance.tsx` theo 4 trạng thái

**Files:**
- Modify: `frontend/src/components/GpuSetupGuidance.tsx`

- [ ] **Step 1: Thay toàn bộ nội dung file**

File hiện tại có nội dung (tham khảo để hiểu điểm khác biệt, không cần chép lại):
- Header `import`/`const URL` cũ dùng 1 cặp link chung, 1 state `hasGpu: boolean | null`.
- Dialog có 2 nhánh: `hasGpu` true/false.

Thay **toàn bộ** nội dung `frontend/src/components/GpuSetupGuidance.tsx` bằng:

```tsx
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
  const [isWindows, setIsWindows] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);

  const handleCheckGpu = async () => {
    setChecking(true);
    try {
      const result = await GpuAPI.checkSetupStatus();
      setStatus(result);
      const { platform } = await import('@tauri-apps/plugin-os');
      setIsWindows(platform() === 'windows');
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
```

**Lưu ý cho người triển khai**: đây là ghi đè toàn bộ file, không phải patch từng đoạn — dùng `Write` (không phải `Edit`) để tránh sai sót khi khớp chuỗi cũ/mới. Các phần giữ nguyên so với trước (nút trigger "Chạy GPU", nút "Khởi động lại ngay" với `disabled`, cách gọi `toast.error`, `openReleaseUrl`) đã được chép nguyên vẹn vào bản mới — so sánh kỹ để không làm mất hành vi đã có (đặc biệt là khoá nút khi `disabled=true`, đã fix ở spec trước để tránh mất dữ liệu ghi âm).

- [ ] **Step 2: Kiểm tra type-check**

Run: `cd frontend && pnpm exec tsc --noEmit 2>&1 | head -40`
Expected: không lỗi (lỗi từ Task 2 giờ đã hết vì `GpuSetupGuidance.tsx` đã dùng `checkSetupStatus`).

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/GpuSetupGuidance.tsx
git commit -m "feat(gpu): show 4-state dialog with direct download links and Windows-specific URLs"
```

---

### Task 4: Kiểm thử thủ công

**Files:** (không sửa file)

- [ ] **Step 1: Chạy app**

Run: `cd frontend && pnpm run tauri:dev:cuda` (dùng bản CUDA vì máy test hiện đã cài CUDA 12.9 — cần build có cờ `cuda` để không bị nhầm lẫn với các lần test CPU trước; command detection tự nó không cần cờ `cuda` nhưng dùng bản CUDA để nhất quán với trạng thái thật của máy).

- [ ] **Step 2: Xác nhận trạng thái "chỉ thiếu cuDNN" (trạng thái thật của máy hiện tại)**

Mở Settings → Transcription Models → Cấu hình chung → bấm "Chạy GPU".
Expected: dialog tiêu đề "Đã phát hiện GPU NVIDIA", mô tả "Đã có CUDA runtime — chỉ còn thiếu cuDNN 9.", **chỉ** nút "Tải cuDNN 9" + nút "Khởi động lại ngay" — **không** có nút "Tải CUDA Toolkit 12.x".

- [ ] **Step 3: Xác nhận nút cuDNN mở đúng link zip trực tiếp**

Bấm "Tải cuDNN 9".
Expected: trình duyệt mở `https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/cudnn-windows-x86_64-9.24.0.43_cuda12-archive.zip` (bắt đầu tải file zip ngay, không qua trang chọn phiên bản).

- [ ] **Step 4: Sau khi cài cuDNN đúng cách (copy DLL vào CUDA bin), test lại trạng thái "sẵn sàng"**

Copy các file `.dll` từ zip vừa tải vào `C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.9\bin\`, khởi động lại app (dùng nút "Khởi động lại ngay" hoặc tự đóng/mở), bấm "Chạy GPU" lại.
Expected: dialog tiêu đề "GPU đã sẵn sàng", không còn nút tải nào.

---

## Tổng kết task

| Task | File | Loại thay đổi |
|---|---|---|
| 1 | `frontend/src-tauri/src/lib.rs` | Thay `check_nvidia_gpu_available` bằng `check_gpu_setup_status` + test |
| 2 | `frontend/src/lib/asr.ts` | Thay `GpuAPI.checkNvidiaAvailable` bằng `checkSetupStatus` |
| 3 | `frontend/src/components/GpuSetupGuidance.tsx` | Viết lại dialog theo 4 trạng thái + link Windows trực tiếp |
| 4 | (không sửa file) | Kiểm thử thủ công trên máy đang ở đúng trạng thái "thiếu cuDNN" |
