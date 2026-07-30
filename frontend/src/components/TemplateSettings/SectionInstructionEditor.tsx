'use client';

import { useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
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
        toast.error('Không tải được nội dung chỉ dẫn AI của phần này — vui lòng kiểm tra lại trước khi lưu.');
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
