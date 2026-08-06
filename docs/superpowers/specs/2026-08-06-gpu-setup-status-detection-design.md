# Phát hiện trạng thái cài đặt CUDA/cuDNN + link tải trực tiếp đúng phiên bản

## Vấn đề

Spec trước ([2026-08-06-gpu-setup-guidance-button-design.md](2026-08-06-gpu-setup-guidance-button-design.md)) thêm nút "Chạy GPU" hướng dẫn cài CUDA 12.x + cuDNN 9, nhưng dùng link chung chung tới trang chủ NVIDIA (`cuda-toolkit-archive`, `developer.nvidia.com/cudnn`) — người dùng phải tự chọn đúng OS/kiến trúc/phiên bản.

Thử nghiệm thực tế phát hiện 2 vấn đề:

1. **Chọn nhầm kiến trúc**: người dùng tải nhầm bản cuDNN cho **Windows ARM64** thay vì **x86_64** (máy này là x64) — do trang chọn phiên bản của NVIDIA dễ gây nhầm lẫn. Cài xong vẫn không có `cudnn64_9.dll` nào trên máy, GPU vẫn fallback CPU y hệt trước khi cài.
2. **Không biết đã cài gì rồi**: dialog luôn hiển thị "cần cài CUDA + cuDNN" dù thực tế máy này đã có sẵn CUDA 12.9 đúng chuẩn (xác nhận qua `cublas64_12.dll`, `cudart64_12.dll`... đã có ở `CUDA\v12.9\bin\`) — chỉ thiếu cuDNN. Người dùng không biết phần nào đã xong, phần nào còn thiếu, phải tự đoán.

## Ngoài phạm vi

- **Không** kiểm tra sâu "CUDA có thực sự chạy được" (không thử load ONNX Runtime CUDA provider thật) — chỉ kiểm tra sự tồn tại file DLL trên PATH, theo đúng quyết định giữ đơn giản từ spec trước.
- **Không** làm link trực tiếp cho Linux/macOS trong spec này — chỉ Windows x86_64 (nơi đã test/xác minh). Các OS khác giữ nguyên hành vi cũ (2 nút, link chung).
- **Không** tự động tải/cài CUDA hay cuDNN thay người dùng — vẫn chỉ mở link, người dùng tự tải & cài (giữ nguyên nguyên tắc từ spec trước: không tự ý thay đổi hệ thống).
- **Không** giữ lại API `check_nvidia_gpu_available` riêng — thay thế hoàn toàn bằng command mới `check_gpu_setup_status` trả về đủ cả 3 thông tin trong 1 lần gọi (tính năng còn rất mới, chưa release, đổi contract không rủi ro).

## Nghiên cứu: link tải trực tiếp

Xác minh qua WebFetch/WebSearch (không đoán URL):

- **CUDA Toolkit 12.9.1, Windows x86_64, local installer** — trang landing đã pre-chọn sẵn OS/kiến trúc/loại cài đặt qua query string, chỉ còn 1 click để tải, không cần đăng nhập:
  `https://developer.nvidia.com/cuda-12-9-1-download-archive?target_os=Windows&target_arch=x86_64&target_version=11&target_type=exe_local`
- **cuDNN 9.24.0.43 (build cho CUDA 12), Windows x86_64** — link file zip trực tiếp từ kho redistributable công khai của NVIDIA, không cần đăng nhập (xác nhận tồn tại qua WebFetch, ~1.8GB, cập nhật 2026-07-02):
  `https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/cudnn-windows-x86_64-9.24.0.43_cuda12-archive.zip`

Cả 2 link đều **chốt phiên bản cụ thể** (đã xác minh hoạt động đúng trên máy test hôm nay). Rủi ro: NVIDIA có thể gỡ các bản archive cũ theo thời gian — cần review/cập nhật link định kỳ nếu phát hiện lỗi 404. Chấp nhận rủi ro này vì lợi ích (tránh chọn nhầm kiến trúc) lớn hơn.

## Thiết kế

### 1. Backend — command `check_gpu_setup_status` (thay thế `check_nvidia_gpu_available`)

File: `frontend/src-tauri/src/lib.rs`

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
        (dll_findable_on_path("cudart64_12.dll"), dll_findable_on_path("cudnn64_9.dll"))
    } else {
        (false, false)
    };
    GpuSetupStatus { has_gpu, has_cuda_runtime, has_cudnn }
}

fn dll_findable_on_path(filename: &str) -> bool {
    std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path).any(|dir| dir.join(filename).is_file())
        })
        .unwrap_or(false)
}
```

**Vì sao không dùng crate `which` cho DLL**: `which::which()` được thiết kế để tìm file thực thi, áp dụng luật mở rộng `PATHEXT` trên Windows (tự thêm `.exe`/`.bat`/`.cmd`...) — không đảm bảo khớp chính xác tên file `.dll` được truyền vào. `std::env::split_paths` + kiểm tra `is_file()` trực tiếp là cách rõ ràng, đúng ý định, không phụ thuộc thêm crate.

Xoá hàm `check_nvidia_gpu_available` cũ (Cargo/Tauri sẽ báo lỗi biên dịch ở registration list nếu quên xoá — dễ phát hiện).

### 2. Frontend — `GpuAPI`

File: `frontend/src/lib/asr.ts`

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

(Thay thế `checkNvidiaAvailable`. Nhờ `#[serde(rename_all = "camelCase")]` trên struct Rust ở mục 1, JSON trả về đã sẵn `hasGpu`/`hasCudaRuntime`/`hasCudnn` — interface TS ở trên khớp thẳng, không cần logic map thủ công.)

