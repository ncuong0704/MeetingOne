# Template Settings: single-pane layout + BlockNote instruction editor — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rework the Settings → Mẫu tab so the template list and the template editor never compete for space (one panel at a time, with a back button), and replace the plain `Textarea` for each section's "Chỉ dẫn cho AI" with a BlockNote rich-text editor.

**Architecture:** `TemplateSettings/index.tsx` switches between rendering `TemplateList` (idle) or `TemplateEditor` (edit/new) instead of showing both side by side. `TemplateSection` gets a frontend-only `_key` (UUID) so BlockNote instances — which only read their content once at mount — remount correctly whenever a template loads, a section is added, or sections are reordered. The instruction field's data model stays a plain markdown `string` (no Rust/backend changes); a new `SectionInstructionEditor` component converts markdown ↔ BlockNote blocks using the same `tryParseMarkdownToBlocks` / `blocksToMarkdownLossy` calls already used in `BlockNoteSummaryView.tsx`.

**Tech Stack:** Next.js 14 / React 18 (`frontend/src`), `@blocknote/core` + `@blocknote/react` + `@blocknote/shadcn` 0.36.0 (already a dependency), Tailwind classes, `lucide-react` icons.

**No test runner exists in this project** (`frontend/package.json` has no `test`/`vitest`/`jest` script — see repo survey). Per `CLAUDE.md`, UI changes are verified by running the dev server and exercising the feature in the browser, not by an automated test suite. Each task below is verified with `tsc --noEmit` (type safety) and, for the final task, a manual browser walkthrough — there is no TDD red/green step because there is no test harness to write into.

---

### Task 1: Add frontend-only `_key` field to `TemplateSection`

**Files:**
- Modify: `frontend/src/components/TemplateSettings/types.ts`

- [ ] **Step 1: Add the `_key` field**

Open `frontend/src/components/TemplateSettings/types.ts` and change the `TemplateSection` interface:

```ts
export interface TemplateSection {
  title: string;
  instruction: string;
  format: 'paragraph' | 'list' | 'string';
  item_format?: string;
  example_item_format?: string;
  /**
   * Frontend-only stable id used to key the BlockNote instruction editor so it
   * remounts (and re-parses markdown) exactly when the underlying section data
   * changes identity — never sent to the backend.
   */
  _key?: string;
}
```

- [ ] **Step 2: Type-check**

Run: `pnpm --dir frontend exec tsc --noEmit -p tsconfig.json`
Expected: no new errors (this is an additive optional field, nothing else references `TemplateSection` exhaustively yet).

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/TemplateSettings/types.ts
git commit -m "feat(templates): add frontend-only _key to TemplateSection for stable React keys"
```

---

### Task 2: `useTemplateSettings` — generate/strip `_key`, add `closeEditor`

**Files:**
- Modify: `frontend/src/components/TemplateSettings/useTemplateSettings.ts`

- [ ] **Step 1: Add `withKeys` / `stripKeys` helpers**

Add these two functions right after the existing `slugify` function (around line 21):

```ts
function withKeys(data: TemplateData): TemplateData {
  return {
    ...data,
    sections: data.sections.map(s => ({ ...s, _key: crypto.randomUUID() })),
  };
}

function stripKeys(data: TemplateData): TemplateData {
  return {
    ...data,
    sections: data.sections.map(({ _key, ...rest }) => rest),
  };
}
```

- [ ] **Step 2: Assign `_key` wherever `editorData` is created**

In `openTemplate`, replace:

```ts
      const parsed: TemplateData = JSON.parse(jsonStr);
      setEditorData(parsed);
```

with:

```ts
      const parsed: TemplateData = JSON.parse(jsonStr);
      setEditorData(withKeys(parsed));
```

In `cloneTemplate`, replace:

```ts
      const parsed: TemplateData = JSON.parse(jsonStr);
      parsed.name = `${parsed.name} (bản sao)`;
      const newId = `copy_of_${slugify(id)}`;
      setEditorData(parsed);
```

with:

```ts
      const parsed: TemplateData = JSON.parse(jsonStr);
      parsed.name = `${parsed.name} (bản sao)`;
      const newId = `copy_of_${slugify(id)}`;
      setEditorData(withKeys(parsed));
