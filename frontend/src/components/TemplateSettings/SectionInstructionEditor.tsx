'use client';

import { useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import {
  useCreateBlockNote,
  FormattingToolbar,
  FormattingToolbarController,
  BlockTypeSelect,
  blockTypeSelectItems,
  BasicTextStyleButton,
  NestBlockButton,
  UnnestBlockButton,
  SuggestionMenuController,
  getDefaultReactSlashMenuItems,
  SideMenuController,
  SideMenu,
  DragHandleButton,
  DragHandleMenu,
  RemoveBlockItem,
} from '@blocknote/react';
import { BlockNoteView } from '@blocknote/shadcn';
import { BlockNoteSchema, defaultBlockSpecs, filterSuggestionItems } from '@blocknote/core';
import type { DefaultReactSuggestionItem } from '@blocknote/react';
import '@blocknote/shadcn/style.css';
import '@blocknote/core/fonts/inter.css';

// Restricted schema: only paragraph, heading, and bullet list are valid block
// types at the data-model level (not just hidden from a menu). Heading itself
// still nominally allows levels 4-6 / isToggleable in its propSchema (no easy
// way to trim that without reimplementing the whole block spec), so levels
// 4-6 and toggle headings are excluded at the UI layer only (BlockTypeSelect
// items + slash menu items below), not at the schema level.
const schema = BlockNoteSchema.create({
  blockSpecs: {
    paragraph: defaultBlockSpecs.paragraph,
    heading: defaultBlockSpecs.heading,
    bulletListItem: defaultBlockSpecs.bulletListItem,
  },
});

type EditorBlock = typeof schema.Block;
type EditorPartialBlock = typeof schema.PartialBlock;

const isAllowedBlockTypeItem = (item: { type: string; props?: Record<string, unknown> }) =>
  item.type === 'paragraph' ||
  item.type === 'bulletListItem' ||
  (item.type === 'heading' &&
    !item.props?.isToggleable &&
    typeof item.props?.level === 'number' &&
    item.props.level <= 3);

const ALLOWED_SLASH_MENU_KEYS = new Set(['paragraph', 'heading', 'heading_2', 'heading_3', 'bullet_list']);

// `getDefaultReactSlashMenuItems` returns items typed as `Omit<DefaultSuggestionItem, "key">`,
// but at runtime each item is built by spreading the underlying (key-bearing)
// `DefaultSuggestionItem` — see @blocknote/react's `getDefaultReactSlashMenuItems`
// implementation, which does `{...item, icon: ...}` on top of
// `getDefaultSlashMenuItems()` results. So `key` is present on the actual
// objects even though the public type omits it; we widen the type locally
// instead of casting to `any`.
type SlashMenuItemWithKey = DefaultReactSuggestionItem & { key: string };

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
  initialBlocks: EditorBlock[];
  onChange: (markdown: string) => void;
  disabled?: boolean;
}) {
  const editor = useCreateBlockNote({
    schema,
    initialContent: initialBlocks.length ? (initialBlocks as EditorPartialBlock[]) : undefined,
  });

  const onChangeRef = useRef(onChange);
  useEffect(() => {
    onChangeRef.current = onChange;
  });

  useEffect(() => {
    let cancelled = false;
    let latestSeq = 0;
    const handleChange = () => {
      const seq = ++latestSeq;
      editor.blocksToMarkdownLossy(editor.document).then(markdown => {
        if (!cancelled && seq === latestSeq) onChangeRef.current(markdown);
      });
    };
    const unsubscribe = editor.onChange(handleChange);
    return () => {
      cancelled = true;
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
        slashMenu={false}
        sideMenu={false}
      >
        <FormattingToolbarController
          formattingToolbar={() => (
            <FormattingToolbar>
              <BlockTypeSelect
                key="blockTypeSelect"
                items={blockTypeSelectItems(editor.dictionary).filter(isAllowedBlockTypeItem)}
              />
              <BasicTextStyleButton basicTextStyle="bold" key="boldStyleButton" />
              <BasicTextStyleButton basicTextStyle="italic" key="italicStyleButton" />
              <NestBlockButton key="nestBlockButton" />
              <UnnestBlockButton key="unnestBlockButton" />
            </FormattingToolbar>
          )}
        />
        <SuggestionMenuController
          triggerCharacter="/"
          getItems={async query =>
            filterSuggestionItems(
              (getDefaultReactSlashMenuItems(editor) as SlashMenuItemWithKey[]).filter(item =>
                ALLOWED_SLASH_MENU_KEYS.has(item.key)
              ),
              query
            )
          }
        />
        <SideMenuController
          sideMenu={sideMenuProps => (
            <SideMenu {...sideMenuProps}>
              <DragHandleButton
                {...sideMenuProps}
                dragHandleMenu={dragHandleMenuProps => (
                  <DragHandleMenu {...dragHandleMenuProps}>
                    <RemoveBlockItem {...dragHandleMenuProps}>Xóa</RemoveBlockItem>
                  </DragHandleMenu>
                )}
              />
            </SideMenu>
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
  const parserEditor = useCreateBlockNote({ schema, initialContent: undefined });
  const [initialBlocks, setInitialBlocks] = useState<EditorBlock[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const blocks = await parserEditor.tryParseMarkdownToBlocks(value);
        if (!cancelled) setInitialBlocks(blocks as EditorBlock[]);
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
