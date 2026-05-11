import { useCallback, RefObject } from 'react';
import { Transcript, Summary } from '@/types';
import { BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { copyRichText, markdownToWordHtml, wrapWordHtml, enrichBlockNoteHtml } from '@/lib/clipboardUtils';

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

  // Helper function to fetch ALL transcripts for copying (not just paginated data)
  const fetchAllTranscripts = useCallback(async (meetingId: string): Promise<Transcript[]> => {
    try {
      console.log('📊 Fetching all transcripts for copying:', meetingId);

      // First, get total count by fetching first page
      const firstPage = await invokeTauri('api_get_meeting_transcripts', {
        meetingId,
        limit: 1,
        offset: 0,
      }) as { transcripts: Transcript[]; total_count: number; has_more: boolean };

      const totalCount = firstPage.total_count;
      console.log(`📊 Total transcripts in database: ${totalCount}`);

      if (totalCount === 0) {
        return [];
      }

      // Fetch all transcripts in one call
      const allData = await invokeTauri('api_get_meeting_transcripts', {
        meetingId,
        limit: totalCount,
        offset: 0,
      }) as { transcripts: Transcript[]; total_count: number; has_more: boolean };

      console.log(`✅ Fetched ${allData.transcripts.length} transcripts from database for copying`);
      return allData.transcripts;
    } catch (error) {
      console.error('❌ Error fetching all transcripts:', error);
      toast.error('Không tải được bản ghi để sao chép');
      return [];
    }
  }, []);

  // Copy transcript to clipboard
  const handleCopyTranscript = useCallback(async () => {
    // CHANGE: Fetch ALL transcripts from database, not from pagination state
    console.log('📊 Fetching all transcripts for copying...');
    const allTranscripts = await fetchAllTranscripts(meeting.id);

    if (!allTranscripts.length) {
      const error_msg = 'Không có bản ghi để sao chép';
      console.log(error_msg);
      toast.error(error_msg);
      return;
    }

    console.log(`✅ Copying ${allTranscripts.length} transcripts to clipboard`);

    // Format timestamps as recording-relative [MM:SS] instead of wall-clock time
    const formatTime = (seconds: number | undefined, fallbackTimestamp: string): string => {
      if (seconds === undefined) {
        // For old transcripts without audio_start_time, use wall-clock time
        return fallbackTimestamp;
      }
      const totalSecs = Math.floor(seconds);
      const mins = Math.floor(totalSecs / 60);
      const secs = totalSecs % 60;
      return `[${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}]`;
    };

    const dateStr = new Date(meeting.created_at).toLocaleDateString('vi-VN');
    const titleStr = meetingTitle ?? meeting.title;

    // ── Plain text ───────────────────────────────────────────────────────────
    const plainText =
      `# Bản ghi cuộc họp: ${titleStr}\n\n` +
      `Ngày: ${dateStr}\n\n` +
      allTranscripts.map(t => `${formatTime(t.audio_start_time, t.timestamp)} ${t.text}`).join('\n');

    // ── Rich HTML (Word-compatible) ──────────────────────────────────────────
    const FONT = "Calibri,'Segoe UI',Arial,sans-serif";
    const rowsHtml = allTranscripts
      .map(t =>
        `<p style="font-family:${FONT};font-size:11pt;margin:3pt 0;">` +
        `<span style="font-family:'Courier New',monospace;font-size:9.5pt;color:#666;margin-right:6pt;">${formatTime(t.audio_start_time, t.timestamp)}</span>` +
        `${t.text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')}` +
        `</p>`
      ).join('\n');

    const bodyHtml =
      `<h1 style="font-family:${FONT};font-size:18pt;font-weight:bold;color:#111;margin:0 0 6pt;">Bản ghi cuộc họp: ${titleStr.replace(/&/g, '&amp;')}</h1>` +
      `<p style="font-family:${FONT};font-size:10pt;color:#666;margin:0 0 10pt;">Ngày: ${dateStr}</p>` +
      `<hr style="border:none;border-top:1pt solid #ccc;margin:8pt 0;">` +
      rowsHtml;
    const richHtml = wrapWordHtml(bodyHtml);

    await copyRichText(richHtml, plainText);
    toast.success('Đã sao chép bản ghi vào bảng nhớ tạm');

    // Track copy analytics
    const wordCount = allTranscripts
      .map(t => t.text.split(/\s+/).length)
      .reduce((a, b) => a + b, 0);

    await Analytics.trackCopy('transcript', {
      meeting_id: meeting.id,
      transcript_length: allTranscripts.length.toString(),
      word_count: wordCount.toString()
    });
  }, [meeting, meetingTitle, fetchAllTranscripts]);

  // Copy summary to clipboard
  const handleCopySummary = useCallback(async () => {
    try {
      let summaryMarkdown = '';

      console.log('🔍 Copy Summary - Starting...');

      // Try to get markdown from BlockNote editor first
      if (blockNoteSummaryRef.current?.getMarkdown) {
        console.log('📝 Trying to get markdown from ref...');
        summaryMarkdown = await blockNoteSummaryRef.current.getMarkdown();
        console.log('📝 Got markdown from ref, length:', summaryMarkdown.length);
      }

      // Fallback: Check if aiSummary has markdown property
      if (!summaryMarkdown && aiSummary && 'markdown' in aiSummary) {
        console.log('📝 Using markdown from aiSummary');
        summaryMarkdown = (aiSummary as any).markdown || '';
        console.log('📝 Markdown from aiSummary, length:', summaryMarkdown.length);
      }

      // Fallback: Check for legacy format
      if (!summaryMarkdown && aiSummary) {
        console.log('📝 Converting legacy format to markdown');
        const sections = Object.entries(aiSummary)
          .filter(([key]) => {
            // Skip non-section keys
            return key !== 'markdown' && key !== 'summary_json' && key !== '_section_order' && key !== 'MeetingName';
          })
          .map(([, section]) => {
            if (section && typeof section === 'object' && 'title' in section && 'blocks' in section) {
              const sectionTitle = `## ${section.title}\n\n`;
              const sectionContent = section.blocks
                .map((block: any) => `- ${block.content}`)
                .join('\n');
              return sectionTitle + sectionContent;
            }
            return '';
          })
          .filter(s => s.trim())
          .join('\n\n');
        summaryMarkdown = sections;
        console.log('📝 Converted legacy format, length:', summaryMarkdown.length);
      }

      // If still no summary content, show message
      if (!summaryMarkdown.trim()) {
        console.error('❌ No summary content available to copy');
        toast.error('Không có nội dung tóm tắt để sao chép');
        return;
      }

      const dateStr = new Date(meeting.created_at).toLocaleDateString('vi-VN', {
        year: 'numeric', month: 'long', day: 'numeric', hour: '2-digit', minute: '2-digit',
      });

      // ── Plain text ─────────────────────────────────────────────────────────
      const plainText =
        `# Tóm tắt cuộc họp: ${meetingTitle}\n\n` +
        `Ngày họp: ${dateStr}\n\n---\n\n` +
        summaryMarkdown;

      // ── Rich HTML: try BlockNote native HTML first ─────────────────────────
      let blockNoteHtml = '';
      if (blockNoteSummaryRef.current?.getHTML) {
        try {
          blockNoteHtml = await blockNoteSummaryRef.current.getHTML();
        } catch (_) {}
      }

      let richHtml: string;
      if (blockNoteHtml.trim()) {
        // Use BlockNote's own HTML output (most accurate formatting)
        richHtml = enrichBlockNoteHtml(blockNoteHtml, `Tóm tắt cuộc họp: ${meetingTitle}`, `Ngày họp: ${dateStr}`);
      } else {
        // Fallback: convert markdown to Word-compatible HTML
        const bodyHtml =
          `<h1 style="font-family:Calibri,sans-serif;font-size:18pt;font-weight:bold;color:#111;margin:0 0 6pt;">` +
          `Tóm tắt cuộc họp: ${meetingTitle}</h1>` +
          `<p style="font-family:Calibri,sans-serif;font-size:10pt;color:#666;margin:0 0 10pt;">Ngày họp: ${dateStr}</p>` +
          `<hr style="border:none;border-top:1pt solid #ccc;margin:8pt 0;">` +
          markdownToWordHtml(summaryMarkdown);
        richHtml = wrapWordHtml(bodyHtml);
      }

      await copyRichText(richHtml, plainText);

      console.log('✅ Successfully copied to clipboard!');
      toast.success('Đã sao chép tóm tắt vào bảng nhớ tạm');

      // Track copy analytics
      await Analytics.trackCopy('summary', {
        meeting_id: meeting.id,
        has_markdown: (!!aiSummary && 'markdown' in aiSummary).toString()
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
