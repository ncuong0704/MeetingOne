# Sao chép bản ghi khớp UI — Implementation Plan

> **For agentic workers:** Execute inline. Spec: [2026-08-16-copy-transcript-matches-ui-design.md](../specs/2026-08-16-copy-transcript-matches-ui-design.md)

**Goal:** Clipboard bản ghi (plain + HTML) cùng cấu trúc với `FlowingTranscriptView`: không timestamp, có nhãn người nói khi UI có.

**Architecture:** Helper thuần `frontend/src/lib/transcriptDisplay.ts` (clean / group / format). `FlowingTranscriptView` dùng lại group/clean; `useCopyOperations` và `TranscriptContext.copyTranscript` gọi format.

**Tech Stack:** TypeScript, `node:test` cho helper; clipboard API hiện có (`copyRichText`).

---

### Task 1: Helper + test

**Files:**
- Create: `frontend/src/lib/transcriptDisplay.ts`
- Create: `frontend/src/lib/transcriptDisplay.test.ts`

- [ ] Format plain/HTML; test không timestamp, speaker, flowing.

### Task 2: Nối UI copy

**Files:**
- Modify: `frontend/src/components/FlowingTranscriptView.tsx` (import helper)
- Modify: `frontend/src/hooks/meeting-details/useCopyOperations.ts`
- Modify: `frontend/src/contexts/TranscriptContext.tsx`

- [ ] Bỏ `formatTime` trong copy transcript.

### Task 3: Verify + restart

- [ ] `npx tsx --test src/lib/transcriptDisplay.test.ts`
- [ ] Restart `pnpm run tauri:dev`
