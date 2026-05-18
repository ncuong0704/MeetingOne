import React, { useEffect, useState, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  Mic, Sparkles, Check, Loader2, AlertCircle,
  Download, ArrowRight, RefreshCw,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { toast } from 'sonner';
import { motion } from 'framer-motion';
import { cn } from '@/lib/utils';
import {
  ModelSettingsModal,
  type ModelSettingsModalRef,
} from '@/components/ModelSettingsModal';

type ZipStatus = 'waiting' | 'downloading' | 'completed' | 'error';
interface ZipState { status: ZipStatus; progress: number; error?: string; }

function StatusDot({ status }: { status: 'idle' | 'loading' | 'ok' | 'warn' | 'error' }) {
  if (status === 'loading') return <Loader2 className="w-4 h-4 text-gray-400 animate-spin" />;
  if (status === 'ok') return (
    <span className="flex h-6 w-6 items-center justify-center rounded-full bg-emerald-100">
      <Check className="w-3.5 h-3.5 text-emerald-600" strokeWidth={2.5} />
    </span>
  );
  if (status === 'warn') return (
    <span className="flex h-6 w-6 items-center justify-center rounded-full bg-amber-100">
      <AlertCircle className="w-3.5 h-3.5 text-amber-600" />
    </span>
  );
  if (status === 'error') return (
    <span className="flex h-6 w-6 items-center justify-center rounded-full bg-red-100">
      <AlertCircle className="w-3.5 h-3.5 text-red-600" />
    </span>
  );
  return <span className="h-2 w-2 rounded-full bg-gray-300" />;
}

function ProgressBar({ value, color = 'bg-gray-800' }: { value: number; color?: string }) {
  return (
    <motion.div className="w-full h-1.5 rounded-full bg-gray-100 overflow-hidden">
      <motion.div
        className={cn('h-full rounded-full', color)}
        initial={{ width: 0 }}
        animate={{ width: `${value}%` }}
        transition={{ duration: 0.4, ease: 'easeOut' }}
      />
    </motion.div>
  );
}

export function DownloadProgressStep() {
  const {
    goNext,
    setParakeetDownloaded,
    parakeetDownloaded,
    startBackgroundDownloads,
    completeOnboarding,
    summaryModelConfig,
    setSummaryModelConfig,
    saveSummaryModelConfig,
  } = useOnboarding();

  const [isMac, setIsMac] = useState(false);
  const [zipState, setZipState] = useState<ZipState>({
    status: parakeetDownloaded ? 'completed' : 'waiting',
    progress: parakeetDownloaded ? 100 : 0,
  });
  const [isCompleting, setIsCompleting] = useState(false);
  const downloadStartedRef = useRef(false);
  const retryingRef = useRef(false);
  const modelSettingsRef = useRef<ModelSettingsModalRef>(null);

  useEffect(() => {
    const checkPlatform = async () => {
      try {
        const { platform } = await import('@tauri-apps/plugin-os');
        setIsMac(platform() === 'macos');
      } catch {
        setIsMac(navigator.userAgent.includes('Mac'));
      }
    };

    const checkModelPresence = async () => {
      if (parakeetDownloaded) return;
      try {
        await invoke('zipformer_validate_model_ready');
        setZipState({ status: 'completed', progress: 100 });
        setParakeetDownloaded(true);
      } catch {
        // Model files not present
      }
    };

    checkPlatform();
    checkModelPresence();
  }, []);

  const handleStartZipDownload = async () => {
    if (downloadStartedRef.current) return;
    downloadStartedRef.current = true;
    setZipState({ status: 'downloading', progress: 0 });
    startBackgroundDownloads(false).catch(err => {
      setZipState({ status: 'error', progress: 0, error: String(err) });
      downloadStartedRef.current = false;
    });
  };

  useEffect(() => {
    const unP = listen<{ progress: number }>('zipformer-model-download-progress', (e) => {
      const p = e.payload.progress;
      setZipState(prev => ({ ...prev, status: p >= 100 ? 'completed' : 'downloading', progress: p }));
      if (p >= 100) setParakeetDownloaded(true);
    });
    const unC = listen('zipformer-model-download-complete', () => {
      setZipState({ status: 'completed', progress: 100 });
      setParakeetDownloaded(true);
      invoke('zipformer_load_model').catch(console.error);
    });
    const unE = listen<{ error: string }>('zipformer-model-download-error', (e) =>
      setZipState(prev => ({ ...prev, status: 'error', error: e.payload.error }))
    );
    return () => { unP.then(f => f()); unC.then(f => f()); unE.then(f => f()); };
  }, [setParakeetDownloaded]);

  const handleRetryZip = async () => {
    if (retryingRef.current) return;
    retryingRef.current = true;
    setZipState({ status: 'downloading', progress: 0 });
    try {
      await invoke('zipformer_download_model');
    } catch (err) {
      setZipState({ status: 'error', progress: 0, error: String(err) });
      toast.error('Thử lại thất bại');
    } finally {
      setTimeout(() => { retryingRef.current = false; }, 2000);
    }
  };

  const handleContinue = async () => {
    try {
      await invoke('zipformer_init');
      const ok = await invoke<boolean>('zipformer_is_model_loaded');
      if (ok && !parakeetDownloaded) {
        setParakeetDownloaded(true);
        setZipState({ status: 'completed', progress: 100 });
      }
    } catch {
      // ignore
    }

    const latestConfig = modelSettingsRef.current?.getConfig() ?? summaryModelConfig;
    setSummaryModelConfig(latestConfig);

    if (!parakeetDownloaded) {
      toast.message('Bạn có thể tải model sau', {
        description: 'Chưa có bộ nhận dạng giọng nói. Bạn vẫn có thể tiếp tục và tải sau trong Cài đặt.',
      });
    }

    const cloudProviders = ['claude', 'openai', 'groq', 'openrouter', 'custom-openai'];
    if (cloudProviders.includes(latestConfig.provider) && !latestConfig.apiKey?.trim()) {
      toast.message('Bạn có thể cấu hình API key sau', {
        description: 'Tóm tắt AI sẽ cần API key. Bạn có thể thêm sau trong Cài đặt > Tóm tắt AI.',
      });
    } else if (latestConfig.provider === 'ollama' && !latestConfig.model) {
      toast.message('Bạn có thể tải model tóm tắt sau', {
        description: 'Tóm tắt AI sẽ cần Ollama + model. Bạn có thể cài/tải sau trong Cài đặt.',
      });
    }

    if (isMac) {
      try {
        await saveSummaryModelConfig(latestConfig);
      } catch {
        toast.error('Không lưu được cài đặt mô hình');
        return;
      }
      goNext();
    } else {
      setIsCompleting(true);
      try {
        await completeOnboarding(latestConfig);
        await new Promise(r => setTimeout(r, 100));
        window.location.reload();
      } catch {
        toast.error('Không thể hoàn tất cài đặt', { description: 'Vui lòng thử lại.' });
        setIsCompleting(false);
      }
    }
  };

  const zipDotStatus =
    zipState.status === 'waiting'     ? 'idle'    :
    zipState.status === 'downloading' ? 'loading' :
    zipState.status === 'completed'   ? 'ok'      : 'error';

  return (
    <OnboardingContainer
      title="Đang chuẩn bị..."
      description="Tải nhận dạng giọng nói và chọn nhà cung cấp AI tóm tắt phù hợp với bạn."
      step={2}
      totalSteps={isMac ? 3 : 2}
    >
      <div className="flex flex-col items-center gap-5">
        <motion.div className="w-full max-w-lg space-y-3">

          {/* Card 1: ZipFormer */}
          <motion.div
            initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.35, delay: 0.05 }}
            className="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden"
          >
            <div className="h-0.5 w-full bg-gradient-to-r from-blue-400 to-blue-600" />
            <div className="p-4">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <div className="w-9 h-9 rounded-lg bg-blue-50 flex items-center justify-center shrink-0">
                    <Mic className="w-4 h-4 text-blue-600" />
                  </div>
                  <div>
                    <p className="text-sm font-semibold text-gray-800">Nhận dạng giọng nói tiếng Việt</p>
                    <p className="text-xs text-gray-500 mt-0.5">zipformer-vi-30m · ~30 MB</p>
                  </div>
                </div>
                {zipState.status === 'waiting' ? (
                  <Button
                    type="button"
                    variant="blue"
                    size="sm"
                    onClick={handleStartZipDownload}
                    className="h-8 gap-1.5 rounded-lg px-3 text-xs font-medium shrink-0"
                  >
                    <Download className="w-3 h-3" /> Tải xuống
                  </Button>
                ) : (
                  <StatusDot status={zipDotStatus} />
                )}
              </div>

              {zipState.status === 'downloading' && (
                <div className="mt-3 space-y-1.5">
                  <ProgressBar value={zipState.progress} color="bg-blue-500" />
                  <div className="flex justify-between">
                    <span className="text-[10px] text-gray-400">Đang tải...</span>
                    <span className="text-[10px] font-medium text-blue-600">{Math.round(zipState.progress)}%</span>
                  </div>
                </div>
              )}

              {zipState.status === 'completed' && (
                <p className="mt-2 text-xs text-emerald-600 font-medium">✓ Đã tải xong và sẵn sàng sử dụng</p>
              )}

              {zipState.status === 'error' && (
                <div className="mt-3 rounded-lg bg-red-50 border border-red-100 p-3 space-y-2">
                  <p className="text-xs text-red-600">{zipState.error}</p>
                  <button
                    onClick={handleRetryZip}
                    className="flex items-center gap-1.5 text-xs font-medium text-red-700 hover:text-red-900 transition-colors"
                  >
                    <RefreshCw className="w-3 h-3" /> Thử lại
                  </button>
                </div>
              )}
            </div>
          </motion.div>

          {/* Card 2: Summary model — multi-provider */}
          <motion.div
            initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.35, delay: 0.12 }}
            className="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden"
          >
            <div className="h-0.5 w-full bg-gradient-to-r from-violet-400 to-violet-600" />
            <div className="p-4 space-y-3">
              <div className="flex items-center gap-3">
                <div className="w-9 h-9 rounded-lg bg-violet-50 flex items-center justify-center shrink-0">
                  <Sparkles className="w-4 h-4 text-violet-600" />
                </div>
                <div>
                  <p className="text-sm font-semibold text-gray-800">Model AI tóm tắt</p>
                  <p className="text-xs text-gray-500 mt-0.5">
                    Ollama cục bộ hoặc đám mây (Claude, OpenAI, Groq, OpenRouter…)
                  </p>
                </div>
              </div>

              <ModelSettingsModal
                ref={modelSettingsRef}
                embedded
                allowSkipApiKey
                modelConfig={summaryModelConfig}
                setModelConfig={setSummaryModelConfig}
                onSave={saveSummaryModelConfig}
                skipInitialFetch
              />
            </div>
          </motion.div>
        </motion.div>

        <motion.div
          initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.35, delay: 0.22 }}
          className="w-full max-w-xs space-y-2"
        >
          <Button
            onClick={handleContinue}
            disabled={isCompleting}
            className="w-full h-11 bg-gray-900 hover:bg-gray-700 text-white rounded-xl group transition-colors disabled:opacity-50"
          >
            {isCompleting ? (
              <><Loader2 className="w-4 h-4 mr-2 animate-spin" />Đang chuẩn bị...</>
            ) : (
              <>Tiếp tục <ArrowRight className="w-4 h-4 ml-2 transition-transform group-hover:translate-x-0.5" /></>
            )}
          </Button>

          {!parakeetDownloaded && (
            <p className="text-[11px] text-center text-gray-400">
              Đang tải bộ nhận dạng giọng nói — có thể bỏ qua và tải sau
            </p>
          )}
        </motion.div>
      </div>
    </OnboardingContainer>
  );
}
