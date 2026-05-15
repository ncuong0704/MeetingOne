"use client";

import { useState, useEffect, useCallback, useRef, forwardRef, useImperativeHandle, Component, type ReactNode } from 'react';
import dynamic from 'next/dynamic';
import { Summary, SummaryDataResponse, SummaryFormat, BlockNoteBlock } from '@/types';
import { AISummary } from './index';
import { Block } from '@blocknote/core';
import { useCreateBlockNote } from '@blocknote/react';
import "@blocknote/shadcn/style.css";

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

// Error boundary to prevent BlockNote render errors from crashing the whole app
class BlockNoteErrorBoundary extends Component<
  { children: ReactNode; fallback: ReactNode },
  { hasError: boolean }
> {
  constructor(props: any) {
    super(props);
    this.state = { hasError: false };
  }
  static getDerivedStateFromError() {
    return { hasError: true };
  }
  componentDidCatch(error: Error) {
    console.error('❌ BlockNote render error caught by boundary:', error);
  }
  render() {
    if (this.state.hasError) return this.props.fallback;
    return this.props.children;
  }
}

// Known block types supported by BlockNote's default schema
const KNOWN_BLOCK_TYPES = new Set([
  'paragraph', 'heading', 'bulletListItem', 'numberedListItem',
  'checkListItem', 'image', 'video', 'audio', 'file', 'table', 'codeBlock'
]);

function sanitizeInlineContent(items: any[]): any[] {
  return items
    .filter(item => item != null)
    .map(item => {
      if (typeof item === 'string') {
        return { type: 'text', text: item, styles: {} };
      }
      if (typeof item !== 'object') {
        return { type: 'text', text: String(item), styles: {} };
      }
      if (item.type === 'link') {
        return {
          type: 'link',
          href: typeof item.href === 'string' ? item.href : '',
          content: Array.isArray(item.content)
            ? sanitizeInlineContent(item.content)
            : [],
        };
      }
      // Default: treat as text (covers type:"text" and unknown types)
      return {
        type: 'text',
        text: typeof item.text === 'string' ? item.text : '',
        styles: item.styles && typeof item.styles === 'object' ? item.styles : {},
      };
    });
}

function sanitizeProps(type: string, props: any): Record<string, any> {
  const p = props && typeof props === 'object' ? { ...props } : {};
  // heading: level must be number 1 | 2 | 3
  if (type === 'heading') {
    const lvl = Number(p.level);
    p.level = (lvl === 1 || lvl === 2 || lvl === 3) ? lvl : 1;
  }
  // checkListItem: checked must be boolean
  if (type === 'checkListItem') {
    p.checked = p.checked === true || p.checked === 'true';
  }
  // numberedListItem: start must be number
  if (type === 'numberedListItem' && p.start !== undefined) {
    p.start = Number(p.start) || 1;
  }
  // textAlignment must be valid
  if (p.textAlignment !== undefined) {
    const validAlignments = ['left', 'center', 'right', 'justify'];
    if (!validAlignments.includes(p.textAlignment)) {
      p.textAlignment = 'left';
    }
  }
  return p;
}

