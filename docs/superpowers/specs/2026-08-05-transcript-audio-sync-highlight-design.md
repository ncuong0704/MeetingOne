# Đồng bộ transcript ↔ audio: highlight khi phát & click-to-seek

## Vấn đề

Khi người dùng mở **Chi tiết cuộc họp** và phát lại file ghi âm, transcript hiện tại là danh sách tĩnh — không có liên kết với vị trí phát audio. App tham chiếu (`test ASR`, tab File) đã có:

- Highlight nền vàng đoạn đang phát theo `positionChanged` / `timeupdate`
- Click anchor / dòng → `seek` tới `audio_start_time` của segment
- Debounce 500ms sau click để highlight không “nhảy” khi seek

Meetily đã có nền tảng kỹ thuật:

| Thành phần | Trạng thái |
|---|---|
| `audio_start_time` / `audio_end_time` trên `Transcript` | Có (DB + API) |
| `MeetingDetails/AudioPlayer.tsx` + `useAudioPlayer.ts` | Có — `currentTime`, `seek()` |
| `seekRef` trên `AudioPlayer` | Có prop, **chưa wire** từ `TranscriptPanel` |
| `VirtualizedTranscriptView` + pagination 100/page | Có |
| `useAutoScroll` + `activeSegmentId` | Có stub scroll-to-center, **chưa truyền** `activeSegmentId` |

Thiếu: nối `currentTime` ↔ segment active, highlight UI, click-to-seek, scroll có điều kiện khi segment ra khỏi viewport.

## Mục tiêu (v1)

Trên trang **Chi tiết cuộc họp** (`meeting-details`), khi bật trình phát audio:

1. **Highlight** segment tương ứng với `currentTime` (mức segment, không partial/word).
2. **Click cả dòng** transcript → seek audio tới `audio_start_time` (không auto-play nếu đang pause).
3. **Auto-scroll:** chỉ cuộn khi segment active **ra khỏi** vùng nhìn (không cuộn mỗi frame).
4. Hoạt động với **pagination** (>100 segment): load thêm khi playback/click cần segment chưa tải.

## Ngoài phạm vi v1

- Trang ghi âm live (`app/page.tsx`) — highlight realtime theo partial/chunk (như `tab_live.py`).
- Highlight sub-chunk / word trong một segment.
- Dual-speaker overlap (nhiều segment active).
- Deep-link URL `?t=123` (stub `initialTimestamp` trong `usePaginatedTranscripts` — phase sau).
- API backend `offset_by_timestamp` (optional phase 1.1).
- Thay đổi Rust / pipeline ASR.

## Quyết định đã chốt (brainstorming)

| Câu hỏi | Lựa chọn |
|---|---|
| Phạm vi v1 | **A** — Chi tiết cuộc họp + phát file audio |
| Auto-scroll khi phát | **C** — Chỉ cuộn khi active segment **ra khỏi** viewport |
| Click-to-seek | **A** — Click **cả dòng** transcript |

## Các phương án

### A. Hook `useTranscriptAudioSync` trong `TranscriptPanel` (khuyến nghị)

- Wire `AudioPlayer` ↔ `VirtualizedTranscriptView` qua hook mới + `seekRef`.
- **Ưu:** Ít file, khớp pattern hiện tại, ship nhanh v1.
- **Nhược:** Live phase 2 cần lift state hoặc tái dùng hook.

### B. `PlaybackSyncContext` ở `page-content.tsx`

- **Ưu:** Mở rộng live sau.
- **Nhược:** Over-engineering cho v1 meeting-details-only.

### C. Gộp logic vào `useAudioPlayer`

- **Ưu:** Một nơi cho audio.
- **Nhược:** Hook phình, transcript phụ thuộc segments — vi phạm separation.

**Khuyến nghị: A.**

## Thiết kế

### 1. Kiến trúc & luồng dữ liệu

```
TranscriptPanel
├── seekRef (useRef)
├── AudioPlayer(meetingFolderPath, seekRef, onTimeUpdate?)
│     └── useAudioPlayer → currentTime mỗi timeupdate
├── useTranscriptAudioSync(segments, currentTime, seekRef, loadMore, hasMore)
│     ├── resolveActiveSegment(segments, t) → activeSegmentId
│     ├── suppressHighlightUntil (500ms sau click)
│     └── handleSegmentClick(id) → seek + suppress
└── VirtualizedTranscriptView
      ├── activeSegmentId
      ├── playbackFollowMode=true
      ├── onSegmentClick
      └── scroll-if-out-of-view (không dùng useAutoScroll center-on-every-change)
```

