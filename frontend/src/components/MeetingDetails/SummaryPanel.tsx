"use client";

import { Summary, SummaryResponse, Transcript } from '@/types';
import { BlockNoteSummaryView, BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import { EmptyStateSummary } from '@/components/EmptyStateSummary';
import { ModelConfig } from '@/components/ModelSettingsModal';
import { SummaryGeneratorButtonGroup } from './SummaryGeneratorButtonGroup';
import { SummaryUpdaterButtonGroup } from './SummaryUpdaterButtonGroup';
import Analytics from '@/lib/analytics';
import { RefObject } from 'react';

interface SummaryPanelProps {
  meeting: {
    id: string;
    title: string;
    created_at: string;
  };
  meetingTitle: string;
  onTitleChange: (title: string) => void;
  isEditingTitle: boolean;
  onStartEditTitle: () => void;
  onFinishEditTitle: () => void;
  isTitleDirty: boolean;
  summaryRef: RefObject<BlockNoteSummaryViewRef>;
  isSaving: boolean;
  onSaveAll: () => Promise<void>;
  onCopySummary: () => Promise<void>;
  onOpenFolder: () => Promise<void>;
  onExportDocx: () => Promise<void>;
  aiSummary: Summary | null;
  summaryStatus: 'idle' | 'processing' | 'summarizing' | 'regenerating' | 'completed' | 'error';
  transcripts: Transcript[];
  modelConfig: ModelConfig;
  setModelConfig: (config: ModelConfig | ((prev: ModelConfig) => ModelConfig)) => void;
  onSaveModelConfig: (config?: ModelConfig) => Promise<void>;
  onGenerateSummary: (customPrompt: string) => Promise<void>;
  onStopGeneration: () => void;
  customPrompt: string;
  summaryResponse: SummaryResponse | null;
  onSaveSummary: (summary: Summary | { markdown?: string; summary_json?: any[] }) => Promise<void>;
  onSummaryChange: (summary: Summary) => void;
  onDirtyChange: (isDirty: boolean) => void;
  summaryError: string | null;
  onRegenerateSummary: () => Promise<void>;
  getSummaryStatusMessage: (status: 'idle' | 'processing' | 'summarizing' | 'regenerating' | 'completed' | 'error') => string;
  availableTemplates: Array<{ id: string, name: string, description: string }>;
  selectedTemplate: string;
  onTemplateSelect: (templateId: string, templateName: string) => void;
  isModelConfigLoading?: boolean;
  onOpenModelSettings?: (openFn: () => void) => void;
}

function SummarySkeleton() {
  return (
    <div className="flex-1 p-6 space-y-6 animate-pulse">
      <div className="space-y-2">
        <div className="h-4 bg-gray-100 rounded-full w-1/4" />
        <div className="h-3 bg-gray-100 rounded-full" />
        <div className="h-3 bg-gray-100 rounded-full w-5/6" />
        <div className="h-3 bg-gray-100 rounded-full w-4/6" />
      </div>
      <div className="space-y-2">
        <div className="h-4 bg-gray-100 rounded-full w-1/3" />
        <div className="h-3 bg-gray-100 rounded-full w-3/4" />
        <div className="h-3 bg-gray-100 rounded-full w-5/6" />
        <div className="h-3 bg-gray-100 rounded-full w-2/3" />
      </div>
      <div className="space-y-2">
        <div className="h-4 bg-gray-100 rounded-full w-1/4" />
        <div className="h-3 bg-gray-100 rounded-full w-4/5" />
        <div className="h-3 bg-gray-100 rounded-full w-3/5" />
      </div>
      <div className="space-y-2">
        <div className="h-4 bg-gray-100 rounded-full w-2/5" />
        <div className="h-3 bg-gray-100 rounded-full w-full" />
        <div className="h-3 bg-gray-100 rounded-full w-5/6" />
        <div className="h-3 bg-gray-100 rounded-full w-3/4" />
      </div>
    </div>
  );
}

export function SummaryPanel({
  meeting,
  meetingTitle,
  onTitleChange,
  isEditingTitle,
  onStartEditTitle,
  onFinishEditTitle,
  isTitleDirty,
  summaryRef,
  isSaving,
  onSaveAll,
  onCopySummary,
  onOpenFolder,
  onExportDocx,
  aiSummary,
  summaryStatus,
  transcripts,
  modelConfig,
  setModelConfig,
  onSaveModelConfig,
  onGenerateSummary,
  onStopGeneration,
  customPrompt,
  summaryResponse,
  onSaveSummary,
  onSummaryChange,
  onDirtyChange,
  summaryError,
  onRegenerateSummary,
  getSummaryStatusMessage,
  availableTemplates,
  selectedTemplate,
  onTemplateSelect,
  isModelConfigLoading = false,
  onOpenModelSettings
}: SummaryPanelProps) {
  const isSummaryLoading = summaryStatus === 'processing' || summaryStatus === 'summarizing' || summaryStatus === 'regenerating';

  const statusBadge = summaryStatus === 'error'
    ? <span className="inline-flex items-center rounded-full bg-[rgba(230,48,39,0.08)] border border-[rgba(230,48,39,0.2)] px-2 py-0.5 text-xs font-medium text-[#e63027]">Lỗi</span>
    : summaryStatus === 'completed'
    ? <span className="inline-flex items-center rounded-full bg-green-50 border border-green-100 px-2 py-0.5 text-xs font-medium text-green-600">Đã tạo</span>
    : isSummaryLoading
    ? <span className="inline-flex items-center gap-1 rounded-full bg-[rgba(22,71,142,0.10)] border border-[rgba(22,71,142,0.2)] px-2 py-0.5 text-xs font-medium text-[#16478e]">
        <span className="h-1.5 w-1.5 rounded-full bg-[#16478e] animate-pulse" />
        Đang tạo…
      </span>
    : null;

  const sharedGeneratorProps = {
    modelConfig,
    setModelConfig,
    onSaveModelConfig,
    onGenerateSummary,
    onStopGeneration,
    customPrompt,
    summaryStatus,
    availableTemplates,
    selectedTemplate,
    onTemplateSelect,
    hasTranscripts: transcripts.length > 0,
    isModelConfigLoading,
    onOpenModelSettings,
  };

  return (
    <div className="flex h-full min-h-0 w-full min-w-0 flex-1 flex-col overflow-hidden bg-white">
      {/* Unified header — [&_span]:hidden forces icon-only buttons so they fit in narrow split panels */}
      <div className="flex shrink-0 items-center justify-between gap-2 border-b border-gray-100 px-4 py-3 min-w-0">
        <div className="flex items-center gap-2 shrink-0 min-w-0">
          <span className="text-sm font-semibold text-gray-800 whitespace-nowrap">Báo cáo AI</span>
          <span className="hidden sm:block">{statusBadge}</span>
        </div>
        {/* [&_span]:hidden collapses all button text to icon-only — tooltips (title=) remain for discoverability */}
        <div className="flex items-center gap-1.5 shrink-0 [&_span]:hidden">
          <SummaryGeneratorButtonGroup {...sharedGeneratorProps} />
          {aiSummary && !isSummaryLoading && (
            <SummaryUpdaterButtonGroup
              isSaving={isSaving}
              isDirty={isTitleDirty || (summaryRef.current?.isDirty || false)}
              onSave={onSaveAll}
              onCopy={onCopySummary}
              onFind={() => {}}
              onOpenFolder={onOpenFolder}
              onExportDocx={onExportDocx}
              hasSummary={!!aiSummary}
            />
          )}
        </div>
      </div>

      {/* Content area */}
      {isSummaryLoading ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <SummarySkeleton />
        </div>
      ) : !aiSummary ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <EmptyStateSummary
            onGenerate={() => onGenerateSummary(customPrompt)}
            hasModel={modelConfig.provider !== null && modelConfig.model !== null}
            isGenerating={false}
          />
        </div>
      ) : transcripts?.length > 0 ? (
        <div className="flex-1 overflow-y-auto min-h-0">
          <div className="p-6 w-full">
            <BlockNoteSummaryView
              ref={summaryRef}
              summaryData={aiSummary}
              onSave={onSaveSummary}
              onSummaryChange={onSummaryChange}
              onDirtyChange={onDirtyChange}
              status={summaryStatus}
              error={summaryError}
              onRegenerateSummary={() => {
                Analytics.trackButtonClick('regenerate_summary', 'meeting_details');
                onRegenerateSummary();
              }}
              meeting={{
                id: meeting.id,
                title: meetingTitle,
                created_at: meeting.created_at
              }}
            />
          </div>
          {summaryStatus === 'error' && (
            <div className="mx-6 mb-6 p-3 rounded-lg bg-red-50 border border-red-100 text-red-700">
              <p className="text-sm font-medium">{getSummaryStatusMessage(summaryStatus)}</p>
            </div>
          )}
        </div>
      ) : null}
    </div>
  );
}
