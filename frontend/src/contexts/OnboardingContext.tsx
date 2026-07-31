'use client';

import React, { createContext, useContext, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ModelConfig } from '@/components/ModelSettingsModal';
import { persistSummaryModelConfig } from '@/lib/summaryModelConfigSync';
import { createDefaultSummaryModelConfig } from '@/constants/modelDefaults';
import { isTauriRuntime, setBrowserOnboardingCompleted } from '@/lib/tauriRuntime';

const DEFAULT_SUMMARY_MODEL_CONFIG: ModelConfig = createDefaultSummaryModelConfig();

interface OnboardingContextType {
  completeOnboarding: () => Promise<void>;
}

const OnboardingContext = createContext<OnboardingContextType | undefined>(undefined);

export function OnboardingProvider({ children }: { children: React.ReactNode }) {
  const summaryModelConfigRef = useRef<ModelConfig>(DEFAULT_SUMMARY_MODEL_CONFIG);
  const isCompletingRef = useRef(false);

  useEffect(() => {
    void initializeDatabaseInBackground();
  }, []);

  const initializeDatabaseInBackground = async () => {
    try {
      const isFirstLaunch = await invoke<boolean>('check_first_launch');
      if (!isFirstLaunch) return;
      await performAutoDetection();
    } catch (error) {
      console.error('[OnboardingContext] Database initialization failed:', error);
    }
  };

  const performAutoDetection = async () => {
    if (typeof navigator !== 'undefined' && navigator.platform?.toLowerCase().includes('mac')) {
      const homebrewDbPath = '/usr/local/var/meetingone/meeting_minutes.db';
      try {
        const homebrewCheck = await invoke<{ exists: boolean; size: number } | null>(
          'check_homebrew_database',
          { path: homebrewDbPath },
        );
        if (homebrewCheck?.exists) {
          await invoke('import_and_initialize_database', { legacyDbPath: homebrewDbPath });
          return;
        }
      } catch {
        // continue
      }
    }

    try {
      const legacyPath = await invoke<string | null>('check_default_legacy_database');
      if (legacyPath) {
        await invoke('import_and_initialize_database', { legacyDbPath: legacyPath });
        return;
      }
    } catch {
      // continue
    }

    await invoke('initialize_fresh_database');
  };

  const completeOnboarding = async () => {
    if (isCompletingRef.current) return;

    try {
      isCompletingRef.current = true;
      const configToSave = summaryModelConfigRef.current;

      if (isTauriRuntime()) {
        await persistSummaryModelConfig(configToSave);
        await invoke('complete_onboarding');
      } else {
        setBrowserOnboardingCompleted();
      }
    } catch (error) {
      console.error('[OnboardingContext] Failed to complete onboarding:', error);
      throw error;
    } finally {
      isCompletingRef.current = false;
    }
  };

  return (
    <OnboardingContext.Provider value={{ completeOnboarding }}>
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