**Lift `currentTime`:** `AudioPlayer` thêm optional callback `onTimeUpdate?: (t: number) => void` (hoặc `currentTime` prop từ parent nếu lift hook lên `TranscriptPanel`). Khuyến nghị: `TranscriptPanel` gọi `useAudioPlayer` trực tiếp, truyền controls xuống `AudioPlayer` dạng presentational — **hoặc** giữ hook trong `AudioPlayer` và expose `onTimeUpdate` để tránh refactor lớn.

**Lựa chọn cụ thể (v1):** Thêm `onTimeUpdate?: (time: number) => void` vào `AudioPlayer`; gọi trong listener `timeupdate` của `useAudioPlayer` (thêm optional callback param). `TranscriptPanel` lưu `currentTime` state từ callback.

### 2. Thuật toán `resolveActiveSegment`

Pure function trong `frontend/src/lib/transcriptAudioSync.ts`:

**Input:** `segments: TranscriptSegmentData[]` (đã sort `timestamp` ASC), `t: number` (giây).

**Quy tắc:**

1. Với mỗi segment `s`:
   - `start = s.timestamp` (=`audio_start_time`, default 0)
   - `end = s.endTime ?? start + 5` (fallback 5s nếu thiếu `audio_end_time` — tránh gap vô hạn)
2. **Trong window:** `start <= t <= end` → candidate.
3. Nhiều candidate: chọn segment có `start` **lớn nhất** vẫn `<= t` (segment “đang nói” gần nhất).
4. Không candidate trong window: chọn segment **tương lai gần nhất** (`start > t`, minimize `start - t`); nếu không có → segment **cuối** có `start <= t`.

**Debounce highlight:** `useTranscriptAudioSync` cập nhật `activeSegmentId` qua `requestAnimationFrame` hoặc throttle 50ms — tránh re-render mỗi `timeupdate` (~4Hz HTML audio).

**Sau click seek:** set `suppressUntil = Date.now() + 500`; trong khoảng này không đổi `activeSegmentId` theo `timeupdate` (pattern `tab_file.py`).

### 3. Click-to-seek

- `onSegmentClick(segmentId)` trên `TranscriptSegment` — `onClick` trên **wrapper dòng** (`div` ngoài), không trên nút edit.
- **Chặn seek khi editing:** nếu `isEditing`, click không seek.
- **Nút Pencil:** `e.stopPropagation()` để click edit không seek.
- Seek: `seekRef.current?.(segment.timestamp)` (giây).
- **Play state:** không thay đổi — pause vẫn pause sau seek.
- Nếu `showAudioPlayer === false`: click vẫn có thể seek nếu player đã mount — **khuyến nghị:** auto-bật player khi click segment (`setShowAudioPlayer(true)`) nếu chưa hiện.

### 4. Auto-scroll (chỉ khi out-of-view)

Không dùng effect `useAutoScroll` hiện tại (scroll center mỗi khi `activeSegmentId` đổi).

Thêm mode `playbackFollow` trong `VirtualizedTranscriptView` hoặc hook `usePlaybackFollowScroll`:

1. Khi `activeSegmentId` thay đổi (và không trong `suppress` window từ playback):
2. Tìm index segment trong list đã load.
3. **Virtualized:** dùng `virtualizer.getVirtualItems()` — active index không nằm trong `[firstIndex, lastIndex]` → `virtualizer.scrollToIndex(index, { align: 'center', behavior: 'smooth' })`.
4. **Non-virtualized:** `getBoundingClientRect` của `#segment-${id}` vs scroll container — nếu top < container top hoặc bottom > container bottom → `scrollIntoView({ block: 'center', behavior: 'smooth' })`.
5. Không scroll nếu segment vẫn fully visible.

`disableAutoScroll={true}` trên meeting details **giữ nguyên** — chặn logic recording scroll-to-bottom; playback follow là đường riêng.

### 5. Pagination (>100 segment)

`usePaginatedTranscripts`: `DEFAULT_PAGE_SIZE = 100`, offset-based `api_get_meeting_transcripts`.

**Khi playback:** nếu `t > maxLoadedEndTime` và `hasMore`:
- Gọi `onLoadMore()` (debounce tối đa 1 lần / 2s).
- `maxLoadedEndTime = max(s.endTime ?? s.timestamp)` trên `convertedSegments`.

**Khi click segment chưa load:**
- Loop: while segment id không có trong list và `hasMore` → `await onLoadMore()`.
- Giới hạn 20 lần load (tránh vòng vô hạn).
- Nếu vẫn không có sau load → toast nhẹ “Không tìm thấy đoạn trong bản ghi đã tải”.

**Phase 1.1 (optional):** API `offset_by_timestamp(meetingId, t)` — tính offset gần đúng, một request thay vì loop.

### 6. UI / styling