```

In `startNewTemplate`, replace:

```ts
    setEditorData({ name: '', description: '', sections: [{ ...EMPTY_SECTION }] });
```

with:

```ts
    setEditorData({ name: '', description: '', sections: [{ ...EMPTY_SECTION, _key: crypto.randomUUID() }] });
```

In `addSection`, replace:

```ts
      return { ...prev, sections: [...prev.sections, { ...EMPTY_SECTION }] };
```

with:

```ts
      return { ...prev, sections: [...prev.sections, { ...EMPTY_SECTION, _key: crypto.randomUUID() }] };
```

- [ ] **Step 3: Strip `_key` before sending to the backend**

In `saveTemplate`, replace:

```ts
    const templateJson = JSON.stringify(editorData, null, 2);
```

with:

```ts
    const templateJson = JSON.stringify(stripKeys(editorData), null, 2);
```

- [ ] **Step 4: Add `closeEditor`**

Add this new callback right after `cancelEdit`:

```ts
  const closeEditor = useCallback(() => {
    setEditorData(null);
    setSelectedId(null);
    setEditorMode('idle');
  }, []);
```

Add `closeEditor` to the hook's returned object (next to `cancelEdit`):

```ts
    cancelEdit,
    closeEditor,
```

- [ ] **Step 5: Type-check**

Run: `pnpm --dir frontend exec tsc --noEmit -p tsconfig.json`
Expected: no errors. If eslint's `no-unused-vars` flags the destructured `_key` in `stripKeys`, that's expected to pass since the project's lint config ignores identifiers prefixed with `_` — confirm with:

Run: `pnpm --dir frontend run lint`
Expected: no new warnings/errors from `useTemplateSettings.ts`.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/TemplateSettings/useTemplateSettings.ts
git commit -m "feat(templates): generate stable _key per section, add closeEditor"
```

---

### Task 3: `TemplateList` — full width (no longer sits beside the editor)

**Files:**
- Modify: `frontend/src/components/TemplateSettings/TemplateList.tsx`

- [ ] **Step 1: Change the width class**

Replace:

```tsx
    <div className="flex flex-col w-[280px] shrink-0 min-h-0 border border-gray-200 rounded-xl overflow-hidden bg-white">
```

with:

```tsx
    <div className="flex flex-col w-full min-h-0 border border-gray-200 rounded-xl overflow-hidden bg-white">
```

- [ ] **Step 2: Type-check**

