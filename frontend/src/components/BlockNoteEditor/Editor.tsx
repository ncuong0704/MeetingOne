"use client";

import { useEffect, useRef } from "react";
import {
  useCreateBlockNote,
  FormattingToolbar,
  FormattingToolbarController,
  BlockTypeSelect,
  BasicTextStyleButton,
  ColorStyleButton,
  CreateLinkButton,
  NestBlockButton,
  UnnestBlockButton,
  TextAlignButton,
} from "@blocknote/react";
import { BlockNoteView } from "@blocknote/shadcn";
import { PartialBlock, Block } from "@blocknote/core";
import "@blocknote/shadcn/style.css";
import "@blocknote/core/fonts/inter.css";
import { useVnMarkPreservation } from "@/hooks/useVnMarkPreservation";

interface EditorProps {
  initialContent?: Block[];
  onChange?: (blocks: Block[]) => void;
  editable?: boolean;
}

export default function Editor({ initialContent, onChange, editable = true }: EditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  const safeInitialContent = (initialContent && initialContent.length > 0)
    ? initialContent as PartialBlock[]
    : undefined;

  console.group('[BlockNote Editor] init');
  console.log('initialContent length:', initialContent?.length ?? 0);
  if (initialContent && initialContent.length > 0) {
    initialContent.forEach((block, i) => {
      // Use JSON.stringify so the full structure is visible even in production minified builds
      try {
        console.log(`  block[${i}] JSON:`, JSON.stringify(block, null, 2));
      } catch {
        console.log(`  block[${i}] (not serializable):`, block);
      }
    });
  }
  console.groupEnd();

  const editor = useCreateBlockNote({
    // Treat an empty array the same as undefined — ProseMirror requires at least one
    // paragraph node in a document, so passing [] triggers renderSpec with invalid spec.
    initialContent: safeInitialContent,
  });

  useVnMarkPreservation(editor, containerRef);

  const onChangeRef = useRef(onChange);
  useEffect(() => {
    onChangeRef.current = onChange;
  });

  useEffect(() => {
    const handleChange = () => {
      onChangeRef.current?.(editor.document);
    };
    const unsubscribe = editor.onChange(handleChange);
    return () => {
      if (typeof unsubscribe === "function") unsubscribe();
    };
  }, [editor]);

  return (
    <div ref={containerRef}>
      <BlockNoteView
        editor={editor}
        editable={editable}
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
              <BasicTextStyleButton basicTextStyle="underline" key="underlineStyleButton" />
              <BasicTextStyleButton basicTextStyle="strike" key="strikeStyleButton" />

              {/* Text & background color picker */}
              <ColorStyleButton key="colorStyleButton" />

              <TextAlignButton textAlignment="left" key="textAlignLeftButton" />
              <TextAlignButton textAlignment="center" key="textAlignCenterButton" />
              <TextAlignButton textAlignment="right" key="textAlignRightButton" />

              <NestBlockButton key="nestBlockButton" />
              <UnnestBlockButton key="unnestBlockButton" />

              <CreateLinkButton key="createLinkButton" />
            </FormattingToolbar>
          )}
        />
      </BlockNoteView>
    </div>
  );
}