function sanitizeBlocks(blocks: any[]): any[] {
  if (!Array.isArray(blocks)) return [];
  return blocks
    .filter(block => block != null && typeof block === 'object')
    .map(block => {
      const type = KNOWN_BLOCK_TYPES.has(block.type) ? block.type : 'paragraph';
      // table blocks have their own content structure — don't flatten
      const content = type === 'table'
        ? (block.content ?? { type: 'tableContent', rows: [] })
        : Array.isArray(block.content)
          ? sanitizeInlineContent(block.content)
          : typeof block.content === 'string'
            ? [{ type: 'text', text: block.content, styles: {} }]
            : [];
      const children = Array.isArray(block.children)
        ? sanitizeBlocks(block.children)
        : [];
      return {
        ...block,
        type,
        content,
        children,
        props: sanitizeProps(type, block.props),
        id: typeof block.id === 'string' && block.id ? block.id : crypto.randomUUID(),
      };
    });
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

  // Markdown format: parsed blocks + key to force Editor remount
  const [mdBlocks, setMdBlocks] = useState<Block[] | null>(null);
  const [mdKey, setMdKey] = useState(0);

  // Stable key to force Editor remount khi summary_json thay đổi (tránh stale initialContent)
  const editorKey = format === 'blocknote' && data?.summary_json
    ? (data.summary_json as Block[]).map((b: any) => b.id ?? '').join(',') || 'blocknote'
    : 'empty';

  // Parser-only editor — never rendered, only used for tryParseMarkdownToBlocks
  const parserEditor = useCreateBlockNote({ initialContent: undefined });

  // Parse markdown → store as initialContent for <Editor> (avoids replaceBlocks + renderSpec crash)
  useEffect(() => {
    if (format !== 'markdown' || !data?.markdown || !parserEditor) return;
    let cancelled = false;
    const parse = async () => {
      try {
        const raw = await parserEditor.tryParseMarkdownToBlocks(data.markdown);
        if (cancelled) return;
        setMdBlocks(sanitizeBlocks(raw));
        setMdKey(k => k + 1);
        setTimeout(() => { isContentLoaded.current = true; }, 100);
      } catch (err) {
        console.error('❌ Failed to parse markdown:', err);
      }
    };
    parse();
    return () => { cancelled = true; };
  }, [format, data?.markdown, parserEditor]);

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
      const markdown = await parserEditor.blocksToMarkdownLossy(currentBlocks);

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
  }, [onSave, isDirty, currentBlocks, parserEditor]);

  // Expose methods to parent via ref
  useImperativeHandle(ref, () => ({
    saveSummary: handleSave,
    getMarkdown: async () => {
      try {
        console.log('🔍 getMarkdown called, format:', format);
        console.log('🔍 currentBlocks length:', currentBlocks.length);
        console.log('🔍 data:', data);

        // For markdown format - use currentBlocks (edited) or mdBlocks (initial)
        if (format === 'markdown') {
          const blocks = currentBlocks.length > 0 ? currentBlocks : (mdBlocks ?? []);
          if (blocks.length > 0 && parserEditor) {
            return await parserEditor.blocksToMarkdownLossy(blocks);
          }
          return data?.markdown || '';
        }

        // For blocknote format - use currentBlocks state
        if (format === 'blocknote') {
          console.log('📝 BlockNote format, currentBlocks:', currentBlocks.length);
          // Ưu tiên: blocks đang được edit
          const blocksToUse = currentBlocks.length > 0
            ? currentBlocks
            : (data?.summary_json as Block[] | undefined);
          if (blocksToUse && blocksToUse.length > 0 && parserEditor) {
            const markdown = await parserEditor.blocksToMarkdownLossy(blocksToUse);
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
      if (format === 'markdown') return currentBlocks.length > 0 ? currentBlocks : (mdBlocks ?? []);
      if (currentBlocks.length > 0) return currentBlocks;
      return (data?.summary_json as Block[] | undefined) ?? [];
    },
    exportToDocxBytes: async (): Promise<Uint8Array> => {
      const { DOCXExporter, docxDefaultSchemaMappings } = await import('@blocknote/xl-docx-exporter');
      const { Packer } = await import('docx');

      let blocks: Block[];
      if (format === 'markdown') {
        blocks = currentBlocks.length > 0 ? currentBlocks : (mdBlocks ?? []);
      } else if (currentBlocks.length > 0) {
        blocks = currentBlocks;
      } else {
        blocks = (data?.summary_json as Block[] | undefined) ?? [];
      }

      if (!blocks.length) throw new Error('Không có nội dung để xuất');

      const exporter = new DOCXExporter(parserEditor.schema, docxDefaultSchemaMappings);
      const docxDoc = await exporter.toDocxJsDocument(blocks as any);
      return new Uint8Array(await Packer.toArrayBuffer(docxDoc));
    },
    exportToPdfBytes: async (): Promise<Uint8Array> => {
      const { PDFExporter, pdfDefaultSchemaMappings } = await import('@blocknote/xl-pdf-exporter');
      const ReactPDF = await import('@react-pdf/renderer');

      // Lấy blocks hiện tại theo format
      let blocks: Block[];
      if (format === 'markdown') {
        blocks = currentBlocks.length > 0 ? currentBlocks : (mdBlocks ?? []);
      } else if (currentBlocks.length > 0) {
        blocks = currentBlocks;
      } else {
        blocks = (data?.summary_json as Block[] | undefined) ?? [];
      }

      if (!blocks.length) throw new Error('Không có nội dung để xuất');

      const exporter = new PDFExporter(parserEditor.schema, pdfDefaultSchemaMappings);
      const pdfDocument = await exporter.toReactPDFDocument(blocks as any);
      const blob = await ReactPDF.pdf(pdfDocument).toBlob();
      return new Uint8Array(await blob.arrayBuffer());
    },
    getHTML: async () => {
      try {
        if (!parserEditor) return '';
        let blocks: Block[];
        if (format === 'markdown') {
          blocks = currentBlocks.length > 0 ? currentBlocks : (mdBlocks ?? []);
        } else if (currentBlocks.length > 0) {
          blocks = currentBlocks;
        } else {
          blocks = (data?.summary_json as Block[] | undefined) ?? [];
        }
        if (!blocks.length) return '';
        return await parserEditor.blocksToHTMLLossy(blocks);
      } catch (err) {
        console.error('❌ Failed to generate HTML:', err);
        return '';
      }
    },
    getEditorElement: () => containerRef.current,
    isDirty
  }), [handleSave, isDirty, parserEditor, format, currentBlocks, data, mdBlocks]);

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

  const editorFallback = (
    <div className="p-4 rounded-lg bg-amber-50 border border-amber-200 text-amber-800 text-sm">
      <p className="font-medium">Không thể hiển thị trình soạn thảo.</p>
      <p className="mt-1 text-xs text-amber-600">Tóm tắt đã được lưu. Vui lòng thử tạo lại.</p>
    </div>
  );

  // Render BlockNote format (has summary_json)
  if (format === 'blocknote') {
    console.log('🎨 Rendering BLOCKNOTE format (direct)');
    return (
      <div ref={containerRef} className="flex flex-col w-full">
        <div className="w-full">
          <BlockNoteErrorBoundary fallback={editorFallback}>
            <Editor
              key={editorKey}
              initialContent={sanitizeBlocks(data.summary_json)}
              onChange={(blocks) => {
                handleEditorChange(blocks);
              }}
              editable={true}
            />
          </BlockNoteErrorBoundary>
        </div>
      </div>
    );
  }

  // Render Markdown format — use <Editor initialContent={mdBlocks}> to avoid replaceBlocks+renderSpec crash
  if (format === 'markdown') {
    console.log('🎨 Rendering MARKDOWN format (via Editor initialContent)');
    return (
      <div ref={containerRef} className="flex flex-col w-full">
        <BlockNoteErrorBoundary fallback={editorFallback}>
          <Editor
            key={mdKey}
            initialContent={mdBlocks ?? undefined}
            onChange={handleEditorChange}
            editable={true}
          />
        </BlockNoteErrorBoundary>
      </div>
    );
  }

  return null;
});

BlockNoteSummaryView.displayName = 'BlockNoteSummaryView';
