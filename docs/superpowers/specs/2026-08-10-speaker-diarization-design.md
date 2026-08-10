# Phân biệt người nói (speaker diarization) — file import

## Vấn đề

Meetily hiện không phân biệt được ai nói gì trong một cuộc họp — toàn bộ transcript là một luồng
văn bản liên tục, không gắn với người nói cụ thể. Với một app "biên bản cuộc họp", đây là khoảng
trống lớn: biên bản không biết ai phát biểu ý nào thì giá trị sử dụng giảm đáng kể, đặc biệt với
cuộc họp nhiều người.

App tham khảo (`C:\Users\HP\Desktop\test ASR`) có tính năng này: chọn model diarization (Pyannote
hoặc Senko CAM++), tự đoán hoặc nhập tay số người nói, và cho sửa tay (đổi tên, tách câu, gộp lượt
nói) sau khi xử lý xong.

**Phạm vi spec này:** chỉ đường **file import** (`import.rs`/`retranscription.rs`, dùng chung
`batch_transcribe.rs`). Live recording chưa có trong spec này — kiến trúc streaming khác hẳn (phải
cluster tăng dần, dễ nhầm khi chưa đủ dữ liệu giọng của một người), để làm riêng sau nếu cần.

## Phát hiện quan trọng (đọc code thật, không đoán)

- `transcripts` đã có sẵn 1 cột `speaker TEXT` (migration
  `20251110000001_add_speaker_field.sql`), nhưng đọc kỹ migration thì mục đích ban đầu là ghi
  `'mic'`/`'system'` — phân biệt **nguồn audio** (mic hay loa), không phải phân biệt **người nói**
  thật sự. Không có chỗ nào trong Rust, TypeScript, hay backend Python đọc/ghi cột này — nó là dead
  column. Vì mục đích khác hẳn tính năng này (và semantics 'mic'/'system' không áp dụng được cho
  file import, vốn thường chỉ có 1 track âm thanh đã trộn sẵn), spec này **không tái sử dụng** cột
  đó — để nguyên, không đụng vào, thêm cột mới đúng kiểu cho rõ ràng.
