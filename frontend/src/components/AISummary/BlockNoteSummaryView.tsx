"use client";

import { useState, useEffect, useCallback, useRef, forwardRef, useImperativeHandle } from 'react';
import dynamic from 'next/dynamic';
import { Summary, SummaryDataResponse, SummaryFormat, BlockNoteBlock } from '@/types';
import { AISummary } from './index';
import { Block } from '@blocknote/core';
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
} from '@blocknote/react';
import { BlockNoteView } from '@blocknote/shadcn';
import "@blocknote/shadcn/style.css";
import { useVnMarkPreservation } from '@/hooks/useVnMarkPreservation';

// Dynamically import BlockNote Editor to avoid SSR issues
const Editor = dynamic(() => import('../BlockNoteEditor/Editor'), { ssr: false });

interface BlockNoteSummaryViewProps {
  summaryData: SummaryDataResponse | Summary | null;
  onSave?: (data: { markdown?: string; summary_json?: BlockNoteBlock[] }) => void;
  onSummaryChange?: (summary: Summary) => void;
  status?: 'idle' | 'processing' | 'summarizing' | 'regenerating' | 'completed' | 'error';
  error?: string | null;
  onRegenerateSummary?: () => void;
  meeting?: {
    id: string;
    title: string;
    created_at: string;
  };
  onDirtyChange?: (isDirty: boolean) => void;
}

export interface BlockNoteSummaryViewRef {
  saveSummary: () => Promise<void>;
  getMarkdown: () => Promise<string>;
  getHTML: () => Promise<string>;
  /** Trả về blocks hiện tại (ưu tiên edits chưa save, fallback về data gốc) */
  getBlocks: () => Block[];
  getEditorElement: () => HTMLElement | null;
  /** Xuất nội dung hiện tại thành bytes DOCX qua BlockNote native exporter */
  exportToDocxBytes: () => Promise<Uint8Array>;
  /** Xuất nội dung hiện tại thành bytes PDF qua BlockNote native exporter */
  exportToPdfBytes: () => Promise<Uint8Array>;
  isDirty: boolean;
}

// Format detection helper
function detectSummaryFormat(data: any): { format: SummaryFormat; data: any } {
  if (!data) {
    return { format: 'legacy', data: null };
  }

  // Priority 1: BlockNote format (has summary_json)
  if (data.summary_json && Array.isArray(data.summary_json)) {
    console.log('✅ FORMAT: BLOCKNOTE (summary_json exists)');
    return { format: 'blocknote', data };
  }

  // Priority 2: Markdown format
  if (data.markdown && typeof data.markdown === 'string') {
    console.log('✅ FORMAT: MARKDOWN (will parse to BlockNote)');
    return { format: 'markdown', data };
  }

  // Priority 3: Legacy JSON
  const hasLegacyStructure = data.MeetingName || Object.keys(data).some(key =>
    typeof data[key] === 'object' && data[key]?.title && data[key]?.blocks
  );

  if (hasLegacyStructure) {
    console.log('✅ FORMAT: LEGACY (custom JSON)');
    return { format: 'legacy', data };
  }

  return { format: 'legacy', data: null };
}

