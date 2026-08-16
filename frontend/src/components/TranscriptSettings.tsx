import AsrPathTabs from './AsrPathTabs';

export interface TranscriptModelProps {
  provider: 'asr';
  model: string;
  apiKey?: string | null;
}

export interface TranscriptSettingsProps {
  transcriptModelConfig: TranscriptModelProps;
  setTranscriptModelConfig: (config: TranscriptModelProps) => void;
  onModelSelect?: () => void;
}

export function TranscriptSettings({}: TranscriptSettingsProps) {
  return (
    <div className="space-y-4">
      <div className="app-surface overflow-hidden">
        <div className="px-5 py-4 border-b border-rule">
          <h3 className="text-base font-semibold text-ink">Nhận dạng giọng nói tiếng Việt</h3>
          <p className="text-sm text-ink-2 mt-1">Quản lý mô hình chuyển đổi giọng nói sang văn bản.</p>
        </div>
        <div className="px-5 py-5">
          <AsrPathTabs />
        </div>
      </div>
    </div>
  );
}
