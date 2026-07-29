"use client";

import { ModelConfig, ModelSettingsModal } from '@/components/ModelSettingsModal';
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogTitle,
} from "@/components/ui/dialog"
import { VisuallyHidden } from "@/components/ui/visually-hidden"
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Sparkles, Settings, Loader2, FileText, Check, Square, Paperclip } from 'lucide-react';
import Analytics from '@/lib/analytics';
import { useState, useEffect, useCallback } from 'react';
import { useMeetingDocuments } from '@/hooks/useMeetingDocuments';
import { MeetingDocumentsDialog } from './MeetingDocumentsDialog';
interface SummaryGeneratorButtonGroupProps {
  modelConfig: ModelConfig;
  setModelConfig: (config: ModelConfig | ((prev: ModelConfig) => ModelConfig)) => void;
  onSaveModelConfig: (config?: ModelConfig) => Promise<void>;
  onGenerateSummary: (customPrompt: string) => Promise<void>;
  onStopGeneration: () => void;
  customPrompt: string;
  summaryStatus: 'idle' | 'processing' | 'summarizing' | 'regenerating' | 'completed' | 'error';
  availableTemplates: Array<{ id: string, name: string, description: string }>;
  selectedTemplate: string;
  onTemplateSelect: (templateId: string, templateName: string) => void;
  hasTranscripts?: boolean;
  isModelConfigLoading?: boolean;
  onOpenModelSettings?: (openFn: () => void) => void;
  meetingId: string;
}

export function SummaryGeneratorButtonGroup({
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
  hasTranscripts = true,
  isModelConfigLoading = false,
  onOpenModelSettings,
  meetingId,
}: SummaryGeneratorButtonGroupProps) {
  const [settingsDialogOpen, setSettingsDialogOpen] = useState(false);
  const [documentsDialogOpen, setDocumentsDialogOpen] = useState(false);
  const { documents, refetch: refetchDocuments } = useMeetingDocuments();

  useEffect(() => {
    refetchDocuments(meetingId);
  }, [meetingId, refetchDocuments]);

  const handleDocumentsDialogChange = useCallback(
    (open: boolean) => {
      setDocumentsDialogOpen(open);
      if (!open) {
        refetchDocuments(meetingId);
      }
    },
    [meetingId, refetchDocuments]
  );

  // Expose the function to open the modal via callback registration
  useEffect(() => {
    if (onOpenModelSettings) {
      // Register our open dialog function with the parent by calling the callback
      // This allows the parent to store a reference to this function
      const openDialog = () => {
        console.log('📱 Opening model settings dialog via callback');
        setSettingsDialogOpen(true);
      };

      // Call the parent's callback with our open function
      // Note: This assumes onOpenModelSettings accepts a function parameter
      // We'll need to adjust the signature
      onOpenModelSettings(openDialog);
    }
  }, [onOpenModelSettings]);

  const documentsButton = (
    <>
      <Button
        variant="outline"
        size="sm"
        className="relative"
        onClick={() => setDocumentsDialogOpen(true)}
        title="Tài liệu tham khảo"
      >
        <Paperclip />
        <span className="hidden lg:inline">Tài liệu</span>
        {documents.length > 0 && (
          <div className="absolute -top-1.5 -right-1.5 flex h-4 w-4 items-center justify-center rounded-full bg-[#16478e] text-[10px] font-medium text-white">
            {documents.length}
          </div>
        )}
      </Button>
      <MeetingDocumentsDialog
        open={documentsDialogOpen}
        onOpenChange={handleDocumentsDialogChange}
        meetingId={meetingId}
      />
    </>
  );

  if (!hasTranscripts) {
    return <ButtonGroup>{documentsButton}</ButtonGroup>;
  }

  const isGenerating = summaryStatus === 'processing' || summaryStatus === 'summarizing' || summaryStatus === 'regenerating';

  return (
    <ButtonGroup>
      {/* Generate Summary or Stop button */}
      {isGenerating ? (
        <Button
          variant="outline"
          size="sm"
          className="bg-[rgba(230,48,39,0.08)] hover:bg-[rgba(230,48,39,0.15)] border-[rgba(230,48,39,0.3)] text-[#e63027] xl:px-4"
          onClick={() => {
            Analytics.trackButtonClick('stop_summary_generation', 'meeting_details');
            onStopGeneration();
          }}
          title="Dừng tạo tóm tắt"
        >
          <Square className="xl:mr-2" size={18} fill="currentColor" />
          <span className="hidden lg:inline xl:inline">Dừng</span>
        </Button>
      ) : (
        <Button
          variant="outline"
          size="sm"
          className="bg-[rgba(22,71,142,0.08)] hover:bg-[rgba(22,71,142,0.15)] border-[rgba(22,71,142,0.3)] text-[#16478e] xl:px-4"
          onClick={() => {
            Analytics.trackButtonClick('generate_summary', 'meeting_details');
            onGenerateSummary(customPrompt);
          }}
          disabled={isModelConfigLoading}
          title={
            isModelConfigLoading
              ? 'Đang tải cấu hình mô hình...'
              : 'Tạo báo cáo'
          }
        >
          {isModelConfigLoading ? (
            <>
              <Loader2 className="animate-spin xl:mr-2" size={18} />
              <span className="hidden xl:inline">Đang xử lý...</span>
            </>
          ) : (
            <>
              <Sparkles size={18} />
              <span className="hidden lg:inline xl:inline">Tạo tóm tắt</span>
            </>
          )}
        </Button>
      )}

      {/* Reference documents button */}
      {documentsButton}

      {/* Settings button */}
      <Dialog open={settingsDialogOpen} onOpenChange={setSettingsDialogOpen}>
        <DialogTrigger asChild>
          <Button
            variant="outline"
            size="sm"
            title="Cài đặt mô hình"
          >
            <Settings />
            <span className="hidden lg:inline">Mô hình AI</span>
          </Button>
        </DialogTrigger>
        <DialogContent
          aria-describedby={undefined}
        >
          <VisuallyHidden>
            <DialogTitle>Cài đặt mô hình AI</DialogTitle>
          </VisuallyHidden>
          <ModelSettingsModal
            onSave={async (config) => {
              await onSaveModelConfig(config);
              setSettingsDialogOpen(false);
            }}
            modelConfig={modelConfig}
            setModelConfig={setModelConfig}
            skipInitialFetch={true}
          />
        </DialogContent>
      </Dialog>

      {/* Template selector dropdown */}
      {availableTemplates.length > 0 && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="outline"
              size="sm"
              title="Chọn mẫu tóm tắt"
            >
              <FileText />
              <span className="hidden lg:inline">Mẫu</span>
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {availableTemplates.map((template) => (
              <DropdownMenuItem
                key={template.id}
                onClick={() => onTemplateSelect(template.id, template.name)}
                title={template.description}
                className="flex items-center justify-between gap-2"
              >
                <span>{template.name}</span>
                {selectedTemplate === template.id && (
                  <Check className="h-4 w-4 text-green-600" />
                )}
              </DropdownMenuItem>
            ))}

          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </ButtonGroup>
  );
}
