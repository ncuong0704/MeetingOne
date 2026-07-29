# Tab Mẫu (Settings): layout một-panel + BlockNote cho chỉ dẫn AI

## Vấn đề

Tab Mẫu trong trang Cài đặt (`TemplateSettings/index.tsx`) hiện hiển thị `TemplateList` (280px)
và `TemplateEditor` (flex-1) cạnh nhau, luôn chiếm chỗ của nhau kể cả khi không cần xem chi tiết.

Ngoài ra, ô "Chỉ dẫn cho AI" trong mỗi phần (`SectionEditor.tsx`) là một `Textarea` thuần —
người dùng phải tự gõ toàn bộ nội dung dạng văn bản phẳng, khó đọc/khó chỉnh sửa khi hướng dẫn
dài hoặc có cấu trúc (nhiều ý, danh sách).

## Giải pháp

### Phần 1 — Layout một-panel (list ⇄ chi tiết)

Mặc định chỉ hiện `TemplateList` (full width). Khi người dùng bấm vào một mẫu để xem/sửa, hoặc
bấm "Tạo mới", danh sách ẩn đi và `TemplateEditor` chiếm toàn bộ chiều rộng thay thế. Có nút
"← Quay lại" trong header của editor để trở về danh sách (không tự động lưu thay đổi trước khi
quay lại — giống hành vi nút "Huỷ" hiện có ở chế độ tạo mới).

### Phần 2 — BlockNote cho "Chỉ dẫn cho AI"

`TemplateSection.instruction` vẫn là `string` markdown ở data model (không đổi Rust/backend —
`frontend/src-tauri/src/summary/templates/types.rs` vẫn nhúng thẳng field này vào prompt LLM
dạng text như hiện nay). Chỉ đổi UI nhập liệu: dùng BlockNote thay cho `Textarea`, theo đúng
pattern chuyển đổi markdown ↔ blocks đã có trong `BlockNoteSummaryView.tsx`
(`tryParseMarkdownToBlocks` / `blocksToMarkdownLossy`).

Toolbar tối giản: `BlockTypeSelect` (đoạn văn/tiêu đề/danh sách gạch đầu dòng/đánh số) + Bold +
Italic + Nest/Unnest (thụt lề danh sách con). Không có Underline/Strike/màu chữ/căn lề/link —
không cần thiết cho một đoạn hướng dẫn gửi LLM.

## Thay đổi theo file

### `frontend/src/components/TemplateSettings/index.tsx`

Render có điều kiện: `state.editorMode === 'idle'` → hiện `TemplateList`; ngược lại → hiện
`TemplateEditor` (bỏ layout 2 cột song song `flex gap-4`, chỉ còn 1 panel active tại một thời
điểm). Container giữ nguyên `h-[calc(100vh-180px)] min-h-[400px]`.

### `frontend/src/components/TemplateSettings/TemplateList.tsx`

Đổi class `w-[280px] shrink-0` → `w-full` (giờ danh sách luôn đứng một mình, không còn cạnh
editor).

### `frontend/src/components/TemplateSettings/TemplateEditor.tsx`

Thêm nút back (icon `ArrowLeft` từ `lucide-react`) cạnh tiêu đề header, gọi prop `onBack` mới.
Hiện ở cả `mode === 'new'` và `mode === 'edit'`.

### `frontend/src/components/TemplateSettings/useTemplateSettings.ts`

- Thêm hàm `closeEditor()`: set `editorMode('idle')`, `selectedId(null)`, `editorData(null)` —
  không lưu thay đổi, trả về danh sách.
- Gắn id nội bộ (`_key: string`, chỉ tồn tại ở frontend, không gửi lên backend) cho mỗi phần tử
  trong `editorData.sections` mỗi khi dữ liệu được tạo/tải (`openTemplate`, `cloneTemplate`,
  `startNewTemplate`, `addSection`) — dùng `crypto.randomUUID()`. Lý do: BlockNote chỉ đọc
  `initialContent` một lần lúc mount, không tự cập nhật khi prop đổi; nếu vẫn key theo `index`
  như hiện tại, editor sẽ hiển thị sai nội dung khi đổi mẫu hoặc di chuyển thứ tự phần. `_key`
  phải bị strip khỏi object trước khi `JSON.stringify` để lưu (ở `saveTemplate`), vì backend
  không biết field này.

### `frontend/src/components/TemplateSettings/types.ts`

Thêm field optional `_key?: string` vào `TemplateSection` (chỉ dùng frontend).

### `frontend/src/components/TemplateSettings/SectionEditor.tsx`

Thay `Textarea` của "Chỉ dẫn cho AI" bằng component mới `SectionInstructionEditor`.

### `frontend/src/components/TemplateSettings/SectionInstructionEditor.tsx` (mới)

Dynamic-import BlockNote (`ssr: false`, giống pattern ở `BlockNoteSummaryView.tsx`) để tránh lỗi
SSR của Next.js. Nhận `value: string` (markdown), `onChange: (markdown: string) => void`,
`disabled?: boolean`. Luồng chuyển đổi:

- Mount: `editor.tryParseMarkdownToBlocks(value)` → dùng làm `initialContent`.
- On change: debounce ~300ms, gọi `blocksToMarkdownLossy(editor.document)` rồi `onChange(md)`.

### `frontend/src/components/TemplateSettings/index.tsx` (sections.map trong `TemplateEditor.tsx`)

Đổi `key={idx}` → `key={section._key}` để BlockNote instance đi đúng theo dữ liệu của nó khi
thêm/xóa/di chuyển phần hoặc chuyển mẫu.

## Ngoài phạm vi

- Không thêm cảnh báo "có thay đổi chưa lưu" khi bấm "Quay lại" hoặc chuyển mẫu.
- Không validate nội dung markdown của instruction ở frontend (backend vẫn báo lỗi qua toast
  nếu instruction rỗng, như hiện tại).
- Không đổi cách `TemplateList` hiển thị từng mẫu (badge, nút đặt mặc định...).
- Không thêm placeholder text bên trong BlockNote khi rỗng.
