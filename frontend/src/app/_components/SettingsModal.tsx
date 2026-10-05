import { TranscriptSettings } from "@/components/TranscriptSettings";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { useConfig } from "@/contexts/ConfigContext";

type modalType = "modelSelector" | "errorAlert" | "chunkDropWarning";

/**
 * SettingsModals Component
 *
 * All settings modals consolidated into a single component.
 * Uses ConfigContext internally - no prop drilling needed!
 */

interface SettingsModalsProps {
  modals: {
    modelSelector: boolean;
    errorAlert: boolean;
    chunkDropWarning: boolean;
  };
  messages: {
    errorAlert: string;
    chunkDropWarning: string;
    modelSelector: string;
  };
  onClose: (name: modalType) => void;
}

export function SettingsModals({
  modals,
  messages,
  onClose,
}: SettingsModalsProps) {
  // Contexts
  const {
    transcriptModelConfig,
    setTranscriptModelConfig,
    showConfidenceIndicator,
    toggleConfidenceIndicator,
  } = useConfig();

  return <>
    {/* Model Selection Modal */}
    {modals.modelSelector && (
      <div className="fixed inset-0 app-modal-overlay flex items-center justify-center z-[var(--z-modal)]">
        <div className="app-surface max-w-4xl w-full mx-4 max-h-[90vh] flex flex-col">
          {/* Fixed Header */}
          <div className="flex justify-between items-center p-6 pb-4 border-b border-rule">
            <h3 className="text-lg font-semibold text-ink">
              {messages.modelSelector ? 'Cần cài đặt nhận dạng giọng nói' : 'Cài đặt nhận dạng tiếng Việt'}
            </h3>
            <button
              onClick={() => onClose('modelSelector')}
              className="text-ink-2 hover:text-ink"
            >
              <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          {/* Scrollable Content */}
          <div className="flex-1 overflow-y-auto p-6 pt-4">
            <TranscriptSettings
              transcriptModelConfig={transcriptModelConfig}
              setTranscriptModelConfig={setTranscriptModelConfig}
              onModelSelect={() => onClose('modelSelector')}
            />
          </div>

          {/* Fixed Footer */}
          <div className="p-6 pt-4 border-t border-rule flex items-center justify-between">
            {/* Confidence Indicator Toggle */}
            <div className="flex items-center gap-3">
              <label className="relative inline-flex items-center cursor-pointer">
                <input
                  type="checkbox"
                  checked={showConfidenceIndicator}
                  onChange={(e) => toggleConfidenceIndicator(e.target.checked)}
                  className="sr-only peer"
                />
                <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-2 peer-focus:ring-primary/30 rounded-full peer peer-checked:after:translate-x-full rtl:peer-checked:after:-translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary"></div>
              </label>
              <div>
                <p className="text-sm font-medium text-ink">Hiển thị chỉ số độ tin cậy</p>
                <p className="text-xs text-ink-2">Hiển thị dấu chấm màu thể hiện chất lượng nhận dạng</p>
              </div>
            </div>

            <button
              onClick={() => onClose('modelSelector')}
              className="px-4 py-2 text-sm font-medium text-ink bg-secondary rounded-md hover:bg-muted focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {messages.modelSelector ? 'Hủy' : 'Xong'}
            </button>
          </div>
        </div>
      </div>
    )}

    {/* Error Alert Modal */}
    {modals.errorAlert && (
      <div className="fixed inset-0 app-modal-overlay flex items-center justify-center z-[var(--z-modal)]">
        <Alert className="max-w-md mx-4 border-red-200 bg-paper-2">
          <AlertTitle className="text-red-800">Ghi âm đã dừng</AlertTitle>
          <AlertDescription className="text-red-700">
            {messages.errorAlert}
            <button
              onClick={() => onClose('errorAlert')}
              className="ml-2 text-red-600 hover:text-red-800 underline"
            >
              Đóng
            </button>
          </AlertDescription>
        </Alert>
      </div>
    )}

    {/* Chunk Drop Warning Modal */}
    {modals.chunkDropWarning && (
      <div className="fixed inset-0 app-modal-overlay flex items-center justify-center z-[var(--z-modal)]">
        <Alert className="max-w-lg mx-4 border-yellow-200 bg-paper-2">
          <AlertTitle className="text-yellow-800">Cảnh báo hiệu suất nhận dạng</AlertTitle>
          <AlertDescription className="text-yellow-700">
            {messages.chunkDropWarning}
            <button
              onClick={() => onClose('chunkDropWarning')}
              className="ml-2 text-yellow-600 hover:text-yellow-800 underline"
            >
              Đóng
            </button>
          </AlertDescription>
        </Alert>
      </div>
    )}
  </>
}
