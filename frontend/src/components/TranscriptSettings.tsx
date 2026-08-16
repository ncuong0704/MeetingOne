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
    <div className="max-w-xl">
      <AsrPathTabs />
    </div>
  );
}
