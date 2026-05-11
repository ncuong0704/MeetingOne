'use client';

import React, { useState, useEffect } from 'react';
import { ArrowLeft, Settings2, Mic, Database as DatabaseIcon, SparkleIcon } from 'lucide-react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import { TranscriptSettings } from '@/components/TranscriptSettings';
import { RecordingSettings } from '@/components/RecordingSettings';
import { PreferenceSettings } from '@/components/PreferenceSettings';
import { SummaryModelSettings } from '@/components/SummaryModelSettings';
import { useConfig } from '@/contexts/ConfigContext';
import { cn } from '@/lib/utils';

const TABS = [
  { value: 'general',            label: 'Chung',      icon: Settings2,   desc: 'Lưu trữ & tùy chọn' },
  { value: 'recording',          label: 'Ghi âm',     icon: Mic,         desc: 'Thiết bị & lưu file' },
  { value: 'Transcriptionmodels',label: 'Nhận dạng',  icon: DatabaseIcon, desc: 'Mô hình giọng nói' },
  { value: 'summaryModels',      label: 'Tóm tắt AI', icon: SparkleIcon, desc: 'Mô hình tóm tắt' },
] as const;

type TabValue = typeof TABS[number]['value'];

export default function SettingsPage() {
  const router = useRouter();
  const { transcriptModelConfig, setTranscriptModelConfig } = useConfig();
  const [activeTab, setActiveTab] = useState<TabValue>('general');

  useEffect(() => {
    const loadTranscriptConfig = async () => {
      try {
        const config = await invoke('api_get_transcript_config') as any;
        if (config) {
          setTranscriptModelConfig({
            provider: config.provider || 'localWhisper',
            model: config.model || 'large-v3',
            apiKey: config.apiKey || null,
          });
        }
      } catch (error) {
        console.error('Failed to load transcript config:', error);
      }
    };
    loadTranscriptConfig();
  }, [setTranscriptModelConfig]);

  return (
    <div className="h-screen bg-gray-50 flex flex-col overflow-hidden">

      {/* ── Header ────────────────────────────────────────────────────────── */}
      <header className="shrink-0 bg-white border-b border-gray-100">
        <div className="max-w-6xl mx-auto px-8">
          {/* Breadcrumb */}
          <div className="flex items-center gap-2 py-5">
            <button
              onClick={() => router.back()}
              className="flex items-center gap-1.5 text-sm text-gray-400 hover:text-gray-700 transition-colors"
            >
              <ArrowLeft className="w-3.5 h-3.5" />
              Quay lại
            </button>
            <span className="text-gray-200">/</span>
            <span className="text-xl font-semibold text-gray-800">Cài đặt</span>
          </div>

          {/* Horizontal tab bar */}
          <div className="flex items-end gap-1 -mb-px">
            {TABS.map(({ value, label, icon: Icon }) => {
              const isActive = activeTab === value;
              return (
                <button
                  key={value}
                  onClick={() => setActiveTab(value)}
                  className={cn(
                    'relative flex items-center gap-2 px-5 py-3 text-base font-medium rounded-t-lg transition-colors duration-150 border border-transparent',
                    isActive
                      ? 'text-gray-900 bg-gray-50 border-gray-100 border-b-gray-50'
                      : 'text-gray-500 hover:text-gray-700 hover:bg-gray-50/60'
                  )}
                >
                  <Icon className={cn('w-3.5 h-3.5 shrink-0', isActive ? 'text-gray-700' : 'text-gray-400')} />
                  {label}
                  {isActive && (
                    <motion.div
                      layoutId="tab-indicator"
                      className="absolute bottom-0 left-0 right-0 h-0.5 bg-[#16478e] rounded-t-full"
                      transition={{ type: 'spring', stiffness: 500, damping: 40 }}
                    />
                  )}
                </button>
              );
            })}
          </div>
        </div>
      </header>

      {/* ── Content ───────────────────────────────────────────────────────── */}
      <main className="flex-1 overflow-y-auto">
        <div className="max-w-6xl mx-auto px-8 py-6">
          <AnimatePresence mode="wait">
            <motion.div
              key={activeTab}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -4 }}
              transition={{ duration: 0.18, ease: 'easeOut' }}
            >
              {activeTab === 'general'             && <PreferenceSettings />}
              {activeTab === 'recording'           && <RecordingSettings />}
              {activeTab === 'Transcriptionmodels' && (
                <TranscriptSettings
                  transcriptModelConfig={transcriptModelConfig}
                  setTranscriptModelConfig={setTranscriptModelConfig}
                />
              )}
              {activeTab === 'summaryModels'       && <SummaryModelSettings />}
            </motion.div>
          </AnimatePresence>
        </div>
      </main>
    </div>
  );
}
