import { useEffect, RefObject } from "react";
import { Plugin, PluginKey } from "@tiptap/pm/state";

/**
 * Fix: Preserve bold/italic marks when typing Vietnamese with diacritics.
 *
 * Root cause: Vietnamese input methods (Unikey Telex/VNI) either:
 *   A) Use IME composition — ProseMirror inserts composed text via DOM sync
 *      and may drop storedMarks in the process.
 *   B) Simulate keystrokes — sends Backspace + replacement char; after the
 *      Backspace transaction ProseMirror clears storedMarks, so the
 *      replacement char is inserted without marks.
 *
 * Fix: Register a ProseMirror appendTransaction plugin that runs synchronously
 * inside the transaction pipeline and re-applies missing marks immediately
 * after the character is inserted.
 */
export function useVnMarkPreservation(
  editor: any,
  containerRef: RefObject<HTMLElement | null>
) {
  useEffect(() => {
    if (!editor) return;
    const tiptap = editor._tiptapEditor;
    if (!tiptap) return;
    const container = containerRef.current;
    if (!container) return;

    // Track IME composition state
    const compositionInfo: {
      marks: readonly any[] | null;
      fromPos: number;
      justEnded: boolean;
    } = { marks: null, fromPos: 0, justEnded: false };

    // Track deletion state (for Unikey backspace-simulation mode)
    const deleteInfo: {
      marks: readonly any[] | null;
      cursorPos: number;
    } = { marks: null, cursorPos: 0 };

    const pluginKey = new PluginKey("vnImeMarkPreserve");

    const plugin = new Plugin({
      key: pluginKey,
      appendTransaction(transactions, oldState, newState) {
        if (!transactions.some((tr) => tr.docChanged)) return null;

        // ── Case A: After IME composition end ────────────────────────────
        if (compositionInfo.justEnded && compositionInfo.marks?.length) {
          compositionInfo.justEnded = false;
          const marks = compositionInfo.marks!;
          compositionInfo.marks = null;
          const from = compositionInfo.fromPos;
          const to = newState.selection.$from.pos;

          if (to > from) {
            const newTr = newState.tr;
            let changed = false;
            marks.forEach((mark: any) => {
              const safePos = Math.max(0, Math.min(to - 1, newState.doc.content.size - 1));
              const alreadyHas = newState.doc
                .resolve(safePos)
                .marks()
                .some((m: any) => m.type.name === mark.type.name);
              if (!alreadyHas) {
                newTr.addMark(from, to, mark);
                changed = true;
              }
            });
            if (changed) {
              deleteInfo.marks = null;
              return newTr;
            }
          }
          // Even if range not found, set storedMarks for continuity
          deleteInfo.marks = null;
          if (marks.length > 0) return newState.tr.setStoredMarks(marks);
          return null;
        }

        // ── Case B: Delete → Insert sequence (Unikey backspace mode) ─────
        const delta = newState.doc.nodeSize - oldState.doc.nodeSize;

        if (delta < 0) {
          // Pure deletion: save marks from just before this transaction
          deleteInfo.marks =
            oldState.storedMarks ?? oldState.selection.$from.marks();
          deleteInfo.cursorPos = newState.selection.$from.pos;
        } else if (delta > 0 && deleteInfo.marks?.length) {
          // Insertion immediately after a deletion: restore saved marks
          const marks = deleteInfo.marks!;
          deleteInfo.marks = null;
          const from = deleteInfo.cursorPos;
          const to = newState.selection.$from.pos;

          if (to > from) {
            const newTr = newState.tr;
            let changed = false;
            marks.forEach((mark: any) => {
              const safePos = Math.max(
                0,
                Math.min(to - 1, newState.doc.content.size - 1)
              );
              const alreadyHas = newState.doc
                .resolve(safePos)
                .marks()
                .some((m: any) => m.type.name === mark.type.name);
              if (!alreadyHas) {
                newTr.addMark(from, to, mark);
                changed = true;
              }
            });
            if (changed) return newTr;
            if (marks.length > 0) return newState.tr.setStoredMarks(marks);
          }
        } else if (delta >= 0) {
          // Other transition: reset delete state
          deleteInfo.marks = null;
        }

        return null;
      },
    });

    // Composition event listeners (for IME mode)
    const onCompositionStart = () => {
      compositionInfo.marks =
        tiptap.state.storedMarks ?? tiptap.state.selection.$from.marks();
      compositionInfo.fromPos = tiptap.state.selection.$from.pos;
      compositionInfo.justEnded = false;
    };

    const onCompositionEnd = () => {
      compositionInfo.justEnded = true;
      // appendTransaction will fire immediately when ProseMirror syncs the DOM
    };

    tiptap.registerPlugin(plugin);
    container.addEventListener("compositionstart", onCompositionStart);
    container.addEventListener("compositionend", onCompositionEnd);

    return () => {
      try {
        tiptap.unregisterPlugin(pluginKey);
      } catch {
        // editor may already be destroyed
      }
      container.removeEventListener("compositionstart", onCompositionStart);
      container.removeEventListener("compositionend", onCompositionEnd);
    };
  }, [editor, containerRef]);
}
