# Nút "Chạy GPU" — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Thêm nút "Chạy GPU" vào Settings (mục "Cấu hình chung" trong tab Transcription Models) — kiểm tra máy có GPU NVIDIA (qua `nvidia-smi`), rồi mở dialog hướng dẫn cài đúng CUDA 12.x runtime + cuDNN 9 (không phải bản CUDA mới nhất).

**Architecture:** 1 Tauri command Rust mới (`check_nvidia_gpu_available`, dùng lại `which::which("nvidia-smi")` giống `build.rs`) + 1 API wrapper TypeScript (`GpuAPI` trong `lib/asr.ts`) + 1 component React mới (`GpuSetupGuidance.tsx`, dùng `Dialog` có sẵn) được nhúng vào `SharedTranscriptPanel.tsx`.

**Tech Stack:** Rust (Tauri command, crate `which` đã có sẵn runtime dependency), React/TypeScript, Radix Dialog (`components/ui/dialog.tsx`), Tailwind CSS.

Spec đầy đủ: [docs/superpowers/specs/2026-08-06-gpu-setup-guidance-button-design.md](../specs/2026-08-06-gpu-setup-guidance-button-design.md)

---

### Task 1: Tauri command `check_nvidia_gpu_available`

**Files:**
- Modify: `frontend/src-tauri/src/lib.rs`

- [x] **Step 1: Thêm command function**

Trong `frontend/src-tauri/src/lib.rs`, ngay sau hàm `is_recording` (kết thúc ở dòng 209 hiện tại):

```rust
async fn is_recording() -> bool {
    audio::recording_commands::is_recording().await
}

#[tauri::command]
fn check_nvidia_gpu_available() -> bool {
    which::which("nvidia-smi").is_ok()
}

#[tauri::command]
fn get_transcription_status() -> TranscriptionStatus {
```

(Chèn khối `check_nvidia_gpu_available` mới giữa hàm `is_recording` đã có và hàm `get_transcription_status` đã có — không đổi 2 hàm này.)

Không cần thêm `use which;` — gọi qua đường dẫn đầy đủ `which::which(...)` vì `which` đã là dependency của crate (`Cargo.toml`), chỉ dùng ở đúng một chỗ này trong `lib.rs`.

- [x] **Step 2: Đăng ký command trong `generate_handler!`**

Tìm khối `tauri::generate_handler![` (khoảng dòng 453), thêm `check_nvidia_gpu_available,` ngay sau `get_transcription_status,`:

```rust
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            is_recording,
            get_transcription_status,
            check_nvidia_gpu_available,
            analytics::commands::init_analytics,
```

- [x] **Step 3: Build để xác nhận không lỗi**

Run: `cd frontend/src-tauri && cargo build --lib 2>&1 | tail -30`
Expected: build thành công, không có `error[E...]` nào liên quan đến `check_nvidia_gpu_available` hay `generate_handler!`.

Ghi chú kiểm thử: hàm này chỉ gói 1 dòng gọi crate `which` (không có logic riêng để unit-test có ý nghĩa — cùng lý do các hàm dò phần cứng khác trong codebase, ví dụ `hardware_detector.rs`, cũng không có unit test). Xác minh bằng build sạch ở bước này + kiểm thử thủ công ở Task 5.

- [x] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/lib.rs
git commit -m "feat(gpu): add check_nvidia_gpu_available Tauri command"
```

---

### Task 2: API wrapper TypeScript `GpuAPI`

**Files:**
- Modify: `frontend/src/lib/asr.ts`

- [x] **Step 1: Thêm `GpuAPI` export**

Trong `frontend/src/lib/asr.ts`, ngay sau khối `CapuAPI` (kết thúc ở dòng 174 hiện tại):

```typescript
export const CapuAPI = {
  getCpuTopology: (): Promise<CpuTopology> => invoke('capu_get_cpu_topology'),
};

export const GpuAPI = {
  checkNvidiaAvailable: (): Promise<boolean> => invoke('check_nvidia_gpu_available'),
};
```

- [x] **Step 2: Kiểm tra type-check**

Run: `cd frontend && pnpm exec tsc --noEmit 2>&1 | head -30`
Expected: không có lỗi TypeScript mới liên quan đến `asr.ts` (lỗi có sẵn từ trước, nếu có, không tính).

- [x] **Step 3: Commit**

```bash
git add frontend/src/lib/asr.ts
git commit -m "feat(gpu): add GpuAPI.checkNvidiaAvailable wrapper"
```

---

### Task 3: Component `GpuSetupGuidance.tsx`

**Files:**
- Create: `frontend/src/components/GpuSetupGuidance.tsx`

- [x] **Step 1: Tạo component**

Tạo file `frontend/src/components/GpuSetupGuidance.tsx`:

```tsx
'use client';