| Element | Style |
|---|---|
| Row active | `bg-amber-100/80` hoặc `bg-[rgba(255,215,0,0.25)]`, `rounded-md` |
| Row hover (playback mode) | `hover:bg-gray-50`, `cursor-pointer` |
| Timestamp khi active | `text-[#16478e] font-semibold` |
| Edit mode | Không highlight click affordance trên textarea |

Giữ nhất quán brand `#16478e`; vàng nhạt tham chiếu test ASR `#ffd700` ở opacity thấp.

### 7. File thay đổi

| File | Thay đổi |
|---|---|
| `frontend/src/lib/transcriptAudioSync.ts` | **Mới** — `resolveActiveSegment`, helpers |
| `frontend/src/hooks/useTranscriptAudioSync.ts` | **Mới** — state, suppress, click handler, load-until |
| `frontend/src/hooks/useAudioPlayer.ts` | Optional `onTimeUpdate` callback |
| `frontend/src/components/MeetingDetails/AudioPlayer.tsx` | `onTimeUpdate` prop; có thể nhận `play/pause/seek` từ parent (minimal) |
| `frontend/src/components/MeetingDetails/TranscriptPanel.tsx` | Wire sync hook, `seekRef`, `showAudioPlayer` auto-open on click |
| `frontend/src/components/VirtualizedTranscriptView.tsx` | Props: `activeSegmentId`, `onSegmentClick`, `playbackFollow`, `isActive` styling |
| `frontend/src/hooks/usePlaybackFollowScroll.ts` | **Mới** (optional — có thể inline trong view) |

**Không đổi:** Rust backend, live recording page, `useAutoScroll` recording path.

### 8. Xử lý lỗi & edge cases

| Case | Xử lý |
|---|---|
| Không có file audio | Player hiện `FILE_NOT_FOUND`; click segment vẫn mở player → user thấy message |
| Segment thiếu `audio_start_time` | Coi `timestamp = 0` |
| Segment thiếu `audio_end_time` | Fallback `end = start + 5` |
| `duration = 0` (chưa load metadata) | Không resolve active; không seek |
| User scroll tay trong lúc phát | Không disable follow (v1) — chỉ scroll khi out-of-view, ít gây conflict |
| Cuộc họp 0 segment | No-op |

### 9. Kiểm thử

**Unit (pure):** `resolveActiveSegment` — cases:
- `t` trong một segment
- overlap hai segment (chọn start gần `t`)
- `t` giữa hai segment (gap) → future nearest
- `t` sau segment cuối
- thiếu `endTime`

**Manual checklist:**
1. Play → highlight đúng dòng theo thời gian.
2. Pause, click dòng giữa → audio nhảy, highlight ổn sau 500ms.
3. Scroll để active segment off-screen → list cuộn center.
4. Active segment trong view → không cuộn.
5. Cuộc họp >100 segment: play tới phút 5 → load more + highlight tiếp.
6. Click segment trang chưa load → load until found + seek.
7. Click Pencil → edit, không seek.
8. Player ẩn → click dòng → player hiện + seek.

**Verify:** `pnpm exec tsc --noEmit` (frontend); smoke trên Tauri dev.

### 10. Tiêu chí hoàn thành

- [ ] Highlight segment sync với `currentTime` khi phát audio trên meeting details.
- [ ] Click cả dòng seek tới `audio_start_time`.
- [ ] Scroll chỉ khi active segment out-of-view.
- [ ] Pagination: playback và click load segment chưa tải (loop `loadMore` với guard).
- [ ] Không regression trang ghi âm live và `disableAutoScroll` recording.

## Tham chiếu code hiện tại

- `frontend/src/components/MeetingDetails/TranscriptPanel.tsx` — chưa wire `seekRef`
- `frontend/src/components/MeetingDetails/AudioPlayer.tsx` — `seekRef` expose `seek`
- `frontend/src/hooks/useAudioPlayer.ts` — `timeupdate` → `currentTime`
- `frontend/src/components/VirtualizedTranscriptView.tsx` — `TranscriptSegment` `id={`segment-${id}`}`
- `frontend/src/hooks/usePaginatedTranscripts.ts` — pagination 100/page
- App tham chiếu: `test ASR/tab_file.py` (`seek_to_sentence`, highlight logic ~3989–4179)

## Self-review (spec)

- **Placeholder:** Không có TBD — phase 1.1 API ghi rõ optional.
- **Nội bộ:** `disableAutoScroll` vs `playbackFollow` tách bạch; không contradict.
- **Phạm vi:** Một subsystem frontend, một implementation plan.
- **Ambiguity:** `endTime` fallback 5s đã explicit; auto-open player on click đã chốt.
