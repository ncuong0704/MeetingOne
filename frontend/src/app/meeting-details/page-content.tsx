"use client";
import { useState, useEffect, useRef } from 'react';
import { motion } from 'framer-motion';
import { Summary, SummaryResponse } from '@/types';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import Analytics from '@/lib/analytics';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { TranscriptPanel } from '@/components/MeetingDetails/TranscriptPanel';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { ModelConfig } from '@/components/ModelSettingsModal';
import { PanelLeft, Columns2, PanelRight } from 'lucide-react';

// Custom hooks
import { useMeetingData } from '@/hooks/meeting-details/useMeetingData';
import { useSummaryGeneration } from '@/hooks/meeting-details/useSummaryGeneration';
import { useTemplates } from '@/hooks/meeting-details/useTemplates';
import { useCopyOperations } from '@/hooks/meeting-details/useCopyOperations';
import { useMeetingOperations } from '@/hooks/meeting-details/useMeetingOperations';
import { useConfig } from '@/contexts/ConfigContext';

export default function PageContent({
  meeting,
  summaryData,
  shouldAutoGenerate = false,
  onAutoGenerateComplete,
  onMeetingUpdated,
  onRefetchTranscripts,
  // Pagination props for efficient transcript loading
  segments,
  hasMore,
  isLoadingMore,
  totalCount,
  loadedCount,
  onLoadMore,
}: {
  meeting: any;
  summaryData: Summary | null;
  shouldAutoGenerate?: boolean;
  onAutoGenerateComplete?: () => void;
  onMeetingUpdated?: () => Promise<void>;
  onRefetchTranscripts?: () => Promise<void>;
  // Pagination props
  segments?: any[];
  hasMore?: boolean;
  isLoadingMore?: boolean;
  totalCount?: number;
  loadedCount?: number;
  onLoadMore?: () => void;
}) {
  console.log('📄 PAGE CONTENT: Initializing with data:', {
    meetingId: meeting.id,
    summaryDataKeys: summaryData ? Object.keys(summaryData) : null,
    transcriptsCount: meeting.transcripts?.length
  });

  // State
  const [customPrompt, setCustomPrompt] = useState<string>('');
  const [isRecording] = useState(false);
  const [summaryResponse] = useState<SummaryResponse | null>(null);

  // Split view UI state (persisted)
  const [layoutMode, setLayoutMode] = useState<'split' | 'transcript' | 'summary'>('split');
  const [leftPct, setLeftPct] = useState<number>(40); // transcript width in split mode (%)
  const isDraggingRef = useRef(false);

  // Ref to store the modal open function from SummaryGeneratorButtonGroup
  const openModelSettingsRef = useRef<(() => void) | null>(null);

  // Sidebar context
  const { serverAddress } = useSidebar();

  // Get model config from ConfigContext
  const { modelConfig, setModelConfig } = useConfig();

  // Custom hooks
  const meetingData = useMeetingData({ meeting, summaryData, onMeetingUpdated });
  const templates = useTemplates();

  // Callback to register the modal open function
  const handleRegisterModalOpen = (openFn: () => void) => {
    console.log('📝 Registering modal open function in PageContent');
    openModelSettingsRef.current = openFn;
  };

  // Callback to trigger modal open (called from error handler)
  const handleOpenModelSettings = () => {
    console.log('🔔 Opening model settings from PageContent');
    if (openModelSettingsRef.current) {
      openModelSettingsRef.current();
    } else {
      console.warn('⚠️ Modal open function not yet registered');
    }
  };

  // Save model config to backend database and sync via event
  const handleSaveModelConfig = async (config?: ModelConfig) => {
    if (!config) return;
    try {
      await invoke('api_save_model_config', {
        provider: config.provider,
        model: config.model,
        whisperModel: config.whisperModel,
        apiKey: config.apiKey ?? null,
        ollamaEndpoint: config.ollamaEndpoint ?? null,
      });

      // Emit event so ConfigContext and other listeners stay in sync
      const { emit } = await import('@tauri-apps/api/event');
      await emit('model-config-updated', config);

      toast.success('Đã lưu cài đặt mô hình');
    } catch (error) {
      console.error('Failed to save model config:', error);
      toast.error('Không lưu được cài đặt mô hình');
    }
  };

  const handleExportSummaryDocx = async () => {
    try {
      if (!meetingData.blockNoteSummaryRef.current?.exportToDocxBytes) {
        toast.error('Không có nội dung tóm tắt để xuất');
        return;
      }

      const bytes = await meetingData.blockNoteSummaryRef.current.exportToDocxBytes();

      const { uint8ToBase64 } = await import('@/lib/exportUtils');
      const base64 = uint8ToBase64(bytes);

      const result = await invoke<{ status: string; message: string; path?: string }>(
        'api_save_export_file',
        { meetingTitle: meetingData.meetingTitle, extension: 'docx', fileDataBase64: base64 }
      );

      if (result.status === 'cancelled') {
        toast.info('Đã hủy lưu file DOCX');
      } else {
        toast.success('Xuất DOCX thành công', {
          description: result.path ? `Đã lưu tại: ${result.path}` : undefined,
        });
      }
    } catch (error) {
      console.error('Failed to export summary DOCX:', error);
      toast.error('Xuất DOCX thất bại', { description: String(error) });
    }
  };

  const handleExportSummaryPdf = async () => {
    try {
      if (!meetingData.blockNoteSummaryRef.current?.exportToPdfBytes) {
        toast.error('Không có nội dung tóm tắt để xuất');
        return;
      }

      const bytes = await meetingData.blockNoteSummaryRef.current.exportToPdfBytes();

      const { uint8ToBase64 } = await import('@/lib/exportUtils');
      const base64 = uint8ToBase64(bytes);

      const result = await invoke<{ status: string; message: string; path?: string }>(
        'api_save_export_file',
        { meetingTitle: meetingData.meetingTitle, extension: 'pdf', fileDataBase64: base64 }
      );

      if (result.status === 'cancelled') {
        toast.info('Đã hủy lưu file PDF');
      } else {
        toast.success('Xuất PDF thành công', {
          description: result.path ? `Đã lưu tại: ${result.path}` : undefined,
        });
      }
    } catch (error) {
      console.error('Failed to export summary PDF:', error);
      toast.error('Xuất PDF thất bại', { description: String(error) });
    }
  };

  const summaryGeneration = useSummaryGeneration({
    meeting,
    transcripts: meetingData.transcripts,
    modelConfig: modelConfig,
    isModelConfigLoading: false, // ConfigContext loads on mount
    selectedTemplate: templates.selectedTemplate,
    onMeetingUpdated,
    updateMeetingTitle: meetingData.updateMeetingTitle,
    setAiSummary: meetingData.setAiSummary,
    onOpenModelSettings: handleOpenModelSettings,
  });

  const copyOperations = useCopyOperations({
    meeting,
    transcripts: meetingData.transcripts,
    meetingTitle: meetingData.meetingTitle,
    aiSummary: meetingData.aiSummary,
    blockNoteSummaryRef: meetingData.blockNoteSummaryRef,
  });

  const meetingOperations = useMeetingOperations({
    meeting,
  });

  // Track page view
  useEffect(() => {
    Analytics.trackPageView('meeting_details');
  }, []);

  // Restore persisted layout settings
  useEffect(() => {
    try {
      const savedMode = localStorage.getItem('meetingDetailsLayoutMode');
      if (savedMode === 'split' || savedMode === 'transcript' || savedMode === 'summary') {
        setLayoutMode(savedMode);
      }
      const savedPct = Number(localStorage.getItem('meetingDetailsSplitLeftPct'));
      if (Number.isFinite(savedPct) && savedPct >= 25 && savedPct <= 75) {
        setLeftPct(savedPct);
      }
    } catch {
      // ignore (e.g. sandboxed env)
    }
  }, []);

  // Persist layout settings
  useEffect(() => {
    try {
      localStorage.setItem('meetingDetailsLayoutMode', layoutMode);
      localStorage.setItem('meetingDetailsSplitLeftPct', String(leftPct));
    } catch {
      // ignore
    }
  }, [layoutMode, leftPct]);

  // Drag handlers for resizable split
  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!isDraggingRef.current) return;
      const container = document.getElementById('meeting-details-split-container');
      if (!container) return;
      const rect = container.getBoundingClientRect();
      const x = Math.min(Math.max(e.clientX - rect.left, 0), rect.width);
      const pct = (x / rect.width) * 100;
      const clamped = Math.min(75, Math.max(25, pct));
      setLeftPct(clamped);
    };
    const onMouseUp = () => {
      isDraggingRef.current = false;
    };
    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
    return () => {
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
    };
  }, []);

  // Auto-generate summary when flag is set
  useEffect(() => {
    let cancelled = false;

    const autoGenerate = async () => {
      if (shouldAutoGenerate && meetingData.transcripts.length > 0 && !cancelled) {
        console.log(`🤖 Auto-generating summary with ${modelConfig.provider}/${modelConfig.model}...`);
        await summaryGeneration.handleGenerateSummary('');

        // Notify parent that auto-generation is complete (only if not cancelled)
        if (onAutoGenerateComplete && !cancelled) {
          onAutoGenerateComplete();
        }
      }
    };

    autoGenerate();

    // Cleanup: cancel if component unmounts or meeting changes
    return () => {
      cancelled = true;
    };
  }, [shouldAutoGenerate, meeting.id, meetingData.transcripts.length]); // Re-run when transcripts arrive (was missing: shouldAutoGenerate could flip true before transcripts synced)

  return (
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3, ease: 'easeOut' }}
      className="flex min-h-0 flex-col h-screen bg-gray-50"
    >
      <div className="flex justify-center px-3 pt-3">
        <div className="inline-flex items-center rounded-full border border-gray-200 bg-white p-0.5 shadow-sm">
          {(
            [
              { mode: 'transcript', icon: PanelLeft,  label: 'Bản ghi',  title: 'Chỉ hiện bản ghi' },
              { mode: 'split',      icon: Columns2,   label: 'Chia đôi', title: 'Hiện cả hai' },
              { mode: 'summary',    icon: PanelRight, label: 'Báo cáo',  title: 'Chỉ hiện báo cáo' },
            ] as const
          ).map(({ mode, icon: Icon, label, title }) => (
            <button
              key={mode}
              type="button"
              onClick={() => setLayoutMode(mode)}
              title={title}
              className={`inline-flex items-center gap-1.5 rounded-full px-4 py-1.5 text-sm font-medium transition-all duration-150 ${
                layoutMode === mode
                  ? 'text-[#16478e] bg-[rgba(22,71,142,0.08)]'
                  : 'text-gray-500 hover:text-[#16478e] hover:bg-[rgba(22,71,142,0.08)]'
              }`}
            >
              <Icon className="h-3.5 w-3.5" />
              <span className="hidden sm:inline">{label}</span>
            </button>
          ))}
        </div>
      </div>

      <div
        id="meeting-details-split-container"
        className="flex min-h-0 flex-1 overflow-hidden p-3 gap-3"
      >
        {(layoutMode === 'split' || layoutMode === 'transcript') && (
          <div
            className="flex min-h-0 h-full min-w-0 flex-col overflow-hidden rounded-xl border border-gray-200 bg-white"
            style={
              layoutMode === 'split'
                ? { flexBasis: `${leftPct}%`, flexGrow: 0, flexShrink: 0 }
                : { flexBasis: '100%', flexGrow: 1 }
            }
          >
            <TranscriptPanel
              transcripts={meetingData.transcripts}
              onCopyTranscript={copyOperations.handleCopyTranscript}
              onOpenMeetingFolder={meetingOperations.handleOpenMeetingFolder}
              isRecording={isRecording}
              disableAutoScroll={true}
              // Pagination props for efficient loading
              usePagination={true}
              segments={segments}
              hasMore={hasMore}
              isLoadingMore={isLoadingMore}
              totalCount={totalCount}
              loadedCount={loadedCount}
              onLoadMore={onLoadMore}
              // Retranscription props
              meetingId={meeting.id}
              meetingFolderPath={meeting.folder_path}
              onRefetchTranscripts={onRefetchTranscripts}
            />
          </div>
        )}

        {layoutMode === 'split' && (
          <div
            className="w-2 flex items-stretch justify-center"
            aria-hidden="true"
          >
            <div
              onMouseDown={() => { isDraggingRef.current = true; }}
              className="w-1.5 rounded-full bg-gray-200 hover:bg-gray-300 cursor-col-resize transition-colors"
              title="Kéo để thay đổi độ rộng"
            />
          </div>
        )}

        {(layoutMode === 'split' || layoutMode === 'summary') && (
          <div
            className="flex min-h-0 h-full min-w-0 flex-col overflow-hidden rounded-xl border border-gray-200 bg-white"
            style={
              layoutMode === 'split'
                ? { flexBasis: `${100 - leftPct}%`, flexGrow: 1, flexShrink: 1, width: `300px` }
                : { flexBasis: '100%', flexGrow: 1 }
            }
          >
            <SummaryPanel
              meeting={meeting}
              meetingTitle={meetingData.meetingTitle}
              onTitleChange={meetingData.handleTitleChange}
              isEditingTitle={meetingData.isEditingTitle}
              onStartEditTitle={() => meetingData.setIsEditingTitle(true)}
              onFinishEditTitle={() => meetingData.setIsEditingTitle(false)}
              isTitleDirty={meetingData.isTitleDirty}
              summaryRef={meetingData.blockNoteSummaryRef}
              isSaving={meetingData.isSaving}
              onSaveAll={meetingData.saveAllChanges}
              onOpenFolder={meetingOperations.handleOpenMeetingFolder}
              onExportDocx={handleExportSummaryDocx}
              onExportPdf={handleExportSummaryPdf}
              aiSummary={meetingData.aiSummary}
              summaryStatus={summaryGeneration.summaryStatus}
              transcripts={meetingData.transcripts}
              modelConfig={modelConfig}
              setModelConfig={setModelConfig}
              onSaveModelConfig={handleSaveModelConfig}
              onGenerateSummary={summaryGeneration.handleGenerateSummary}
              onStopGeneration={summaryGeneration.handleStopGeneration}
              customPrompt={customPrompt}
              summaryResponse={summaryResponse}
              onSaveSummary={meetingData.handleSaveSummary}
              onSummaryChange={meetingData.handleSummaryChange}
              onDirtyChange={meetingData.setIsSummaryDirty}
              summaryError={summaryGeneration.summaryError}
              onRegenerateSummary={summaryGeneration.handleRegenerateSummary}
              getSummaryStatusMessage={summaryGeneration.getSummaryStatusMessage}
              availableTemplates={templates.availableTemplates}
              selectedTemplate={templates.selectedTemplate}
              onTemplateSelect={templates.handleTemplateSelection}
              isModelConfigLoading={false}
              onOpenModelSettings={handleRegisterModalOpen}
            />
          </div>
        )}
      </div>
    </motion.div>
  );
}
