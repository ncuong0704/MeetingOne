# Highlight câu theo trình phát — ghi âm trực tiếp lệch thời gian

**Ngày:** 2026-08-15  
**Trạng thái:** Chốt để implement

## Vấn đề

Sau ghi âm trực tiếp, trang chi tiết mở `audio.mp4` và highlight câu theo `HTMLAudioElement.currentTime`. **Highlight lệch thời gian** so với tiếng đang phát.

Nhập file âm thanh: cùng UI highlight, **khớp**. App test ASR (tab File) cũng khớp.

UI không phải nguyên nhân — `AudioPlayer` → `useAudioPlayer` → `useTranscriptAudioSync` → `resolveActiveSegment(segments, currentTime)` dùng `audio_start_time` / `audio_end_time` cho cả hai luồng.

## Kỹ thuật nhập file / test ASR (đúng)

1. ASR chạy **trên chính file sẽ phát** (word timestamps).
2. CAPU trên toàn văn → `split_sentences` (`re.split(r'(?<=[.?!])\s+'`) → `align_sentences_to_words`.
3. Meetily: `frontend/src-tauri/src/audio/sentence_segment.rs` (`finalize_rover_word_timeline`) — cùng pattern test ASR.
4. Player `currentTime` và timestamp **cùng một timeline**.

## Kỹ thuật ghi trực tiếp (lệch)

Hai lệch độc lập:

### A. Timeline PCM vs AAC concat (lệch ngày càng lớn)

- Mixer 48 kHz gửi **cùng cửa sổ** sang ghi file và streaming ASR.
- Streaming ASR downsample 48k→16k; `StreamingSession` đếm sample 16 kHz → `audio_start_time` = đồng hồ PCM (đúng với tín hiệu mix).
- File phát: checkpoint AAC `audio_chunk_XXX.mp4` mỗi 30s rồi **ffmpeg concat `-c copy`**. Mỗi lần encode AAC-LC có encoder delay (~2048 sample ≈ 43 ms @ 48 kHz). Concat copy **cộng dồn** delay → file dài hơn PCM, lệch tăng theo số checkpoint (10 phút ≈ 20 chunk ≈ 0,9 s; 30 phút ≈ 2,6 s).
- Import không bị: một lần encode, decoder/player bù priming một lần.

### B. Granularity CAPU (khối lớn vs từng câu)

- `stop_recording` → `finalize_live_with_capu` → `CapuBatcher.flush` lấy **first.start + last.end** của cả batch, `replace_transcript_segments` gộp nhiều utterance thành **một** đoạn.
- Import sau CAPU **tách câu và map word times**. Live không làm bước này → highlight cả khối, cảm giác “không đúng câu đang nói”.

Hangover endpoint (im lặng cuối utterance) làm biên câu kém sắc hơn word-level import; không giải thích lệch tăng theo thời lượng họp. Pause discard cả recording lẫn ASR — không phải nguyên nhân.

## Quyết định

| Quyết định | Giá trị |
|---|---|
| Clock playback | Re-encode khi merge checkpoint (`-c:a aac`, không `-c copy`) — một AAC stream, một priming như import. Áp cả `merge_checkpoints` và recovery. |
| Câu như import | Sau CAPU live: `split_sentences` + map thời gian từ utterance gốc (nội suy từ thành `TimedWord`, rồi `align_sentences_to_words`). Không re-ASR cả file lúc stop. |
| Áp segment | Một lần rebuild danh sách (giữ `user_edited`), không replace batch-by-batch 1:1 — tránh lệch thứ tự khi 1 batch → N câu. |
| Highlight UI | Không đổi. `transcriptAudioSync` giữ nguyên. |

## Pipeline sau sửa

```
Stop ghi
  → drain streaming ASR (PCM times trên utterance)
  → unlisten transcript-finalized (tránh replace 1:1 đè lên N câu)
  → CAPU theo batch (speaker + word budget, như cũ)
  → mỗi batch: tách câu + align lên TimedWord nội suy từ utterance
  → ghi đè mọi segment không user_edited bằng danh sách câu
  → merge checkpoint: ffmpeg concat + re-encode AAC 48 kHz mono
  → lưu transcripts.json / DB như hiện tại
Phát lại
  → currentTime ≈ PCM clock (sai số priming một lần, ~40 ms)
  → resolveActiveSegment theo câu
```

## Ngoài phạm vi

- Re-ASR offline cả file lúc stop (chậm, đổi text).
- Word-level highlight.
- Đổi VAD / hangover streaming.
- Sửa UI `useTranscriptAudioSync`.

## Tiêu chí xong

- Unit test: nội suy TimedWord từ utterance; tách câu map span; rebuild không đè `user_edited`; ffmpeg args **không** `-c copy`.
- Live: highlight câu khớp trình phát (không lệch tăng theo phút). Import không regress.
- `cargo test` các module liên quan pass.
- App `tauri:dev` chạy để kiểm thử ghi live → chi tiết → play.
