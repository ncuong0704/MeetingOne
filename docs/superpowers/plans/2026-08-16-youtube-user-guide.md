# YouTube user guide — Implementation Plan

> **For agentic workers:** Execute inline. Spec: [2026-08-16-youtube-user-guide-design.md](../specs/2026-08-16-youtube-user-guide-design.md)

**Goal:** Dialog Hướng dẫn hiển thị video YouTube từ `guideVideos.ts`; gỡ react-joyride và `data-tour`.

**Architecture:** Catalog thuần TS + `UserGuideButton` gọi `open_external_url`. Xóa `UserGuideContext`/Joyride; gỡ listener tour khỏi settings/template.

**Tech Stack:** TypeScript, `open_external_url`, `node:test` cho URL helper.

---

### Task 1: Catalog + dialog

- Create: `frontend/src/components/UserGuide/guideVideos.ts`
- Create: `frontend/src/components/UserGuide/guideVideos.test.ts`
- Modify: `UserGuideButton.tsx`, `index.ts`

### Task 2: Gỡ Joyride

- Delete tour/joyride/context files
- Unwrap `UserGuideProvider` in `layout.tsx`
- Strip `data-tour` and tour event listeners
- `pnpm remove react-joyride`

### Task 3: Test + restart

- `npx tsx --test src/components/UserGuide/guideVideos.test.ts`
- Restart `tauri:dev`
