import { useCallback, RefObject } from 'react';
import { Transcript, Summary } from '@/types';
import { BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { copyRichText, wrapWordHtml } from '@/lib/clipboardUtils';
import { formatTranscriptHtml, formatTranscriptPlainText } from '@/lib/transcriptDisplay';
import { blocksToWordHtml } from '@/lib/blockNoteToWordHtml';

function escapeHtmlAttr(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

interface UseCopyOperationsProps {
  meeting: any;
  transcripts: Transcript[];
  meetingTitle: string;
  aiSummary: Summary | null;
  blockNoteSummaryRef: RefObject<BlockNoteSummaryViewRef>;
}

export function useCopyOperations({
  meeting,
  transcripts,
  meetingTitle,
  aiSummary,
  blockNoteSummaryRef,
}: UseCopyOperationsProps) {

  // Lấy toàn bộ transcripts từ DB (không bị giới hạn pagination)
  const fetchAllTranscripts = useCallback(async (meetingId: string): Promise<Transcript[]> => {
    try {
      const firstPage = await invokeTauri('api_get_meeting_transcripts', {
        meetingId, limit: 1, offset: 0,
      }) as { transcripts: Transcript[]; total_count: number; has_more: boolean };

      const totalCount = firstPage.total_count;
      if (totalCount === 0) return [];

      const allData = await invokeTauri('api_get_meeting_transcripts', {
        meetingId, limit: totalCount, offset: 0,
      }) as { transcripts: Transcript[]; total_count: number; has_more: boolean };

      return allData.transcripts;
    } catch (error) {
      console.error('❌ Error fetching all transcripts:', error);
      toast.error('Không tải được bản ghi để sao chép');
      return [];
    }
  }, []);

  // ── Copy bản ghi ─────────────────────────────────────────────────────────────
  const handleCopyTranscript = useCallback(async () => {
    const allTranscripts = await fetchAllTranscripts(meeting.id);

    if (!allTranscripts.length) {
      toast.error('Không có bản ghi để sao chép');
      return;
    }

    const segments = allTranscripts.map((t) => ({
      id: t.id,
      text: t.text,
      speakerId: t.speaker_id ?? null,
      speakerName: t.speaker_name ?? null,
    }));
    const plainText = formatTranscriptPlainText(segments);
    const bodyHtml = formatTranscriptHtml(segments);

    await copyRichText(wrapWordHtml(bodyHtml), plainText);
    toast.success('Đã sao chép bản ghi vào bảng nhớ tạm');

    const wordCount = allTranscripts
      .map(t => t.text.split(/\s+/).length)
      .reduce((a, b) => a + b, 0);
    await Analytics.trackCopy('transcript', {
      meeting_id: meeting.id,
      transcript_length: allTranscripts.length.toString(),
      word_count: wordCount.toString(),
    });
  }, [meeting, fetchAllTranscripts]);

  // ── Copy tóm tắt ─────────────────────────────────────────────────────────────
  const handleCopySummary = useCallback(async () => {
    try {
      // Lấy blocks trực tiếp từ editor (giữ màu, định dạng đầy đủ)
      const blocks = blockNoteSummaryRef.current?.getBlocks?.() ?? [];

      // Fallback: legacy format không có BlockNote editor
      if (!blocks.length && aiSummary) {
        const sections = Object.entries(aiSummary)
          .filter(([key]) => !['markdown', 'summary_json', '_section_order', 'MeetingName'].includes(key))
          .map(([, section]: [string, any]) => {
            if (section?.title && Array.isArray(section.blocks)) {
              return `## ${section.title}\n\n` + section.blocks.map((b: any) => `- ${b.content}`).join('\n');
            }
            return '';
          })
          .filter(Boolean)
          .join('\n\n');

        if (!sections.trim()) {
          toast.error('Không có nội dung tóm tắt để sao chép');
          return;
        }
        await copyRichText(
          wrapWordHtml(`<pre style="font-family:Calibri,sans-serif;white-space:pre-wrap">${sections}</pre>`),
          sections
        );
        toast.success('Đã sao chép tóm tắt vào bảng nhớ tạm');
        return;
      }

      if (!blocks.length) {
        toast.error('Không có nội dung tóm tắt để sao chép');
        return;
      }

      const dateStr = new Date(meeting.created_at).toLocaleDateString('vi-VN', {
        year: 'numeric', month: 'long', day: 'numeric', hour: '2-digit', minute: '2-digit',
      });

      // Plain text qua markdown (chấp nhận lossy cho text/plain)
      let plainMarkdown = '';
      try {
        plainMarkdown = (await blockNoteSummaryRef.current?.getMarkdown?.()) ?? '';
      } catch { /* ignore */ }
      const plainText =
        `# Tóm tắt cuộc họp: ${meetingTitle}\n\nNgày họp: ${dateStr}\n\n---\n\n${plainMarkdown}`;

      // Rich HTML render trực tiếp từ blocks — giữ màu text/background
      const bodyHtml =
        `<h1 style="font-family:Calibri,'Segoe UI',Arial,sans-serif;font-size:18pt;font-weight:bold;color:#111;margin:0 0 6pt;">` +
        `Tóm tắt cuộc họp: ${escapeHtmlAttr(meetingTitle)}</h1>` +
        `<p style="font-family:Calibri,'Segoe UI',Arial,sans-serif;font-size:10pt;color:#666;margin:0 0 10pt;">Ngày họp: ${escapeHtmlAttr(dateStr)}</p>` +
        `<hr style="border:none;border-top:1pt solid #ccc;margin:8pt 0;">` +
        blocksToWordHtml(blocks);

      await copyRichText(wrapWordHtml(bodyHtml), plainText);
      toast.success('Đã sao chép tóm tắt vào bảng nhớ tạm');

      await Analytics.trackCopy('summary', {
        meeting_id: meeting.id,
        has_markdown: (!!aiSummary && 'markdown' in aiSummary).toString(),
      });
    } catch (error) {
      console.error('❌ Failed to copy summary:', error);
      toast.error('Không sao chép được tóm tắt');
    }
  }, [aiSummary, meetingTitle, meeting, blockNoteSummaryRef]);

  return {
    handleCopyTranscript,
    handleCopySummary,
  };
}
