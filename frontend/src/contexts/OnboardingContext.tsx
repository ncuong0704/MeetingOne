'use client';

import React, { createContext, useContext, useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { PermissionStatus, OnboardingPermissions } from '@/types/onboarding';
import type { ModelConfig } from '@/components/ModelSettingsModal';
import { persistSummaryModelConfig } from '@/lib/summaryModelConfigSync';

const DEFAULT_SUMMARY_MODEL_CONFIG: ModelConfig = {
  provider: 'ollama',
  model: 'qwen3.5:0.8b',
  whisperModel: 'large-v3',
  apiKey: null,
  ollamaEndpoint: null,
};

interface OnboardingStatus {
  version: string;
  completed: boolean;
  current_step: number;
  model_status: {
    zipformer: string;
    summary: string;
  };
  last_updated: string;
}

interface ParakeetProgressInfo {
  percent: number;
  downloadedMb: number;
  totalMb: number;
  speedMbps: number;
}

interface OnboardingContextType {
  currentStep: number;
  parakeetDownloaded: boolean;
  parakeetProgress: number;
  parakeetProgressInfo: ParakeetProgressInfo;
  summaryModelConfig: ModelConfig;
  databaseExists: boolean;
  isBackgroundDownloading: boolean;
  // Permissions
  permissions: OnboardingPermissions;
  permissionsSkipped: boolean;
  // Navigation
  goToStep: (step: number) => void;
  goNext: () => void;
  goPrevious: () => void;
  // Setters
  setParakeetDownloaded: (value: boolean) => void;
  setSummaryModelConfig: (config: ModelConfig | ((prev: ModelConfig) => ModelConfig)) => void;
  setDatabaseExists: (value: boolean) => void;
  setPermissionStatus: (permission: keyof OnboardingPermissions, status: PermissionStatus) => void;
  setPermissionsSkipped: (skipped: boolean) => void;
  completeOnboarding: (configOverride?: ModelConfig) => Promise<void>;
  saveSummaryModelConfig: (config: ModelConfig) => Promise<void>;
  startBackgroundDownloads: (includeGemma: boolean) => Promise<void>;
  retryParakeetDownload: () => Promise<void>;
}

const OnboardingContext = createContext<OnboardingContextType | undefined>(undefined);

export function OnboardingProvider({ children }: { children: React.ReactNode }) {
  const [currentStep, setCurrentStep] = useState(1);
  const [completed, setCompleted] = useState(false);
  const [parakeetDownloaded, setParakeetDownloaded] = useState(false);
  const [parakeetProgress, setParakeetProgress] = useState(0);
  const [parakeetProgressInfo, setParakeetProgressInfo] = useState<ParakeetProgressInfo>({
    percent: 0,
    downloadedMb: 0,
    totalMb: 0,
    speedMbps: 0,
  });
  const [summaryModelConfig, setSummaryModelConfigState] = useState<ModelConfig>(DEFAULT_SUMMARY_MODEL_CONFIG);
  const summaryModelConfigRef = useRef<ModelConfig>(DEFAULT_SUMMARY_MODEL_CONFIG);
  const setSummaryModelConfig = useCallback(
    (config: ModelConfig | ((prev: ModelConfig) => ModelConfig)) => {
      setSummaryModelConfigState((prev) => {
        const next = typeof config === 'function' ? config(prev) : config;
        summaryModelConfigRef.current = next;
        return next;
      });
    },
    [],
  );
  const [databaseExists, setDatabaseExists] = useState(false);
  const [isBackgroundDownloading, setIsBackgroundDownloading] = useState(false);

  // Permissions state
  const [permissions, setPermissions] = useState<OnboardingPermissions>({
    microphone: 'not_determined',
    systemAudio: 'not_determined',
    screenRecording: 'not_determined',
  });
  const [permissionsSkipped, setPermissionsSkipped] = useState(false);

  /** Until true, skip debounced auto-save so we never persist default React state over disk. */
  const [onboardingStatusHydrated, setOnboardingStatusHydrated] = useState(false);

  const saveTimeoutRef = useRef<NodeJS.Timeout>();

  // Load status on mount and initialize database
  useEffect(() => {
    loadOnboardingStatus();
    checkDatabaseStatus();
    initializeDatabaseInBackground();

    // Fetch and set recommended Ollama model when using local Ollama provider
    const fetchRecommendation = async () => {
      try {
        const rec = await invoke<{ model: string }>('get_ollama_model_recommendation');
        setSummaryModelConfig((prev) => ({
          ...prev,
          model: rec.model,
        }));
        console.log('[OnboardingContext] Set recommended Ollama model:', rec.model);
      } catch (error) {
        console.error('[OnboardingContext] Failed to get Ollama recommendation:', error);
      }
    };
    fetchRecommendation();
  }, []);

  // Initialize database silently in background (moved from SetupOverviewStep)
  const initializeDatabaseInBackground = async () => {
    try {
      console.log('[OnboardingContext] Starting background database initialization');
      const isFirstLaunch = await invoke<boolean>('check_first_launch');

      if (!isFirstLaunch) {
        console.log('[OnboardingContext] Database exists, skipping initialization');
        setDatabaseExists(true);
        return;
      }

      // First launch - attempt auto-detection and import
      await performAutoDetection();
    } catch (error) {
      console.error('[OnboardingContext] Database initialization failed:', error);
      // Don't throw - database init failure shouldn't block onboarding
    }
  };

  const performAutoDetection = async () => {
    // Check Homebrew (macOS only)
    if (typeof navigator !== 'undefined' && navigator.platform?.toLowerCase().includes('mac')) {
      const homebrewDbPath = '/usr/local/var/meetingone/meeting_minutes.db';
      try {
        const homebrewCheck = await invoke<{ exists: boolean; size: number } | null>(
          'check_homebrew_database',
          { path: homebrewDbPath }
        );

        if (homebrewCheck?.exists) {
          console.log('[OnboardingContext] Found Homebrew database, importing');
          await invoke('import_and_initialize_database', { legacyDbPath: homebrewDbPath });
          setDatabaseExists(true);
          return;
        }
      } catch (e) {
        console.log('[OnboardingContext] Homebrew check failed, continuing:', e);
      }
    }

    // Check default legacy database location
    try {
      const legacyPath = await invoke<string | null>('check_default_legacy_database');
      if (legacyPath) {
        console.log('[OnboardingContext] Found legacy database, importing');
        await invoke('import_and_initialize_database', { legacyDbPath: legacyPath });
        setDatabaseExists(true);
        return;
      }
    } catch (e) {
      console.log('[OnboardingContext] Legacy check failed, continuing:', e);
    }

    // No legacy database found - initialize fresh
    console.log('[OnboardingContext] No legacy database found, initializing fresh');
    await invoke('initialize_fresh_database');
    setDatabaseExists(true);
  };

  const isCompletingRef = useRef(false);

  // Auto-save on state change (debounced)
  useEffect(() => {
    if (!onboardingStatusHydrated) return;
    if (saveTimeoutRef.current) clearTimeout(saveTimeoutRef.current);

    // Don't auto-save if completed (to avoid overwriting completion status)
    // Also don't auto-save if we are currently in the process of completing
    if (completed || isCompletingRef.current) return;

    saveTimeoutRef.current = setTimeout(() => {
      saveOnboardingStatus();
    }, 1000);

    return () => {
      if (saveTimeoutRef.current) clearTimeout(saveTimeoutRef.current);
    };
  }, [currentStep, parakeetDownloaded, completed, onboardingStatusHydrated]);

  // Listen to Parakeet download progress
  useEffect(() => {
    const unlisten = listen<{ progress: number }>(
      'zipformer-model-download-progress',
      (event) => {
        const { progress } = event.payload;
        setParakeetProgress(progress);
        setParakeetProgressInfo({ percent: progress, downloadedMb: 0, totalMb: 30, speedMbps: 0 });
        if (progress >= 100) setParakeetDownloaded(true);
      }
    );

    const unlistenComplete = listen(
      'zipformer-model-download-complete',
      () => {
        setParakeetDownloaded(true);
        setParakeetProgress(100);
      }
    );

    const unlistenError = listen<{ error: string }>(
      'zipformer-model-download-error',
      (event) => {
        console.error('ZipFormer download error:', event.payload.error);
      }
    );

    return () => {
      unlisten.then(fn => fn());
      unlistenComplete.then(fn => fn());
      unlistenError.then(fn => fn());
    };
  }, []);


  const checkDatabaseStatus = async () => {
    try {
      const isFirstLaunch = await invoke<boolean>('check_first_launch');
      setDatabaseExists(!isFirstLaunch);
      console.log('[OnboardingContext] Database exists:', !isFirstLaunch);
    } catch (error) {
      console.error('[OnboardingContext] Failed to check database status:', error);
      setDatabaseExists(false);
    }
  };

  const loadOnboardingStatus = async () => {
    try {
      const status = await invoke<OnboardingStatus | null>('get_onboarding_status');
      if (status) {
        console.log('[OnboardingContext] Loaded saved status:', status);

        // Don't trust saved status - verify actual model status on disk
        const verifiedStatus = await verifyModelStatus(status);

        setCurrentStep(verifiedStatus.currentStep);
        setCompleted(verifiedStatus.completed);
        setParakeetDownloaded(verifiedStatus.parakeetDownloaded);

        console.log('[OnboardingContext] Verified status:', verifiedStatus);

        // Check if any downloads are active to restore isBackgroundDownloading state
        await checkActiveDownloads();
      }
    } catch (error) {
      console.error('[OnboardingContext] Failed to load onboarding status:', error);
    } finally {
      setOnboardingStatusHydrated(true);
    }
  };

  // Load saved summary model config from DB so onboarding reflects settings/popup changes
  useEffect(() => {
    if (!onboardingStatusHydrated) return;
    const loadSavedSummaryConfig = async () => {
      try {
        const data = await invoke<ModelConfig | null>('api_get_model_config');
        if (!data?.provider) return;
        if (data.provider !== 'ollama' && data.provider !== 'custom-openai' && !data.apiKey) {
          try {
            data.apiKey = await invoke<string>('api_get_api_key', { provider: data.provider });
          } catch {
            // no key stored yet
          }
        }
        setSummaryModelConfig(data);
      } catch (error) {
        console.error('[OnboardingContext] Failed to load saved summary model config:', error);
      }
    };
    void loadSavedSummaryConfig();
  }, [onboardingStatusHydrated, setSummaryModelConfig]);

  // Verify that models actually exist on disk, not just trust saved JSON
  const verifyModelStatus = async (savedStatus: OnboardingStatus) => {
    let parakeetDownloaded = false;

    // Verify ZipFormer model files exist on disk (validateModelReady checks files AND loads if present)
    try {
      await invoke('zipformer_validate_model_ready');
      parakeetDownloaded = true;
      console.log('[OnboardingContext] ZipFormer model files verified on disk: true');
    } catch (error) {
      // Model files not present — genuinely not downloaded
      parakeetDownloaded = false;
      console.log('[OnboardingContext] ZipFormer model files not found on disk:', error);
    }

    // Determine the correct step based on verified status
    let currentStep = savedStatus.current_step;
    let completed = savedStatus.completed;

    // Migrate from 4-step flow (v1) to 3-step flow
    if (currentStep > 2) {
      currentStep -= 1;
    }
    // Clamp step to new max (3)
    if (currentStep > 3) {
      currentStep = 2;
    }

    return {
      currentStep,
      completed,
      parakeetDownloaded,
    };
  };

  const saveOnboardingStatus = async () => {
    // Safety check: if we are in the process of completing, DO NOT save
    // This prevents a race condition where a download completion event triggers a save
    // that overwrites the "completed" status set by completeOnboarding
    if (isCompletingRef.current) {
      console.log('[OnboardingContext] Skipping saveOnboardingStatus because completion is in progress');
      return;
    }

    try {
      await invoke('save_onboarding_status_cmd', {
        status: {
          version: '1.0',
          completed: completed,
          current_step: currentStep,
          model_status: {
            zipformer: parakeetDownloaded ? 'downloaded' : 'not_downloaded',
            summary: 'not_downloaded',
          },
          last_updated: new Date().toISOString(),
        },
      });
    } catch (error) {
      console.error('[OnboardingContext] Failed to save onboarding status:', error);
    }
  };

  const saveSummaryModelConfig = useCallback(async (config: ModelConfig) => {
    setSummaryModelConfig(config);
    await persistSummaryModelConfig(config);
  }, [setSummaryModelConfig]);

  // Keep onboarding UI in sync when settings or popup save
  useEffect(() => {
    const setup = async () => {
      const unlisten = await listen<ModelConfig>('model-config-updated', (event) => {
        setSummaryModelConfig(event.payload);
      });
      return unlisten;
    };
    let cleanup: (() => void) | undefined;
    setup().then((fn) => { cleanup = fn; });
    return () => { cleanup?.(); };
  }, [setSummaryModelConfig]);

  const completeOnboarding = async (configOverride?: ModelConfig) => {
    try {
      isCompletingRef.current = true;

      if (saveTimeoutRef.current) {
        clearTimeout(saveTimeoutRef.current);
        saveTimeoutRef.current = undefined;
      }

      const configToSave = configOverride ?? summaryModelConfigRef.current;
      await saveSummaryModelConfig(configToSave);
      await invoke('complete_onboarding');
      setCompleted(true);
      console.log('[OnboardingContext] Onboarding completed with provider:', configToSave.provider);

      isCompletingRef.current = false;
    } catch (error) {
      console.error('[OnboardingContext] Failed to complete onboarding:', error);
      isCompletingRef.current = false;
      throw error;
    }
  };

  // Start background downloads — only ZipFormer (Ollama handles its own models)
  const startBackgroundDownloads = async (_includeGemma: boolean) => {
    console.log('[OnboardingContext] Starting background downloads (ZipFormer only)');
    setIsBackgroundDownloading(true);

    try {
      if (!parakeetDownloaded) {
        console.log('[OnboardingContext] Starting ZipFormer download');
        invoke('zipformer_download_model')
          .catch(err => console.error('[OnboardingContext] ZipFormer download failed:', err));
      }
    } catch (error) {
      console.error('[OnboardingContext] Failed to start background downloads:', error);
      setIsBackgroundDownloading(false);
      throw error;
    }
  };

  // Check if any models are currently downloading (for re-entry)
  const checkActiveDownloads = async () => {
    try {
      const status = await invoke<{ type: string }>('zipformer_get_model_status');
      if (status?.type === 'Downloading') {
        console.log('[OnboardingContext] Detected active ZipFormer download on mount');
        setIsBackgroundDownloading(true);
      }
    } catch (error) {
      console.warn('[OnboardingContext] Failed to check active downloads:', error);
    }
  };

  const retryParakeetDownload = async () => {
    console.log('[OnboardingContext] Retrying ZipFormer download');
    try {
      await invoke('zipformer_download_model');
    } catch (error) {
      console.error('[OnboardingContext] Retry failed:', error);
      throw error;
    }
  };

  const setPermissionStatus = useCallback((permission: keyof OnboardingPermissions, status: PermissionStatus) => {
    setPermissions((prev: OnboardingPermissions) => ({
      ...prev,
      [permission]: status,
    }));
  }, []);

  const goToStep = useCallback((step: number) => {
    setCurrentStep(Math.max(1, Math.min(step, 3)));
  }, []);

  const goNext = useCallback(() => {
    setCurrentStep((prev: number) => {
      const next = prev + 1;
      // Don't go past step 3
      return Math.min(next, 3);
    });
  }, []);

  const goPrevious = useCallback(() => {
    setCurrentStep((prev: number) => {
      const previous = prev - 1;
      // Don't go below step 1
      return Math.max(previous, 1);
    });
  }, []);

  return (
    <OnboardingContext.Provider
      value={{
        currentStep,
        parakeetDownloaded,
        parakeetProgress,
        parakeetProgressInfo,
        summaryModelConfig,
        databaseExists,
        isBackgroundDownloading,
        permissions,
        permissionsSkipped,
        goToStep,
        goNext,
        goPrevious,
        setParakeetDownloaded,
        setSummaryModelConfig,
        setDatabaseExists,
        setPermissionStatus,
        setPermissionsSkipped,
        completeOnboarding,
        saveSummaryModelConfig,
        startBackgroundDownloads,
        retryParakeetDownload,
      }}
    >
      {children}
    </OnboardingContext.Provider>
  );
}

export function useOnboarding() {
  const context = useContext(OnboardingContext);
  if (!context) {
    throw new Error('useOnboarding must be used within OnboardingProvider');
  }
  return context;
}
