import ZipFormerModelManager from './ZipFormerModelManager';

export interface TranscriptModelProps {
  provider: 'zipformer';
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
      <div className="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
        <div className="px-5 py-4 border-b border-gray-50">
          <h3 className="text-base font-semibold text-gray-900">Nhận dạng giọng nói tiếng Việt</h3>
          <p className="text-sm text-gray-500 mt-1">Quản lý mô hình chuyển đổi giọng nói sang văn bản.</p>
        </div>
        <div className="px-5 py-5">
          <ZipFormerModelManager />
        </div>
      </div>
    </div>
  );
}