Run: `pnpm --dir frontend exec tsc --noEmit -p tsconfig.json`
Expected: no errors (class-name-only change).

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/TemplateSettings/TemplateList.tsx
git commit -m "feat(templates): make TemplateList full width for single-pane layout"
```

---

### Task 4: `TemplateEditor` — back button, drop dead idle branch, key sections by `_key`

**Files:**
- Modify: `frontend/src/components/TemplateSettings/TemplateEditor.tsx`

- [ ] **Step 1: Import `ArrowLeft` and add the `onBack` prop**

Replace the icon import line:

```tsx
import { Plus, Save, Trash2, Copy, FileText } from 'lucide-react';
```

with:

```tsx
import { Plus, Save, Trash2, Copy, ArrowLeft } from 'lucide-react';
```

(`FileText` was only used by the idle-state placeholder removed in Step 3 below, so it's dropped from the import too.)

In the `TemplateEditorProps` interface, narrow `mode` and add `onBack` (this component is now only ever rendered for `'edit' | 'new'` — the parent (`index.tsx`, Task 5) renders `TemplateList` instead when the mode is `'idle'`):

```ts
interface TemplateEditorProps {
  mode: 'edit' | 'new';
  data: TemplateData | null;
  editingId: string;
  selectedInfo: TemplateInfo | undefined;
  isSaving: boolean;
  isDeleting: boolean;
  onEditingIdChange: (id: string) => void;
  onUpdateMeta: (field: 'name' | 'description', value: string) => void;
  onAddSection: () => void;
  onRemoveSection: (index: number) => void;
  onMoveSection: (index: number, direction: 'up' | 'down') => void;
  onUpdateSection: (index: number, field: keyof TemplateSection, value: string) => void;
  onSave: () => void;
  onDelete: (id: string) => void;
  onClone: (id: string) => void;
  onCancel: () => void;
  onBack: () => void;
}
```

Add `onBack` to the destructured function parameters (right after `onCancel`):

```ts
export function TemplateEditor({
  mode,
  data,
  editingId,
  selectedInfo,
  isSaving,
  isDeleting,
  onEditingIdChange,
  onUpdateMeta,
  onAddSection,
  onRemoveSection,
  onMoveSection,
  onUpdateSection,
  onSave,
  onDelete,
  onClone,
  onCancel,
  onBack,
}: TemplateEditorProps) {
```

- [ ] **Step 2: Type-check now to confirm the prop wiring compiles on its own**

Run: `pnpm --dir frontend exec tsc --noEmit -p tsconfig.json`
Expected: errors about `mode === 'idle'` being an impossible comparison further down and about `index.tsx` not passing `onBack` yet — both fixed by the remaining steps in this task and Task 5. This step is just a checkpoint; don't worry if it's red here.

- [ ] **Step 3: Remove the now-unreachable idle-state branch**

Delete this block entirely (mode can no longer be `'idle'` — the parent renders `TemplateList` for that case):

```tsx
  if (mode === 'idle') {
    return (
      <div className="flex-1 flex flex-col items-center justify-center text-center gap-3 border border-gray-200 rounded-xl bg-white p-8">
        <FileText className="w-12 h-12 text-gray-200" />
        <div>
          <p className="text-sm font-medium text-gray-500">Chọn một mẫu để xem</p>
          <p className="text-xs text-gray-400 mt-1">hoặc tạo mẫu mới từ nút bên trái</p>
        </div>
      </div>
    );
  }

  if (!data) return null;
```

Replace it with just:

```tsx
  if (!data) return null;
```

- [ ] **Step 4: Add the back button to the header**

Replace:

```tsx
      {/* Header */}
      <div className="flex items-center justify-between px-5 py-3 border-b border-gray-100 shrink-0">
        <h3 className="text-sm font-semibold text-gray-800">
          {mode === 'new' ? 'Tạo mẫu mới' : 'Chỉnh sửa mẫu'}
        </h3>

        {/* Action bar */}
        <div className="flex items-center gap-2">
```

with:

```tsx
      {/* Header */}
      <div className="flex items-center justify-between px-5 py-3 border-b border-gray-100 shrink-0">
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onBack}
            className="p-1 rounded hover:bg-gray-100 transition-colors"
            title="Quay lại danh sách"
          >
            <ArrowLeft className="w-4 h-4 text-gray-500" />
          </button>
          <h3 className="text-sm font-semibold text-gray-800">
            {mode === 'new' ? 'Tạo mẫu mới' : 'Chỉnh sửa mẫu'}
          </h3>
        </div>

        {/* Action bar */}
        <div className="flex items-center gap-2">