import { useState } from 'react';
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

export default function GpuSetupGuidance() {
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
    } finally {
      setChecking(false);
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
          disabled={checking}
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
```

- [x] **Step 2: Kiểm tra type-check**

Run: `cd frontend && pnpm exec tsc --noEmit 2>&1 | head -30`
Expected: không có lỗi TypeScript trong `GpuSetupGuidance.tsx` (import `openReleaseUrl` từ `@/lib/appUpdate` và `GpuAPI` từ `@/lib/asr` phải resolve đúng — cả hai đã export từ Task 2 và đã tồn tại sẵn trong `appUpdate.ts`).

- [x] **Step 3: Commit**

```bash
git add frontend/src/components/GpuSetupGuidance.tsx
git commit -m "feat(gpu): add GpuSetupGuidance component with CUDA install dialog"
```

---

### Task 4: Nhúng vào `SharedTranscriptPanel.tsx`

**Files:**
- Modify: `frontend/src/components/SharedTranscriptPanel.tsx`

- [x] **Step 1: Thêm import**

Ở đầu file `frontend/src/components/SharedTranscriptPanel.tsx`, sau import `asrSettingsConstants` (dòng 10 hiện tại):

```typescript
import {
  DEFAULT_CAPU_CASE_LEVEL,
  DEFAULT_CAPU_PUNCTUATION_LEVEL,
  FALLBACK_PHYSICAL_CORES,
  levelLabel,
} from './asrSettingsConstants';
import GpuSetupGuidance from './GpuSetupGuidance';
```

- [x] **Step 2: Render component ngay sau khối tiêu đề "Cấu hình chung"**

Tìm khối JSX (dòng 70-78 hiện tại):

```tsx
  return (
    <div className="space-y-4 mt-6 pt-6 border-t border-gray-100 dark:border-gray-700">
      <div>
        <h4 className="text-sm font-semibold text-gray-900 dark:text-white">Cấu hình chung</h4>
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-1">
          Hotwords và CAPU dùng cho cả ghi âm trực tiếp và nhập file. Với ghi âm trực tiếp, dấu
          câu/viết hoa chỉ áp dụng sau khi kết thúc cuộc họp.
        </p>
      </div>

      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          Từ khóa ưu tiên (tên riêng, thuật ngữ chuyên ngành)
        </label>
```

Sửa thành (chèn `<GpuSetupGuidance />` sau khối tiêu đề, trước phần hotwords):

```tsx
  return (
    <div className="space-y-4 mt-6 pt-6 border-t border-gray-100 dark:border-gray-700">
      <div>
        <h4 className="text-sm font-semibold text-gray-900 dark:text-white">Cấu hình chung</h4>
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-1">
          Hotwords và CAPU dùng cho cả ghi âm trực tiếp và nhập file. Với ghi âm trực tiếp, dấu
          câu/viết hoa chỉ áp dụng sau khi kết thúc cuộc họp.
        </p>
      </div>

      <GpuSetupGuidance />

      <div className="space-y-2">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
          Từ khóa ưu tiên (tên riêng, thuật ngữ chuyên ngành)
        </label>
```

- [x] **Step 3: Kiểm tra type-check**

Run: `cd frontend && pnpm exec tsc --noEmit 2>&1 | head -30`
Expected: không có lỗi TypeScript.

- [x] **Step 4: Commit**

```bash
git add frontend/src/components/SharedTranscriptPanel.tsx
git commit -m "feat(gpu): wire GpuSetupGuidance into SharedTranscriptPanel"
```

---

### Task 5: Kiểm thử thủ công end-to-end

**Files:** (không sửa file, chỉ chạy app thật — theo CLAUDE.md: xác nhận UI bằng app thật, không chỉ code review)

- [x] **Step 1: Chạy app ở chế độ dev**

Run: `cd frontend && pnpm run tauri:dev:cpu`
Expected: app mở lên, không lỗi build.

(Dùng bản CPU vì `check_nvidia_gpu_available` không phụ thuộc cờ `cuda` — build nhanh hơn để test riêng tính năng UI này.)

- [x] **Step 2: Kiểm tra nút "Chạy GPU" xuất hiện đúng chỗ**

Mở app → Settings → tab "Transcription Models" → cuộn xuống mục "Cấu hình chung".
Expected: thấy dòng "Tăng tốc GPU (NVIDIA)" với nút "Chạy GPU" bên phải, nằm ngay dưới phần mô tả "Cấu hình chung", phía trên ô "Từ khóa ưu tiên".

- [x] **Step 3: Bấm nút, kiểm tra dialog**

Bấm "Chạy GPU".
Expected (máy có GPU NVIDIA — máy dev hiện tại có GPU): dialog "Đã phát hiện GPU NVIDIA" hiện ra, có đoạn text nhắc CUDA 12.x + cuDNN 9, và 2 nút "Tải CUDA Toolkit 12.x" / "Tải cuDNN 9".

- [x] **Step 4: Kiểm tra 2 nút mở đúng link**

Bấm "Tải CUDA Toolkit 12.x".
Expected: trình duyệt mặc định mở `https://developer.nvidia.com/cuda-toolkit-archive`.

Bấm "Tải cuDNN 9" (mở dialog lại nếu đã đóng).
Expected: trình duyệt mặc định mở `https://developer.nvidia.com/cudnn`.

- [x] **Step 5: Kiểm tra không bị kẹt trạng thái**

Bấm nút "Chạy GPU" nhanh 2 lần liên tiếp.
Expected: nút hiện "Đang kiểm tra..." rồi trở lại "Chạy GPU" bình thường, không bị kẹt ở trạng thái disabled, dialog vẫn mở đúng 1 lần.

- [x] **Step 6: Đóng dialog bằng nút X và bằng click ra ngoài**

Expected: cả 2 cách đều đóng dialog được (hành vi mặc định của Radix Dialog, không cần code thêm).

---

### Task 6: Nút "Khởi động lại ngay" (bổ sung sau khi review Task 3-4)

**Bối cảnh:** Trong lúc kiểm thử thủ công Task 5, phát hiện dialog chỉ ghi text "Cài xong, khởi động lại MeetingOne để áp dụng" nhưng không có cách nào giúp user làm việc đó — phải tự đóng app (Alt+F4 / nút X) rồi mở lại thủ công. Codebase đã có sẵn cơ chế relaunch dùng cho luồng cập nhật app (`frontend/src/lib/appUpdate.ts:138-139`, dùng `@tauri-apps/plugin-process`'s `relaunch()`), nên tái dùng thay vì bắt user tự thao tác.

**Files:**
- Modify: `frontend/src/components/GpuSetupGuidance.tsx`

- [x] **Step 1: Thêm hàm `handleRestart` và nút trong dialog**

Trong `frontend/src/components/GpuSetupGuidance.tsx`, thêm hàm sau, đặt ngay sau `handleCheckGpu`:

```typescript
  const handleRestart = async () => {
    const { relaunch } = await import('@tauri-apps/plugin-process');
    await relaunch();
  };
```

Sau đó, trong `<DialogFooter>` (nhánh `hasGpu` true), thêm nút thứ ba sau nút "Tải cuDNN 9":

```tsx
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
```

Không cần try/catch quanh `relaunch()` — nếu gọi thành công, tiến trình app kết thúc gần như ngay lập tức (không còn code nào chạy tiếp để xử lý lỗi có ý nghĩa); đây cũng là cách `appUpdate.ts:138-139` đang gọi (không try/catch riêng cho `relaunch()`).

- [x] **Step 2: Kiểm tra type-check**

Run: `cd frontend && pnpm exec tsc --noEmit 2>&1 | head -30`
Expected: không lỗi. `@tauri-apps/plugin-process` đã là dependency có sẵn (đã dùng ở `appUpdate.ts`), không cần cài thêm.

- [x] **Step 3: Commit**

```bash
git add frontend/src/components/GpuSetupGuidance.tsx
git commit -m "feat(gpu): add restart-now button to GPU setup dialog"
```

- [x] **Step 4: Kiểm thử thủ công**

Chạy app (`pnpm run tauri:dev:cpu`), mở dialog "Chạy GPU" (trường hợp có GPU), bấm "Khởi động lại ngay".
Expected: app đóng và tự mở lại (cửa sổ biến mất rồi xuất hiện lại sau vài giây).

---

### Task 7: Khóa nút khi đang ghi âm (bổ sung sau final review)

**Bối cảnh:** Final code review phát hiện: `SharedTranscriptPanel.tsx` đã nhận prop `disabled` từ `AsrPathTabs.tsx` (`disabled={isRecording}`, xem `AsrPathTabs.tsx:28,51`) và áp dụng cho MỌI control khác trong panel (textarea, sliders, nút lưu) — nhưng `GpuSetupGuidance` được nhúng ở Task 4 mà KHÔNG nhận prop này. Hậu quả: user có thể mở Settings trong lúc đang ghi âm (Settings vẫn mở được khi ghi âm — đó là lý do panel khác phải tự disable), bấm "Chạy GPU" → "Khởi động lại ngay" → app relaunch ngay lập tức, làm gián đoạn/mất bản ghi đang chạy, không có cảnh báo nào. Cần nối `GpuSetupGuidance` vào cùng cơ chế `disabled` mà các control khác trong file đã dùng, cho nhất quán.

**Files:**
- Modify: `frontend/src/components/GpuSetupGuidance.tsx`
- Modify: `frontend/src/components/SharedTranscriptPanel.tsx`

- [x] **Step 1: Thêm prop `disabled` vào `GpuSetupGuidance`**

Trong `frontend/src/components/GpuSetupGuidance.tsx`, thêm interface props và áp dụng `disabled` cho nút "Chạy GPU" (giống cách các control khác trong `SharedTranscriptPanel.tsx` bị khóa toàn bộ khi đang ghi âm, không chỉ riêng nút restart):

```tsx
interface GpuSetupGuidanceProps {
  disabled?: boolean;
}

export default function GpuSetupGuidance({ disabled = false }: GpuSetupGuidanceProps) {
```

Sửa nút "Chạy GPU" (nút trigger, không phải nút trong dialog) từ:
```tsx
        <button
          onClick={handleCheckGpu}
          disabled={checking}
          className="px-4 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 hover:bg-gray-50 dark:hover:bg-gray-700 disabled:opacity-50 text-gray-700 dark:text-gray-200 font-medium transition-colors whitespace-nowrap"
        >
```
thành:
```tsx
        <button
          onClick={handleCheckGpu}
          disabled={checking || disabled}
          className="px-4 py-2 text-sm rounded-md border border-gray-300 dark:border-gray-600 hover:bg-gray-50 dark:hover:bg-gray-700 disabled:opacity-50 text-gray-700 dark:text-gray-200 font-medium transition-colors whitespace-nowrap"
        >
```

(Panel đã có sẵn banner "Không thể thay đổi mô hình khi đang ghi âm" ở cấp `AsrPathTabs.tsx` khi `isRecording`, nên không cần thêm text giải thích riêng ở đây — chỉ cần disable nút để nhất quán với các control khác.)

- [x] **Step 2: Truyền `disabled` từ `SharedTranscriptPanel.tsx`**

Trong `frontend/src/components/SharedTranscriptPanel.tsx`, sửa:
```tsx
      <GpuSetupGuidance />
```
thành:
```tsx
      <GpuSetupGuidance disabled={disabled} />
```

(`disabled` đã là prop có sẵn của `SharedTranscriptPanel` — xem `SharedTranscriptPanelProps` — chỉ cần truyền tiếp xuống, không cần đổi gì khác trong file.)

- [x] **Step 3: Kiểm tra type-check**

Run: `cd frontend && pnpm exec tsc --noEmit 2>&1 | head -30`
Expected: không lỗi.

- [x] **Step 4: Commit**

```bash
git add frontend/src/components/GpuSetupGuidance.tsx frontend/src/components/SharedTranscriptPanel.tsx
git commit -m "fix(gpu): disable GPU setup button while recording"
```

---

## Tổng kết task

| Task | File | Loại thay đổi |
|---|---|---|
| 1 | `frontend/src-tauri/src/lib.rs` | Thêm Tauri command + đăng ký |
| 2 | `frontend/src/lib/asr.ts` | Thêm `GpuAPI` wrapper |
| 3 | `frontend/src/components/GpuSetupGuidance.tsx` | File mới — component + dialog |
| 4 | `frontend/src/components/SharedTranscriptPanel.tsx` | Nhúng component |
| 5 | (không sửa file) | Kiểm thử thủ công end-to-end |
| 6 | `frontend/src/components/GpuSetupGuidance.tsx` | Thêm nút "Khởi động lại ngay" (relaunch) |
| 7 | `GpuSetupGuidance.tsx` + `SharedTranscriptPanel.tsx` | Khóa nút khi đang ghi âm |
