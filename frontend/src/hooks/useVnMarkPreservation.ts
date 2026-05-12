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

// Validate a mark is usable in the given state's schema
function isMarkValidInSchema(mark: any, state: any): boolean {
  try {
    if (!mark || typeof mark !== "object") return false;
    if (!mark.type || typeof mark.type !== "object") return false;
    if (typeof mark.type.name !== "string") return false;
    // Check the mark type exists in the current schema
    return !!state.schema.marks[mark.type.name];
  } catch {
    return false;
  }
}

// Re-create a mark using the current schema to avoid cross-schema errors
function resolveMarkInSchema(mark: any, state: any): any {
  try {
    const markType = state.schema.marks[mark.type.name];
    if (!markType) return null;
    return markType.create(mark.attrs ?? {});
  } catch {
    return null;
  }
}

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
        try {
          if (!transactions.some((tr) => tr.docChanged)) return null;

          // ── Case A: After IME composition end ────────────────────────────
          if (compositionInfo.justEnded && compositionInfo.marks?.length) {
            compositionInfo.justEnded = false;
            const rawMarks = compositionInfo.marks!;
            compositionInfo.marks = null;
            const from = compositionInfo.fromPos;
            const to = newState.selection.$from.pos;

            // Resolve marks against current schema to avoid cross-schema errors
            const marks = rawMarks
              .filter((m) => isMarkValidInSchema(m, newState))
              .map((m) => resolveMarkInSchema(m, newState))
              .filter(Boolean);

            if (to > from && marks.length > 0) {
              const newTr = newState.tr;
              let changed = false;
              for (const mark of marks) {
                try {
                  const safePos = Math.max(0, Math.min(to - 1, newState.doc.content.size - 1));
                  const alreadyHas = newState.doc
                    .resolve(safePos)
                    .marks()
                    .some((m: any) => m.type.name === mark.type.name);
                  if (!alreadyHas) {
                    newTr.addMark(from, to, mark);
                    changed = true;
                  }
                } catch {
                  // skip this mark if it causes issues
                }
              }
              if (changed) {
                deleteInfo.marks = null;
                return newTr;
              }
            }
            // Even if range not found, set storedMarks for continuity
            deleteInfo.marks = null;
            if (marks.length > 0) {
              try {
                return newState.tr.setStoredMarks(marks);
              } catch {
                return null;
              }
            }
            return null;
          }

          // ── Case B: Delete → Insert sequence (Unikey backspace mode) ─────
          const delta = newState.doc.nodeSize - oldState.doc.nodeSize;

          if (delta < 0) {
            // Pure deletion: save marks from just before this transaction
            try {
              deleteInfo.marks =
                oldState.storedMarks ?? oldState.selection.$from.marks();
              deleteInfo.cursorPos = newState.selection.$from.pos;
            } catch {
              deleteInfo.marks = null;
            }
          } else if (delta > 0 && deleteInfo.marks?.length) {
            // Insertion immediately after a deletion: restore saved marks
            const rawMarks = deleteInfo.marks!;
            deleteInfo.marks = null;

            // Resolve marks against current schema
            const marks = rawMarks
              .filter((m) => isMarkValidInSchema(m, newState))
              .map((m) => resolveMarkInSchema(m, newState))
              .filter(Boolean);

            if (marks.length === 0) return null;

            const from = deleteInfo.cursorPos;
            const to = newState.selection.$from.pos;

            if (to > from) {
              const newTr = newState.tr;
              let changed = false;
              for (const mark of marks) {
                try {
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
                } catch {
                  // skip this mark
                }
              }
              if (changed) return newTr;
              try {
                return newState.tr.setStoredMarks(marks);
              } catch {
                return null;
              }
            }
          } else if (delta >= 0) {
            // Other transition: reset delete state
            deleteInfo.marks = null;
          }

          return null;
        } catch (err) {
          // Prevent any plugin error from crashing ProseMirror's transaction pipeline
          console.warn("[useVnMarkPreservation] appendTransaction error:", err);
          return null;
        }
      },
    });

    // Composition event listeners (for IME mode)
    const onCompositionStart = () => {
      try {
        compositionInfo.marks =
          tiptap.state.storedMarks ?? tiptap.state.selection.$from.marks();
        compositionInfo.fromPos = tiptap.state.selection.$from.pos;
        compositionInfo.justEnded = false;
      } catch {
        compositionInfo.marks = null;
      }
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