```

- [ ] **Step 5: Key sections by `_key` instead of array index**

Replace:

```tsx
              {data.sections.map((section, idx) => (
                <SectionEditor
                  key={idx}
```

with:

```tsx
              {data.sections.map((section, idx) => (
                <SectionEditor
                  key={section._key ?? idx}
```

(`?? idx` is a defensive fallback only — every code path that creates a section in `useTemplateSettings.ts`, Task 2, always assigns `_key`.)

- [ ] **Step 6: Type-check**

Run: `pnpm --dir frontend exec tsc --noEmit -p tsconfig.json`
Expected: remaining errors should only be in `index.tsx` (doesn't pass `onBack`, still calls `TemplateEditor` with a `mode: EditorMode` that includes `'idle'`) — resolved in Task 5.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/TemplateSettings/TemplateEditor.tsx
git commit -m "feat(templates): add back button to TemplateEditor, key sections by stable _key"
```

---

### Task 5: `TemplateSettings/index.tsx` — single-pane conditional rendering

**Files:**
- Modify: `frontend/src/components/TemplateSettings/index.tsx`

- [ ] **Step 1: Replace the side-by-side layout with conditional rendering**

Replace:

```tsx
  return (
    <div className="flex gap-4 h-[calc(100vh-180px)] min-h-[400px]">
      <TemplateList
        templates={state.templates}
        selectedId={state.selectedId}
        defaultTemplateId={state.defaultTemplateId}
        isSettingDefault={state.isSettingDefault}
        isLoading={state.isLoadingList}
        onSelect={state.openTemplate}
        onNew={state.startNewTemplate}
        onSetDefault={state.setAsDefault}
      />

      <TemplateEditor
        mode={state.editorMode}
        data={state.editorData}
        editingId={state.editingId}
        selectedInfo={selectedInfo}
        isSaving={state.isSaving}
        isDeleting={state.isDeleting}
        onEditingIdChange={state.setEditingId}
        onUpdateMeta={state.updateMeta}
        onAddSection={state.addSection}
        onRemoveSection={state.removeSection}
        onMoveSection={state.moveSection}
        onUpdateSection={state.updateSection}
        onSave={state.saveTemplate}
        onDelete={id => setDeleteTargetId(id)}
        onClone={state.cloneTemplate}
        onCancel={state.cancelEdit}
      />

      {deleteTargetId && deleteTargetInfo && (
```

with:

```tsx
  return (
    <div className="h-[calc(100vh-180px)] min-h-[400px]">
      {state.editorMode === 'idle' ? (
        <TemplateList
          templates={state.templates}
          selectedId={state.selectedId}
          defaultTemplateId={state.defaultTemplateId}
          isSettingDefault={state.isSettingDefault}
          isLoading={state.isLoadingList}
          onSelect={state.openTemplate}
          onNew={state.startNewTemplate}
          onSetDefault={state.setAsDefault}
        />
      ) : (
        <TemplateEditor
          mode={state.editorMode}
          data={state.editorData}
          editingId={state.editingId}
          selectedInfo={selectedInfo}
          isSaving={state.isSaving}
          isDeleting={state.isDeleting}
          onEditingIdChange={state.setEditingId}
          onUpdateMeta={state.updateMeta}
          onAddSection={state.addSection}
          onRemoveSection={state.removeSection}
          onMoveSection={state.moveSection}
          onUpdateSection={state.updateSection}
          onSave={state.saveTemplate}
          onDelete={id => setDeleteTargetId(id)}
          onClone={state.cloneTemplate}
          onCancel={state.cancelEdit}
          onBack={state.closeEditor}
        />
      )}

      {deleteTargetId && deleteTargetInfo && (
```

(TypeScript narrows `state.editorMode` to `'edit' | 'new'` inside the `else` branch of `state.editorMode === 'idle' ? ... : ...` automatically, so this satisfies the `mode: 'edit' | 'new'` prop type from Task 4 without a cast.)

- [ ] **Step 2: Type-check the whole `TemplateSettings` module**

Run: `pnpm --dir frontend exec tsc --noEmit -p tsconfig.json`
Expected: no errors anywhere under `frontend/src/components/TemplateSettings/`.

- [ ] **Step 3: Lint**

Run: `pnpm --dir frontend run lint`
Expected: no new errors/warnings.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/TemplateSettings/index.tsx
git commit -m "feat(templates): single-pane layout — list and editor no longer share the row"
```

---

### Task 6: `SectionInstructionEditor` — BlockNote wrapper (markdown in, markdown out)

**Files:**
- Create: `frontend/src/components/TemplateSettings/SectionInstructionEditor.tsx`

- [ ] **Step 1: Write the component**

```tsx
'use client';

import { useEffect, useRef, useState } from 'react';
import {
  useCreateBlockNote,
  FormattingToolbar,
  FormattingToolbarController,
  BlockTypeSelect,
  BasicTextStyleButton,
  NestBlockButton,
  UnnestBlockButton,
} from '@blocknote/react';
import { BlockNoteView } from '@blocknote/shadcn';
import { Block, PartialBlock } from '@blocknote/core';
import '@blocknote/shadcn/style.css';
import '@blocknote/core/fonts/inter.css';

interface SectionInstructionEditorProps {
  value: string;
  onChange: (markdown: string) => void;
  disabled?: boolean;
}

/** Renders once `initialBlocks` is parsed — a fresh BlockNote instance per mount,
 * so `useCreateBlockNote({ initialContent })` only ever needs its value at creation time. */
function InnerEditor({
  initialBlocks,
  onChange,
  disabled,
}: {
  initialBlocks: Block[];
  onChange: (markdown: string) => void;
  disabled?: boolean;
}) {
  const editor = useCreateBlockNote({
    initialContent: initialBlocks.length ? (initialBlocks as PartialBlock[]) : undefined,
  });

  const onChangeRef = useRef(onChange);
  useEffect(() => {
    onChangeRef.current = onChange;
  });

  useEffect(() => {
    let debounceTimer: ReturnType<typeof setTimeout>;
    const handleChange = () => {
      clearTimeout(debounceTimer);
      debounceTimer = setTimeout(async () => {
        const markdown = await editor.blocksToMarkdownLossy(editor.document);
        onChangeRef.current(markdown);
      }, 300);
    };
    const unsubscribe = editor.onChange(handleChange);
    return () => {
      clearTimeout(debounceTimer);
      if (typeof unsubscribe === 'function') unsubscribe();
    };
  }, [editor]);

  return (
    <div className="border border-gray-200 rounded-md min-h-[80px] text-sm [&_.bn-editor]:px-2 [&_.bn-editor]:py-1.5">
      <BlockNoteView
        editor={editor}
        editable={!disabled}
        theme="light"
        spellCheck={false}
        formattingToolbar={false}
      >
        <FormattingToolbarController
          formattingToolbar={() => (
            <FormattingToolbar>
              <BlockTypeSelect key="blockTypeSelect" />
              <BasicTextStyleButton basicTextStyle="bold" key="boldStyleButton" />
              <BasicTextStyleButton basicTextStyle="italic" key="italicStyleButton" />
              <NestBlockButton key="nestBlockButton" />
              <UnnestBlockButton key="unnestBlockButton" />
            </FormattingToolbar>
          )}
        />
      </BlockNoteView>
    </div>
  );
}

export default function SectionInstructionEditor({
  value,
  onChange,
  disabled = false,
}: SectionInstructionEditorProps) {
  // Parser-only editor — never rendered, only used to turn the stored markdown
  // string into blocks once, the same pattern BlockNoteSummaryView.tsx uses.
  const parserEditor = useCreateBlockNote({ initialContent: undefined });
  const [initialBlocks, setInitialBlocks] = useState<Block[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const blocks = await parserEditor.tryParseMarkdownToBlocks(value);
        if (!cancelled) setInitialBlocks(blocks as Block[]);
      } catch (err) {
        console.error('Không parse được nội dung chỉ dẫn thành BlockNote blocks:', err);
        if (!cancelled) setInitialBlocks([]);
      }
    })();
    return () => {
      cancelled = true;
    };
    // Intentionally run once: this instance is remounted (fresh `value`) via the
    // section's `_key` whenever the underlying data actually changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!initialBlocks) {
    return (
      <div className="text-xs text-gray-400 px-3 py-2 border border-gray-200 rounded-md min-h-[80px]">
        Đang tải trình soạn thảo...
      </div>
    );
  }

  return <InnerEditor initialBlocks={initialBlocks} onChange={onChange} disabled={disabled} />;
}
```

- [ ] **Step 2: Type-check**

Run: `pnpm --dir frontend exec tsc --noEmit -p tsconfig.json`
Expected: no errors. If `tryParseMarkdownToBlocks`/`blocksToMarkdownLossy` report a type mismatch, compare the exact call signatures already working in
`frontend/src/components/AISummary/BlockNoteSummaryView.tsx:288` and `:349` — copy the same argument/cast shape used there.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/TemplateSettings/SectionInstructionEditor.tsx
git commit -m "feat(templates): add BlockNote-based SectionInstructionEditor (markdown in/out)"
```

---

### Task 7: Wire `SectionInstructionEditor` into `SectionEditor`

**Files:**
- Modify: `frontend/src/components/TemplateSettings/SectionEditor.tsx`

- [ ] **Step 1: Swap the `Textarea` import for a dynamic import of the new editor**

Replace:

```tsx
import React from 'react';
import { ChevronUp, ChevronDown, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import {
```

with:

```tsx
import React from 'react';
import dynamic from 'next/dynamic';
import { ChevronUp, ChevronDown, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
```

Then, right after the imports block (before `interface SectionEditorProps`), add the dynamic import (BlockNote touches the DOM directly and must never run during Next.js SSR — the same reason `BlockNoteSummaryView.tsx` dynamic-imports `BlockNoteEditor/Editor`):

```tsx
const SectionInstructionEditor = dynamic(() => import('./SectionInstructionEditor'), {
  ssr: false,
  loading: () => (
    <div className="text-xs text-gray-400 px-3 py-2 border border-gray-200 rounded-md min-h-[80px]">
      Đang tải trình soạn thảo...
    </div>
  ),
});
```

- [ ] **Step 2: Replace the Textarea usage**

Replace:

```tsx
      {/* Instruction */}
      <div className="space-y-1">
        <label className="text-xs font-medium text-gray-600">Chỉ dẫn cho AI</label>
        <Textarea
          value={section.instruction}
          onChange={e => onChange('instruction', e.target.value)}
          placeholder="Mô tả chi tiết AI cần trích xuất gì từ cuộc họp..."
          className="text-sm min-h-[80px] resize-y"
          disabled={disabled}
        />
      </div>
```

with:

```tsx
      {/* Instruction */}
      <div className="space-y-1">
        <label className="text-xs font-medium text-gray-600">Chỉ dẫn cho AI</label>
        <SectionInstructionEditor
          value={section.instruction}
          onChange={md => onChange('instruction', md)}
          disabled={disabled}
        />
      </div>
```

- [ ] **Step 3: Type-check**

Run: `pnpm --dir frontend exec tsc --noEmit -p tsconfig.json`
Expected: no errors under `frontend/src/components/TemplateSettings/`.

- [ ] **Step 4: Lint**

Run: `pnpm --dir frontend run lint`
Expected: no new errors/warnings (in particular, no "unused import" for the removed `Textarea`).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/TemplateSettings/SectionEditor.tsx
git commit -m "feat(templates): use BlockNote editor for section instruction field"
```

---

### Task 8: Manual verification in the running app

There is no automated UI test harness in this project, so this feature is verified by hand per `CLAUDE.md`'s guidance for frontend changes.

**Files:** none (verification only).

- [ ] **Step 1: Start the dev app**

Run: `pnpm --dir frontend run tauri:dev:cpu` (or whichever `tauri:dev:*` variant matches the dev machine's GPU, per `CLAUDE.md`)
Expected: app window opens.

- [ ] **Step 2: Walk the golden path**

In the app: open Settings → tab "Mẫu".
- Confirm only the template list is visible (full width), no editor panel beside it.
- Click a template → list disappears, editor takes the full width, header shows a "←" back button.
- Confirm the "Chỉ dẫn cho AI" field for each section renders as a BlockNote editor (not a plain textarea) pre-filled with the existing instruction text, with a small toolbar exposing block type / bold / italic / indent controls.
- Edit an instruction (add a bold word and a bullet list item), click "Lưu" → toast "Đã lưu mẫu thành công".
- Click "←" → back to the list. Re-open the same template → confirm the bold word and bullet list you added are still there (proves the markdown round-trip through `blocksToMarkdownLossy` → save → `tryParseMarkdownToBlocks` on reload works).

- [ ] **Step 3: Edge cases**

- Click "Tạo mới" → confirm it also goes full width (not beside the list), and its instruction field starts as an empty BlockNote editor.
- Add a second section, move it up/down with the ↑/↓ buttons → confirm each section's instruction content stays attached to the correct section (not swapped/lost) — this is the scenario Task 1/2's `_key` fix targets.
- Remove a section, then add a new one → confirm the new section's editor is empty (not showing stale content from the removed one).
- Open a built-in (non-custom) template → confirm the BlockNote editor is read-only (`disabled` → `editable={false}`) and shows the existing "mẫu mặc định" banner/behavior unchanged.

- [ ] **Step 4: Report results**

Note in the conversation which of the above passed, and paste/describe any console errors (Rust log or browser devtools) if something didn't behave as expected — do not mark this task done without having actually run the app.