- Kiến trúc pipeline hiện tại (ROVER → `stitch_word_chunks` → CAPU `finalize_rover_word_timeline`)
  không mang theo thông tin "audio gốc" xuống tới bước tạo `TranscriptSegment` cuối cùng — mọi thứ
  từ đó là văn bản + timestamp giây. Diarization vì vậy phải chạy như **một nhánh riêng, đọc thẳng
  từ samples đã decode** (`load_audio_for_file_pipeline`'s output), không lồng vào giữa luồng
  ASR/CAPU.
- Model CAPU/ROVER hiện tải theo cơ chế "tải khi cần" nhất quán (`capu_engine/commands.rs`:
  `capu_is_model_downloaded`, `capu_init`, hằng số file/kích thước trong `config.rs`) — spec này đi
  theo đúng khuôn mẫu đó cho 2 model mới, không phát minh cơ chế tải mới.
- CAPU đã có tiền lệ "lỗi thì fallback về text thô, không chặn cả pipeline"
  (`CapuBatcher::flush`/`flush_with_fallback`) — diarization áp dụng đúng nguyên tắc này.

## Ngoài phạm vi

- Live recording (xem phần "Phạm vi" ở trên).
- Tách tiếng nói chồng lời (overlap speech separation) — độ phức tạp cao (Conv-TasNet trong app
  tham khảo), xếp vào tính năng riêng sau nếu cần. V1 gán chồng lời cho 1 người nói duy nhất.
- Tách 1 câu thành người nói khác (split) — chỉ làm đổi tên + gộp cho v1, theo quyết định đã chốt.
- Undo cho thao tác gộp — không có trong v1, xem "Rủi ro" bên dưới.
- Chọn giữa nhiều model diarization (app tham khảo cho chọn Pyannote vs Senko CAM++) — v1 chỉ dùng
  1 cặp model cố định (chọn cụ thể lúc lập plan, xem "Thiết kế §2").

## Thiết kế

### 1. Vị trí trong pipeline

Diarization chạy **độc lập với ASR/CAPU**, dùng chung bộ samples 16kHz đã decode cho ASR
(`load_audio_for_file_pipeline`), không phụ thuộc kết quả ASR/CAPU:

```
samples (16kHz, đã decode)
    ├──→ VAD → ROVER decode → stitch → CAPU finalize → TranscriptSegment[]  (đã có)
    └──→ [nếu bật] segmentation → embedding → clustering → SpeakerTurn[]     (mới)
                                                                  │
                                          sau khi CẢ HAI xong ────┘
                                                     │
                                    align_speakers_to_segments(): mỗi TranscriptSegment
                                    được gán speaker_id theo SpeakerTurn trùng thời gian
                                    nhiều nhất (max-overlap, hàm thuần, dễ test)
```

Hai nhánh có thể chạy song song (đều chỉ cần `samples`, không phụ thuộc nhau) — mức độ song song
thật sự (tránh tranh CPU với ROVER vốn đã dùng nhiều luồng) cần đo thực tế lúc implement, không
chốt cứng trong spec.

### 2. Module mới — `diarization_engine/`

Theo đúng khuôn mẫu `capu_engine/`, `rover_engine/`:

- `segmentation.rs` — load + chạy model segmentation (ONNX qua `ort`, giống `rover_engine`), input
  toàn bộ audio, output các đoạn nhỏ đồng nhất 1 giọng + điểm đổi giọng.
- `embedding.rs` — load + chạy model embedding, input 1 đoạn nhỏ, output vector đặc trưng giọng cố
  định chiều dài.
- `clustering.rs` — agglomerative hierarchical clustering (cosine distance) trên tập vector, 2 chế
  độ: cắt về đúng K cụm (khi người dùng nhập số người nói), hoặc dừng theo ngưỡng khoảng cách (chế
  độ tự động — ngưỡng cụ thể cần tinh chỉnh bằng audio thật lúc implement, không đoán trước).
- `engine.rs` — orchestrator: samples → segmentation → embedding từng đoạn → clustering →
  `Vec<SpeakerTurn { start_sec, end_sec, cluster_index }>`.
- `align.rs` (hoặc hàm trong `sentence_segment.rs`) — `align_speakers_to_segments`, hàm thuần map
  `SpeakerTurn[]` lên `TranscriptSegment[]` đã có, theo overlap lớn nhất.

Model ONNX cụ thể (segmentation + embedding) chốt lúc lập plan — cần kiểm tra thực tế bản ONNX nào
tải được, dung lượng, giấy phép, giống cách đã xác nhận trực tiếp file `.onnx`/`vocab.txt` cho
CAPU/ROVER thay vì suy đoán. Thêm hằng số vào `config.rs` theo đúng mẫu `CAPU_MODEL_FILE` v.v.

### 3. Data model (schema)

Migration mới `frontend/src-tauri/migrations/20260810000000_add_meeting_speakers.sql`:

```sql
CREATE TABLE meeting_speakers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id TEXT NOT NULL REFERENCES meetings(id),
    cluster_index INTEGER NOT NULL,
    display_name TEXT NOT NULL,
    color TEXT NOT NULL
);

ALTER TABLE transcripts ADD COLUMN speaker_id INTEGER REFERENCES meeting_speakers(id);
```

- `display_name` mặc định `"Người nói {cluster_index + 1}"`, đổi được.
- `color` gán tự động theo `cluster_index % bảng_màu_cố_định` lúc tạo — nhất quán trong 1 cuộc họp.
- `transcripts.speaker_id` NULL với mọi transcript hiện có / mọi meeting không bật diarization —
  tương thích ngược hoàn toàn, không cần backfill.

**Đổi tên** = `UPDATE meeting_speakers SET display_name = ? WHERE id = ?` — áp dụng toàn bộ cuộc
họp cùng lúc vì tất cả segment cùng người trỏ chung 1 `meeting_speakers.id`.

**Gộp với đoạn trước** = `UPDATE transcripts SET speaker_id = <speaker_id của đoạn liền trước> WHERE id = <đoạn hiện tại>` — chỉ đổi **đoạn đang thao tác**, không đụng các đoạn khác cùng cụm ở
chỗ khác trong cuộc họp (sửa đúng chỗ sai, không gộp 2 người thành 1 trên toàn cuộc họp).

### 4. UI/UX

- Checkbox "Phân biệt người nói" trong khu vực cài đặt ASR luồng File (cạnh hotwords, max segment
  seconds hiện có — component tương ứng với `SharedTranscriptPanel.tsx`), **tắt mặc định**. Bật thì
  hiện thêm ô nhập tùy chọn "Số người nói (để trống nếu không biết)", giới hạn hợp lý (1-20).
- `FlowingTranscriptView.tsx`: khi `TranscriptSegmentData` có `speakerId`/`speakerName`/`speakerColor`
  (field mới, optional), các đoạn liên tiếp cùng người nói được nhóm thành 1 khối với nhãn tên +
  màu ở đầu khối — một loại ngắt đoạn mới, độc lập với ngắt đoạn theo dấu câu hiện có. Meeting
  không có dữ liệu diarization thì hiển thị y hệt hiện tại, không cần fallback đặc biệt (field đơn
  giản là `undefined`).
- Đổi tên: bấm vào nhãn tên → Popover (tái dùng đúng cơ chế Popover đã dùng cho sửa nội dung câu) →
  lưu qua command Tauri mới `rename_meeting_speaker(speaker_id, new_name)`.
- Gộp: nút nhỏ hiện khi hover khối (trừ khối đầu tiên của cuộc họp) → bấm là gộp ngay, không cần
  dialog xác nhận → command `merge_speaker_segment(segment_id)` (tự tìm `speaker_id` của đoạn liền
  trước ở phía Rust, tránh phải truyền 2 id từ frontend rồi tự tính lệch).

### 5. Xử lý lỗi & trường hợp biên

Nguyên tắc chính: **diarization lỗi không bao giờ làm hỏng cả import** — model chưa tải, tải lỗi,
hay suy luận lỗi giữa chừng đều fallback về "coi như không bật checkbox" (mọi `speaker_id` NULL),
kèm cảnh báo nhẹ, đúng tiền lệ `CapuBatcher::flush`.

- Model chưa tải khi bật checkbox → dùng lại luồng tải-khi-cần đã có (không làm luồng tải riêng).
- Số người nói nhập tay validate 1-20 (giống app tham khảo).
- Chỉ 1 người nói thực tế → tự hội tụ về 1 cụm qua đúng code path, không cần case riêng.
- `retranscription.rs` dùng chung `batch_transcribe.rs` nên tự động thừa hưởng luồng này — nối
  đúng chỗ lúc lập plan, không cần thiết kế riêng.
- Sửa nội dung câu bằng tay không ảnh hưởng `speaker_id` — hai việc độc lập.

## Kiểm thử

### Tự động (Rust)
- `clustering.rs`: vector giả lập 2 cụm tách biệt rõ → gộp đúng; chế độ K cố định; chế độ ngưỡng tự
  động (test với ngưỡng đặt cứng trong test, không phụ thuộc giá trị "đúng" cuối cùng sẽ tinh chỉnh
  sau bằng audio thật).
- `align_speakers_to_segments`: `SpeakerTurn[]`/`TranscriptSegment[]` giả lập với các ca max-overlap
  rõ ràng, kể cả ca lệch biên (segment nằm giữa 2 turn).
- Gán màu theo `cluster_index` — hàm thuần, test trực tiếp.
- Đổi tên / gộp trên DB — theo đúng khuôn mẫu test repository đã có trong `database/repositories/`.

### Tích hợp với model thật (đánh dấu `#[ignore]`, giống `rover_decode_on_real_audio`,
`restore_punctuation_on_real_model`)
- Chạy `diarization_engine::engine` trên 1 file audio nhiều người nói thật, kiểm tra số cụm và ranh
  giới lượt nói hợp lý.