export const BlockNoteSummaryView = forwardRef<BlockNoteSummaryViewRef, BlockNoteSummaryViewProps>(({
  summaryData,
  onSave,
  onSummaryChange,
  status = 'idle',
  error = null,
  onRegenerateSummary,
  meeting,
  onDirtyChange
}, ref) => {
  const { format, data } = detectSummaryFormat(summaryData);
  const [isDirty, setIsDirty] = useState(false);
  const [currentBlocks, setCurrentBlocks] = useState<Block[]>([]);
  const [isSaving, setIsSaving] = useState(false);
  const isContentLoaded = useRef(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const markdownEditorRef = useRef<HTMLDivElement>(null);

  // Stable key to force Editor remount khi summary_json thay đổi (tránh stale initialContent)
  const editorKey = format === 'blocknote' && data?.summary_json
    ? (data.summary_json as Block[]).map((b: any) => b.id).join(',')
    : 'empty';

  // Create BlockNote editor for markdown parsing
  const editor = useCreateBlockNote({
    initialContent: undefined
  });

  // Fix: Preserve bold/italic marks when typing Vietnamese diacritics (markdown format editor)
  useVnMarkPreservation(editor, markdownEditorRef);

  // Parse markdown to blocks when format is markdown
  useEffect(() => {
    if (format === 'markdown' && data?.markdown && editor) {
      const loadMarkdown = async () => {
        try {
          console.log('📝 Parsing markdown to BlockNote blocks...');
          const blocks = await editor.tryParseMarkdownToBlocks(data.markdown);
          editor.replaceBlocks(editor.document, blocks);
          console.log('✅ Markdown parsed successfully');

          // Delay to ensure editor has finished rendering before allowing onChange
          setTimeout(() => {
            isContentLoaded.current = true;
          }, 100);
        } catch (err) {
          console.error('❌ Failed to parse markdown:', err);
        }
      };
      loadMarkdown();
    }
  }, [format, data?.markdown, editor]);

  // Reset state khi summary mới được load (ví dụ sau khi regenerate)
  useEffect(() => {
    isContentLoaded.current = false;
    setIsDirty(false);
    setCurrentBlocks([]);
  }, [editorKey]);

  // Set content loaded flag for blocknote format
  useEffect(() => {
    if (format === 'blocknote' && data?.summary_json) {
      // Delay to ensure editor has finished rendering
      setTimeout(() => {
        isContentLoaded.current = true;
      }, 150);
    }
  }, [format, data?.summary_json]);

  const handleEditorChange = useCallback((blocks: Block[]) => {
    // Only set dirty flag if content has finished loading
    if (isContentLoaded.current) {
      setCurrentBlocks(blocks);
      setIsDirty(true);
    }
  }, []);

  // Notify parent of dirty state changes
  useEffect(() => {
    if (onDirtyChange) {
      onDirtyChange(isDirty);
    }
  }, [isDirty, onDirtyChange]);

  const handleSave = useCallback(async () => {
    if (!onSave || !isDirty) return;

    setIsSaving(true);
    try {
      console.log('💾 Saving BlockNote content...');

      // Generate markdown from current blocks
      const markdown = await editor.blocksToMarkdownLossy(currentBlocks);

      onSave({
        markdown: markdown,
        summary_json: currentBlocks as unknown as BlockNoteBlock[]
      });

      setIsDirty(false);
      console.log('✅ Save successful');
    } catch (err) {
      console.error('❌ Save failed:', err);
      alert('Failed to save changes. Please try again.');
    } finally {
      setIsSaving(false);
    }
  }, [onSave, isDirty, currentBlocks, editor]);

  // Expose methods to parent via ref
  useImperativeHandle(ref, () => ({
    saveSummary: handleSave,
    getMarkdown: async () => {
      try {
        console.log('🔍 getMarkdown called, format:', format);
        console.log('🔍 currentBlocks length:', currentBlocks.length);
        console.log('🔍 data:', data);

        // For markdown format - use the main editor
        if (format === 'markdown' && editor) {
          console.log('📝 Using markdown editor, blocks:', editor.document.length);
          const markdown = await editor.blocksToMarkdownLossy(editor.document);
          console.log('📝 Generated markdown length:', markdown.length);
          return markdown;
        }

        // For blocknote format - use currentBlocks state
        if (format === 'blocknote') {
          console.log('📝 BlockNote format, currentBlocks:', currentBlocks.length);
          // Ưu tiên: blocks đang được edit
          const blocksToUse = currentBlocks.length > 0
            ? currentBlocks
            : (data?.summary_json as Block[] | undefined);
          if (blocksToUse && blocksToUse.length > 0 && editor) {
            const markdown = await editor.blocksToMarkdownLossy(blocksToUse);
            console.log('📝 Generated markdown from blocks, length:', markdown.length);
            return markdown;
          }
          // Fallback cuối: nếu backend đã lưu sẵn markdown
          if (data?.markdown) {
            console.log('📝 Using fallback markdown from data');
            return data.markdown;
          }
        }

        // For legacy format - return empty (handled by parent)
        console.warn('⚠️ Cannot generate markdown for legacy format, returning empty');
        return '';
      } catch (err) {
        console.error('❌ Failed to generate markdown:', err);
        return '';
      }
    },
    getBlocks: (): Block[] => {
      if (format === 'markdown') return editor.document;
      if (currentBlocks.length > 0) return currentBlocks;
      return (data?.summary_json as Block[] | undefined) ?? [];
    },
    exportToDocxBytes: async (): Promise<Uint8Array> => {
      const { DOCXExporter, docxDefaultSchemaMappings } = await import('@blocknote/xl-docx-exporter');
      const { Packer } = await import('docx');

      // Lấy blocks hiện tại theo format
      let blocks: Block[];
      if (format === 'markdown') {
        blocks = editor.document;
      } else if (currentBlocks.length > 0) {
        blocks = currentBlocks;
      } else {
        blocks = (data?.summary_json as Block[] | undefined) ?? [];
      }

      if (!blocks.length) throw new Error('Không có nội dung để xuất');

      const exporter = new DOCXExporter(editor.schema, docxDefaultSchemaMappings);
      const docxDoc = await exporter.toDocxJsDocument(blocks as any);
      return new Uint8Array(await Packer.toArrayBuffer(docxDoc));
    },
    exportToPdfBytes: async (): Promise<Uint8Array> => {
      const { PDFExporter, pdfDefaultSchemaMappings } = await import('@blocknote/xl-pdf-exporter');
      const ReactPDF = await import('@react-pdf/renderer');

      // Lấy blocks hiện tại theo format
      let blocks: Block[];
      if (format === 'markdown') {
        blocks = editor.document;
      } else if (currentBlocks.length > 0) {
        blocks = currentBlocks;
      } else {
        blocks = (data?.summary_json as Block[] | undefined) ?? [];
      }

      if (!blocks.length) throw new Error('Không có nội dung để xuất');

      const exporter = new PDFExporter(editor.schema, pdfDefaultSchemaMappings);
      const pdfDocument = await exporter.toReactPDFDocument(blocks as any);
      const blob = await ReactPDF.pdf(pdfDocument).toBlob();
      return new Uint8Array(await blob.arrayBuffer());
    },
    getHTML: async () => {
      try {
        if (!editor) return '';
        let blocks: Block[];
        if (format === 'markdown') {
          blocks = editor.document;
        } else if (currentBlocks.length > 0) {
          blocks = currentBlocks;
        } else {
          blocks = (data?.summary_json as Block[] | undefined) ?? [];
        }
        if (!blocks.length) return '';
        return await editor.blocksToHTMLLossy(blocks);
      } catch (err) {
        console.error('❌ Failed to generate HTML:', err);
        return '';
      }
    },
    getEditorElement: () => containerRef.current,
    isDirty
  }), [handleSave, isDirty, editor, format, currentBlocks, data]);

  // Render legacy format
  if (format === 'legacy') {
    console.log('🎨 Rendering LEGACY format');
    return (
      <AISummary
        summary={summaryData as Summary}
        status={status}
        error={error}
        onSummaryChange={onSummaryChange || (() => { })}
        onRegenerateSummary={onRegenerateSummary || (() => { })}
        meeting={meeting}
      />
    );
  }

  // Render BlockNote format (has summary_json)
  if (format === 'blocknote') {
    console.log('🎨 Rendering BLOCKNOTE format (direct)');
    return (
      <div ref={containerRef} className="flex flex-col w-full">
        <div className="w-full">
          <Editor
            key={editorKey}
            initialContent={data.summary_json}
            onChange={(blocks) => {
              console.log('📝 Editor blocks changed:', blocks.length);
              handleEditorChange(blocks);
            }}
            editable={true}
          />
        </div>
      </div>
    );
  }

  // Render Markdown format (parse and display in BlockNote)
  if (format === 'markdown') {
    console.log('🎨 Rendering MARKDOWN format (parsed to BlockNote)');
    return (
      <div ref={containerRef} className="flex flex-col w-full">
        <div ref={markdownEditorRef} className="w-full">
          <BlockNoteView
            editor={editor}
            editable={true}
            onChange={() => {
              if (isContentLoaded.current) {
                handleEditorChange(editor.document);
              }
            }}
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
      </div>
    );
  }

  return null;
});

BlockNoteSummaryView.displayName = 'BlockNoteSummaryView';
