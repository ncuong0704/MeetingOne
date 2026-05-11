import React, { useEffect, useState, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  Mic, Sparkles, Check, Loader2, AlertCircle,
  Download, Star, Cpu, ArrowRight, RefreshCw, ExternalLink,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { toast } from 'sonner';
import { motion } from 'framer-motion';
import { cn } from '@/lib/utils';

// ── types (unchanged) ──────────────────────────────────────────────────────
type ZipStatus = 'waiting' | 'downloading' | 'completed' | 'error';
interface ZipState { status: ZipStatus; progress: number; error?: string; }
interface OllamaModelOption {
  name: string; family: string; size_gb: number;
  description: string; is_pulled: boolean; is_recommended: boolean;
}
type OllamaStatus = 'checking' | 'not_installed' | 'selecting' | 'pulling' | 'ready';

// ── small helpers ──────────────────────────────────────────────────────────
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
    <div className="w-full h-1.5 rounded-full bg-gray-100 overflow-hidden">
      <motion.div
        className={cn('h-full rounded-full', color)}
        initial={{ width: 0 }}
        animate={{ width: `${value}%` }}
        transition={{ duration: 0.4, ease: 'easeOut' }}
      />
    </div>
  );
}

// ── component ──────────────────────────────────────────────────────────────
export function DownloadProgressStep() {
  const {
    goNext, setSelectedSummaryModel,
    parakeetDownloaded, setParakeetDownloaded,
    startBackgroundDownloads, completeOnboarding,
  } = useOnboarding();

  const [isMac, setIsMac] = useState(false);
  const [ollamaStatus, setOllamaStatus] = useState<OllamaStatus>('checking');
  const [modelOptions, setModelOptions] = useState<OllamaModelOption[] | null>(null);
  const [hardwareInfo, setHardwareInfo] = useState<{ ram_gb?: number; max_size_gb?: number } | null>(null);
  const [selectedModel, setSelectedModel] = useState('');
  const [pullingModel, setPullingModel] = useState('');
  const [pullProgress, setPullProgress] = useState(0);
  const [pullError, setPullError] = useState<string | null>(null);
  const [zipState, setZipState] = useState<ZipState>({
    status: parakeetDownloaded ? 'completed' : 'waiting',
    progress: parakeetDownloaded ? 100 : 0,
  });
  const [isCompleting, setIsCompleting] = useState(false);
  const downloadStartedRef = useRef(false);
  const retryingRef = useRef(false);

  // ── load models (logic unchanged) ─────────────────────────────────────
  const loadModels = async () => {
    setOllamaStatus('checking');
    setModelOptions(null);
    setHardwareInfo(null);

    let ollamaReachable = false;
    try {
      await invoke<unknown[]>('get_ollama_models', { endpoint: null });
      ollamaReachable = true;
    } catch (err) {
      const s = String(err);
      if (s.includes('No models found') || s.includes('NoModelsFound')) ollamaReachable = true;
    }

    if (!ollamaReachable) { setOllamaStatus('not_installed'); return; }

    try {
      const options = await invoke<OllamaModelOption[]>('get_ollama_model_options', { endpoint: null });
      setModelOptions(options);
      const rec = await invoke<{ model: string; ram_gb: number; size_gb: number }>('get_ollama_model_recommendation');
      setHardwareInfo({ ram_gb: rec.ram_gb, max_size_gb: rec.size_gb });

      const alreadyPulled = options.find(m => m.is_pulled);
      if (alreadyPulled) {
        const best = options.find(m => m.is_pulled && m.is_recommended) ?? alreadyPulled;
        setSelectedModel(best.name);
        setSelectedSummaryModel(best.name);
        setOllamaStatus('ready');
      } else {
        const best = options.find(m => m.is_recommended) ?? options[0];
        if (best) { setSelectedModel(best.name); setSelectedSummaryModel(best.name); }
        setOllamaStatus('selecting');
      }
    } catch {
      setOllamaStatus('selecting');
    }
  };

  useEffect(() => {
    const checkPlatform = async () => {
      try {
        const { platform } = await import('@tauri-apps/plugin-os');
        setIsMac(platform() === 'macos');
      } catch { setIsMac(navigator.userAgent.includes('Mac')); }
    };
    checkPlatform();
    loadModels();
  }, []);

  useEffect(() => {
    if (downloadStartedRef.current) return;
    downloadStartedRef.current = true;
    if (!parakeetDownloaded) {
      setZipState(prev => ({ ...prev, status: 'downloading' }));
      startBackgroundDownloads(false).catch(err =>
        setZipState(prev => ({ ...prev, status: 'error', error: String(err) }))
      );
    }
  }, []);

  useEffect(() => {
    const unP = listen<{ progress: number }>('zipformer-model-download-progress', (e) => {
      const p = e.payload.progress;
      setZipState(prev => ({ ...prev, status: p >= 100 ? 'completed' : 'downloading', progress: p }));
      if (p >= 100) setParakeetDownloaded(true);
    });
    const unC = listen('zipformer-model-download-complete', () => {
      setZipState({ status: 'completed', progress: 100 });
      setParakeetDownloaded(true);
    });
    const unE = listen<{ error: string }>('zipformer-model-download-error', (e) =>
      setZipState(prev => ({ ...prev, status: 'error', error: e.payload.error }))
    );
    return () => { unP.then(f => f()); unC.then(f => f()); unE.then(f => f()); };
  }, []);

  useEffect(() => {
    const unP = listen<{ modelName: string; progress: number }>(
      'ollama-model-download-progress', (e) => setPullProgress(e.payload.progress)
    );
    const unC = listen<{ modelName: string }>('ollama-model-download-complete', (e) => {
      const tag = e.payload.modelName;
      setPullProgress(100); setPullingModel(''); setPullError(null);
      setModelOptions(prev => prev ? prev.map(m => m.name === tag ? { ...m, is_pulled: true } : m) : prev);
      setSelectedModel(tag); setSelectedSummaryModel(tag);
      setOllamaStatus('ready');
      toast.success(`Đã tải xong ${tag}!`);
    });
    const unE = listen<{ modelName: string; error: string }>('ollama-model-download-error', (e) => {
      setPullingModel(''); setPullError(e.payload.error); setOllamaStatus('selecting');
      toast.error('Tải model thất bại', { description: e.payload.error });
    });
    return () => { unP.then(f => f()); unC.then(f => f()); unE.then(f => f()); };
  }, []);

  const handlePullModel = async (tag: string) => {
    setPullError(null); setPullProgress(0); setPullingModel(tag); setOllamaStatus('pulling');
    try {
      await invoke('pull_ollama_model', { modelName: tag, endpoint: null });
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setPullingModel(''); setPullError(msg); setOllamaStatus('selecting');
      toast.error('Không thể kéo model', { description: msg });
    }
  };

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
      if (ok && !parakeetDownloaded) { setParakeetDownloaded(true); setZipState({ status: 'completed', progress: 100 }); }
    } catch { /* ignore */ }

    if (!parakeetDownloaded) {
      toast.message('Bạn có thể tải model sau', { description: 'Chưa có bộ nhận dạng giọng nói. Bạn vẫn có thể tiếp tục và tải sau trong Cài đặt.' });
    } else if (ollamaStatus !== 'ready') {
      toast.message('Bạn có thể tải model tóm tắt sau', { description: 'Tóm tắt AI sẽ cần Ollama + model. Bạn có thể cài/tải sau trong Cài đặt.' });
    }

    if (isMac) {
      goNext();
    } else {
      setIsCompleting(true);
      try {
        await completeOnboarding();
        await new Promise(r => setTimeout(r, 100));
        window.location.reload();
      } catch {
        toast.error('Không thể hoàn tất cài đặt', { description: 'Vui lòng thử lại.' });
        setIsCompleting(false);
      }
    }
  };

  const pulledOptions   = modelOptions?.filter(m => m.is_pulled)  ?? [];
  const unpulledOptions = modelOptions?.filter(m => !m.is_pulled) ?? [];

  const zipDotStatus =
    zipState.status === 'waiting'     ? 'idle'    :
    zipState.status === 'downloading' ? 'loading' :
    zipState.status === 'completed'   ? 'ok'      : 'error';

  const ollamaDotStatus =
    ollamaStatus === 'checking'      ? 'loading' :
    ollamaStatus === 'ready'         ? 'ok'      :
    ollamaStatus === 'not_installed' ? 'error'   :
    ollamaStatus === 'pulling'       ? 'loading' : 'warn';

  const isBusy = isCompleting || ollamaStatus === 'checking' || ollamaStatus === 'pulling';

  // ── render ──────────────────────────────────────────────────────────────
  return (
    <OnboardingContainer
      title="Đang chuẩn bị..."
      description="Tải bộ nhận dạng giọng nói và chọn model AI tóm tắt."
      step={3}
      totalSteps={isMac ? 4 : 3}
    >
      <div className="flex flex-col items-center gap-5">
        <div className="w-full max-w-lg space-y-3">

          {/* ── Card 1: ZipFormer ──────────────────────────────────────── */}
          <motion.div
            initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.35, delay: 0.05 }}
            className="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden"
          >
            {/* Accent bar */}
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
                <StatusDot status={zipDotStatus} />
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

          {/* ── Card 2: Ollama ─────────────────────────────────────────── */}
          <motion.div
            initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.35, delay: 0.12 }}
            className="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden"
          >
            <div className="h-0.5 w-full bg-gradient-to-r from-violet-400 to-violet-600" />

            <div className="p-4 space-y-3">
              {/* Header */}
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <div className="w-9 h-9 rounded-lg bg-violet-50 flex items-center justify-center shrink-0">
                    <Sparkles className="w-4 h-4 text-violet-600" />
                  </div>
                  <div>
                    <p className="text-sm font-semibold text-gray-800">Model AI tóm tắt (Ollama)</p>
                    <p className="text-xs text-gray-500 mt-0.5">
                      {ollamaStatus === 'checking'      ? 'Đang phân tích phần cứng...'          :
                       ollamaStatus === 'not_installed' ? 'Ollama chưa được cài đặt'             :
                       ollamaStatus === 'ready'         ? `Đã chọn: ${selectedModel}`            :
                       ollamaStatus === 'pulling'       ? `Đang tải ${pullingModel}...`          :
                       `${modelOptions?.length ?? 0} model phù hợp với máy của bạn`}
                    </p>
                  </div>
                </div>
                <StatusDot status={ollamaDotStatus} />
              </div>

              {/* Hardware badge */}
              {hardwareInfo && ollamaStatus !== 'not_installed' && (
                <div className="flex items-center gap-1.5 bg-gray-50 rounded-lg border border-gray-100 px-3 py-1.5">
                  <Cpu className="w-3 h-3 text-gray-400 shrink-0" />
                  <span className="text-[11px] text-gray-500">
                    {hardwareInfo.ram_gb ? `RAM ${hardwareInfo.ram_gb} GB` : 'Phần cứng'}
                    {hardwareInfo.max_size_gb ? ` · Gợi ý model ~${hardwareInfo.max_size_gb} GB` : ''}
                  </span>
                </div>
              )}

              {/* ── Not installed ─────────────────────────────────────── */}
              {ollamaStatus === 'not_installed' && (
                <div className="rounded-lg bg-red-50 border border-red-100 p-3 space-y-2">
                  <p className="text-xs text-red-700">Bạn chưa cài Ollama. Tải và cài đặt để sử dụng tóm tắt AI.</p>
                  <button
                    onClick={() => invoke('open_external_url', { url: 'https://ollama.com/download' })}
                    className="flex items-center gap-1.5 w-full h-8 justify-center bg-gray-900 hover:bg-gray-700 text-white text-xs font-medium rounded-lg transition-colors"
                  >
                    <ExternalLink className="w-3 h-3" />
                    Tải Ollama tại ollama.com/download
                  </button>
                  <button
                    onClick={loadModels}
                    className="w-full h-8 border border-gray-200 hover:bg-gray-50 text-gray-600 text-xs font-medium rounded-lg transition-colors flex items-center justify-center gap-1.5"
                  >
                    <RefreshCw className="w-3 h-3" /> Kiểm tra lại
                  </button>
                </div>
              )}

              {/* ── Model selection ───────────────────────────────────── */}
              {(ollamaStatus === 'selecting' || ollamaStatus === 'pulling') && (
                <div className="space-y-3">
                  {pullError && (
                    <div className="rounded-lg bg-red-50 border border-red-100 px-3 py-2">
                      <p className="text-xs text-red-600">{pullError}</p>
                    </div>
                  )}

                  {/* Already pulled */}
                  {pulledOptions.length > 0 && (
                    <div className="space-y-1.5">
                      <p className="text-[11px] font-semibold text-gray-400 uppercase tracking-wide">Đã có sẵn trên máy</p>
                      {pulledOptions.map(m => (
                        <div
                          key={m.name}
                          onClick={() => { setSelectedModel(m.name); setSelectedSummaryModel(m.name); }}
                          className={cn(
                            'flex items-center justify-between p-3 rounded-lg border cursor-pointer transition-all',
                            selectedModel === m.name
                              ? 'border-gray-900 bg-gray-50 ring-1 ring-gray-900'
                              : 'border-gray-100 hover:border-gray-300 hover:bg-gray-50'
                          )}
                        >
                          <div className="flex items-center gap-2.5">
                            <div className={cn(
                              'w-4 h-4 rounded-full border-2 flex items-center justify-center shrink-0 transition-colors',
                              selectedModel === m.name ? 'border-gray-900 bg-gray-900' : 'border-gray-300'
                            )}>
                              {selectedModel === m.name && <div className="w-1.5 h-1.5 rounded-full bg-white" />}
                            </div>
                            <div>
                              <div className="flex items-center gap-1.5">
                                <span className="text-xs font-semibold text-gray-900">{m.name}</span>
                                {m.is_recommended && <Star className="w-3 h-3 text-amber-400 fill-amber-400" />}
                                <span className="text-[10px] px-1.5 py-0.5 bg-emerald-100 text-emerald-700 rounded-full font-medium">Đã tải</span>
                              </div>
                              <p className="text-[10px] text-gray-500">{m.size_gb} GB · {m.description}</p>
                            </div>
                          </div>
                        </div>
                      ))}
                    </div>
                  )}

                  {/* To pull */}
                  <div className="space-y-1.5">
                    <p className="text-[11px] font-semibold text-gray-400 uppercase tracking-wide">
                      {pulledOptions.length > 0 ? 'Tải thêm model' : 'Chọn model để tải về'}
                    </p>
                    <div className="space-y-1.5 max-h-52 overflow-y-auto pr-0.5">
                      {unpulledOptions.map(m => {
                        const isPullingThis = pullingModel === m.name;
                        return (
                          <div
                            key={m.name}
                            className={cn(
                              'p-3 rounded-lg border transition-all',
                              isPullingThis ? 'border-violet-200 bg-violet-50' : 'border-gray-100'
                            )}
                          >
                            <div className="flex items-center justify-between gap-3">
                              <div className="min-w-0">
                                <div className="flex items-center gap-1.5 flex-wrap">
                                  <span className="text-xs font-semibold text-gray-900">{m.name}</span>
                                  {m.is_recommended && <Star className="w-3 h-3 text-amber-400 fill-amber-400 shrink-0" />}
                                  <span className="text-[10px] text-gray-400 font-medium">{m.family}</span>
                                </div>
                                <p className="text-[10px] text-gray-500 mt-0.5">{m.size_gb} GB · {m.description}</p>
                              </div>
                              {isPullingThis ? (
                                <Loader2 className="w-4 h-4 text-violet-600 animate-spin shrink-0" />
                              ) : (
                                <button
                                  onClick={() => handlePullModel(m.name)}
                                  disabled={ollamaStatus === 'pulling'}
                                  className="flex items-center gap-1.5 px-2.5 h-7 bg-gray-900 hover:bg-gray-700 disabled:opacity-40 text-white text-xs font-medium rounded-lg transition-colors shrink-0"
                                >
                                  <Download className="w-3 h-3" /> Tải
                                </button>
                              )}
                            </div>
                            {isPullingThis && pullProgress > 0 && (
                              <div className="mt-2.5 space-y-1">
                                <ProgressBar value={pullProgress} color="bg-violet-500" />
                                <div className="flex justify-between">
                                  <span className="text-[10px] text-violet-500">Đang tải...</span>
                                  <span className="text-[10px] font-medium text-violet-600">{pullProgress}%</span>
                                </div>
                              </div>
                            )}
                          </div>
                        );
                      })}
                    </div>
                  </div>

                  {pulledOptions.length > 0 && (
                    <button
                      onClick={() => {
                        const best = pulledOptions.find(m => m.is_recommended) ?? pulledOptions[0];
                        setSelectedModel(best.name); setSelectedSummaryModel(best.name); setOllamaStatus('ready');
                      }}
                      className="w-full h-8 bg-gray-900 hover:bg-gray-700 text-white text-xs font-medium rounded-lg transition-colors"
                    >
                      Dùng model đã có sẵn
                    </button>
                  )}
                </div>
              )}

              {/* ── Ready ─────────────────────────────────────────────── */}
              {ollamaStatus === 'ready' && selectedModel && (
                <div className="flex items-center justify-between rounded-lg bg-emerald-50 border border-emerald-100 px-3 py-2.5">
                  <div>
                    <p className="text-xs font-semibold text-emerald-800">{selectedModel} · Sẵn sàng</p>
                    {modelOptions?.find(m => m.name === selectedModel) && (
                      <p className="text-[10px] text-emerald-700 mt-0.5">
                        {modelOptions.find(m => m.name === selectedModel)?.size_gb} GB ·{' '}
                        {modelOptions.find(m => m.name === selectedModel)?.description}
                      </p>
                    )}
                  </div>
                  <button
                    onClick={() => setOllamaStatus('selecting')}
                    className="text-[10px] text-gray-500 hover:text-gray-700 underline shrink-0 ml-3 transition-colors"
                  >
                    Đổi model
                  </button>
                </div>
              )}
            </div>
          </motion.div>
        </div>

        {/* ── Continue ──────────────────────────────────────────────────── */}
        <motion.div
          initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.35, delay: 0.22 }}
          className="w-full max-w-xs space-y-2"
        >
          <Button
            onClick={handleContinue}
            disabled={isBusy}
            className="w-full h-11 bg-gray-900 hover:bg-gray-700 text-white rounded-xl group transition-colors disabled:opacity-50"
          >
            {isBusy ? (
              <><Loader2 className="w-4 h-4 mr-2 animate-spin" />Đang xử lý...</>
            ) : (
              <>Tiếp tục <ArrowRight className="w-4 h-4 ml-2 transition-transform group-hover:translate-x-0.5" /></>
            )}
          </Button>

          {!parakeetDownloaded && (
            <p className="text-[11px] text-center text-gray-400">
              Đang tải bộ nhận dạng giọng nói — có thể bỏ qua và tải sau
            </p>
          )}
          {ollamaStatus === 'not_installed' && (
            <p className="text-[11px] text-center text-gray-400">
              Chưa có Ollama — bạn có thể tiếp tục và cài sau trong Cài đặt
            </p>
          )}
        </motion.div>
      </div>
    </OnboardingContainer>
  );
}