### Tự kiểm chứng bằng dữ liệu thật trước khi báo xong
Vì độ chính xác diarization mang tính cảm quan, chạy thử trên audio thật nhiều người nói (không
phải audio giả lập), tự soi kết quả (số người phát hiện đúng, ranh giới lượt nói có hợp lý không) —
không chỉ dựa vào test pass, theo đúng cách đã tự chẩn đoán CAPU trong phiên làm việc trước.

## Rủi ro & giảm thiểu

| Rủi ro | Giảm thiểu |
|---|---|
| Chưa xác định được model ONNX segmentation/embedding cụ thể tải được, dung lượng, giấy phép hợp lý | Xác nhận trực tiếp bằng cách tải thử + chạy thử trước khi chốt vào plan, giống cách đã làm với CAPU/ROVER — không đoán tên file |
| Ngưỡng clustering tự động (chế độ không nhập số người nói) sai lệch nhiều so với thực tế | Tinh chỉnh bằng audio thật nhiều người nói trong lúc implement, chấp nhận cần vài vòng thử giống CAPU confidence level |
| Chạy song song với ROVER làm tranh CPU, chậm hơn kỳ vọng | Đo thực tế; nếu tệ hơn chạy tuần tự thì đổi sang tuần tự (checkbox tắt mặc định nên tác động tới đa số người dùng = 0) |
| `align_speakers_to_segments` gán sai khi 1 câu CAPU span đúng ranh giới đổi người nói (ví dụ ngắt lời giữa câu) | Chấp nhận là hạn chế đã biết ở mức câu; người dùng có nút "gộp" để sửa tay chỗ sai, không cần giải tại thuật toán |
| Gộp nhầm không có undo | V1 chấp nhận rủi ro nhỏ (thao tác gộp là 1 click, thấy kết quả ngay); bổ sung undo sau nếu người dùng thực tế gặp vấn đề |