### 3. Frontend — `GpuSetupGuidance.tsx`: 4 trạng thái

```tsx
const [status, setStatus] = useState<GpuSetupStatus | null>(null);
// ...
const result = await GpuAPI.checkSetupStatus();
setStatus(result);
setDialogOpen(true);
```

Nội dung dialog theo `status`:

| Điều kiện | Tiêu đề | Nội dung | Nút |
|---|---|---|---|
| `!hasGpu` | "Không phát hiện GPU NVIDIA" | Như cũ | (không có) |
| `hasGpu && hasCudaRuntime && hasCudnn` | "GPU đã sẵn sàng" | "✅ Đã cài đủ CUDA + cuDNN, không cần làm gì thêm." | (không có) |
| `hasGpu && !hasCudaRuntime` | "Đã phát hiện GPU NVIDIA" | Như cũ (cần CUDA 12.x + cuDNN 9) | Tải CUDA + Tải cuDNN + Khởi động lại |
| `hasGpu && hasCudaRuntime && !hasCudnn` | "Đã phát hiện GPU NVIDIA" | "Đã có CUDA runtime — chỉ còn thiếu cuDNN 9." | **Chỉ** Tải cuDNN + Khởi động lại |

Nút "Khởi động lại ngay" giữ nguyên logic khoá khi `disabled` (đang ghi âm) từ spec trước — áp dụng cho mọi trạng thái có nút.

### 4. Windows-only cho link trực tiếp

```typescript
const CUDA_ARCHIVE_URL_GENERIC = 'https://developer.nvidia.com/cuda-toolkit-archive';
const CUDNN_URL_GENERIC = 'https://developer.nvidia.com/cudnn';
const CUDA_ARCHIVE_URL_WINDOWS = 'https://developer.nvidia.com/cuda-12-9-1-download-archive?target_os=Windows&target_arch=x86_64&target_version=11&target_type=exe_local';
const CUDNN_URL_WINDOWS = 'https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/cudnn-windows-x86_64-9.24.0.43_cuda12-archive.zip';
```

Dùng `@tauri-apps/plugin-os`'s `platform()` (đã dùng sẵn ở `appUpdate.ts:134`) để chọn URL — `platform() === 'windows'` dùng link trực tiếp, ngược lại dùng link chung.

**Trên non-Windows**: vì `check_gpu_setup_status` luôn trả `has_cuda_runtime: false, has_cudnn: false` (theo thiết kế mục 1), dialog sẽ luôn rơi vào nhánh "cần cài CUDA + cuDNN" (hiện cả 2 nút, dùng link chung) — giữ đúng hành vi 2-trạng-thái cũ, không hiển thị thông tin sai lệch kiểu "đã sẵn sàng" hay "chỉ thiếu cuDNN" mà chưa thực sự kiểm tra được trên nền tảng đó.

## Kiểm thử

- **Rust**: `dll_findable_on_path` là hàm thuần (pure function nhận `&str`, không đọc `PATH` thật) nếu tách riêng phần "tìm trong danh sách thư mục" khỏi phần "đọc biến môi trường" — cân nhắc refactor nhỏ để unit-test được: `fn find_dll_in_dirs(filename: &str, dirs: impl Iterator<Item = PathBuf>) -> bool`, rồi `dll_findable_on_path` chỉ gọi hàm này với `std::env::split_paths(...)`. Viết test với thư mục giả (tempdir) chứa/không chứa file để xác nhận logic đúng, theo TDD.
- **Manual**: máy test hiện tại (đã cài CUDA 12.9, thiếu cuDNN) là ca kiểm thử thực tế sẵn có cho nhánh "chỉ thiếu cuDNN" — bấm "Chạy GPU" phải chỉ hiện nút "Tải cuDNN 9", không hiện nút CUDA.

## Tiêu chí hoàn thành

- [ ] `check_gpu_setup_status` trả đúng cả 3 trường, build sạch.
- [ ] Dialog hiển thị đúng 4 trạng thái, verify bằng máy test thật (đang ở đúng trạng thái "có CUDA, thiếu cuDNN").
- [ ] Link Windows trỏ đúng, chỉ dùng khi `platform() === 'windows'`.
- [ ] Nút restart vẫn khoá đúng khi đang ghi âm (không regress spec trước).
- [ ] `cargo test` (bao gồm test mới cho `find_dll_in_dirs`) + `tsc --noEmit` sạch.

## Tài liệu liên quan

- [2026-08-06-gpu-setup-guidance-button-design.md](2026-08-06-gpu-setup-guidance-button-design.md) — spec gốc của nút "Chạy GPU", bị thay thế một phần bởi spec này (command, dialog logic).
