'use client';

import React, { useState, useEffect } from 'react';
import { ArrowLeft, Settings2, Database as DatabaseIcon, SparkleIcon, LayoutTemplate, MessageSquareText, Users, LogOut } from 'lucide-react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import { createDefaultTranscriptModelConfig, ZIPFORMER_MODEL_ID } from '@/constants/modelDefaults';
import { RecordingSettings } from '@/components/RecordingSettings';
import { TranscriptSettings } from '@/components/TranscriptSettings';
import { SummaryModelSettings } from '@/components/SummaryModelSettings';
import { TemplateSettings } from '@/components/TemplateSettings';
import { PromptSettings } from '@/components/PromptSettings';
import { SpeakerDirectorySettings } from '@/components/SpeakerDirectorySettings';
import { useConfig } from '@/contexts/ConfigContext';
import { useAuth } from '@/contexts/AuthContext';
import { cn } from '@/lib/utils';

const TABS = [
  { value: 'general',            label: 'Chung',      icon: Settings2,      desc: 'Ghi âm, lưu trữ & tùy chọn' },
  { value: 'directory',          label: 'Danh sách',  icon: Users,          desc: 'Người nói gợi ý khi gán tên' },
  { value: 'Transcriptionmodels',label: 'Nhận dạng',  icon: DatabaseIcon,   desc: 'Mô hình giọng nói' },
  { value: 'summaryModels',      label: 'Tóm tắt AI', icon: SparkleIcon,    desc: 'Mô hình tóm tắt' },
  { value: 'templates',          label: 'Mẫu',        icon: LayoutTemplate,      desc: 'Tùy chỉnh mẫu tóm tắt' },
  { value: 'promptSettings',     label: 'Prompt AI',  icon: MessageSquareText,   desc: 'Tùy chỉnh prompt gửi AI' },
] as const;

type TabValue = typeof TABS[number]['value'];

function initialsFromName(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return `${parts[0][0]}${parts[parts.length - 1][0]}`.toUpperCase();
}

export default function SettingsPage() {
  const router = useRouter();
  const { user, logout, authRequired } = useAuth();
  const { transcriptModelConfig, setTranscriptModelConfig } = useConfig();
  const [activeTab, setActiveTab] = useState<TabValue>('general');

  useEffect(() => {
    const loadTranscriptConfig = async () => {
      try {
        const config = await invoke('api_get_transcript_config') as { live?: { model?: string } };
        if (config?.live?.model) {
          setTranscriptModelConfig({
            ...createDefaultTranscriptModelConfig(),
            model: config.live.model || ZIPFORMER_MODEL_ID,
            apiKey: null,
          });
        }
      } catch (error) {
        console.error('Failed to load transcript config:', error);
      }
    };
    loadTranscriptConfig();
  }, [setTranscriptModelConfig]);

  return (
    <div className="h-screen bg-paper flex flex-col overflow-hidden">
      <header className="shrink-0 bg-paper-2 border-b border-rule">
        <div className="flex items-center gap-3 px-5 h-12">
          <button
            onClick={() => router.back()}
            className="flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors"
          >
            <ArrowLeft className="w-3.5 h-3.5" />
            Quay lại
          </button>
          <span className="text-rule">/</span>
          <span className="text-lg font-semibold text-foreground tracking-tight">Cài đặt</span>
        </div>
      </header>

      <div className="flex flex-1 min-h-0">
        <nav className="w-52 shrink-0 border-r border-rule bg-paper-2 flex flex-col min-h-0">
          <div className="flex-1 overflow-y-auto py-3 px-2">
            {TABS.map(({ value, label, desc, icon: Icon }) => {
              const isActive = activeTab === value;
              return (
                <button
                  key={value}
                  onClick={() => setActiveTab(value)}
                  className={cn(
                    'w-full flex items-start gap-2.5 rounded-r-md px-3 py-2.5 text-left border-l-2 transition-colors duration-150',
                    isActive
                      ? 'border-primary bg-paper text-foreground'
                      : 'border-transparent text-muted-foreground hover:bg-paper hover:text-foreground'
                  )}
                >
                  <Icon className={cn('w-3.5 h-3.5 mt-0.5 shrink-0', isActive ? 'text-primary' : 'text-muted-foreground')} />
                  <span className="min-w-0">
                    <span className={cn('block text-sm', isActive ? 'font-medium' : 'font-normal')}>{label}</span>
                    <span className="block text-[11px] leading-snug text-ink-2 mt-0.5">{desc}</span>
                  </span>
                </button>
              );
            })}
          </div>

          {authRequired && user && (
            <div className="shrink-0 border-t border-rule px-3 py-3">
              <div className="flex items-center gap-2.5 min-w-0">
                <span
                  aria-hidden
                  className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-primary/10 text-[11px] font-medium font-mono text-primary"
                >
                  {initialsFromName(user.fullName)}
                </span>
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-medium text-ink truncate" title={user.fullName}>
                    {user.fullName}
                  </p>
                  <p className="text-[11px] leading-snug font-mono text-ink-2 truncate" title={user.email}>
                    {user.email}
                  </p>
                </div>
              </div>
              <button
                type="button"
                onClick={() => { void logout(); }}
                className="mt-2.5 inline-flex h-8 w-full items-center justify-center gap-1.5 rounded-md border border-rule bg-paper px-2.5 text-xs font-medium text-ink-2 hover:border-destructive/40 hover:bg-paper-2 hover:text-destructive transition-colors duration-150"
              >
                <LogOut className="w-3.5 h-3.5" />
                Đăng xuất
              </button>
            </div>
          )}
        </nav>

        <main className="flex-1 overflow-y-auto bg-paper">
          <div className="max-w-3xl px-8 py-6">
            <AnimatePresence mode="wait">
              <motion.div
                key={activeTab}
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                transition={{ duration: 0.15, ease: [0.16, 1, 0.3, 1] }}
              >
                {activeTab === 'general'             && <RecordingSettings />}
                {activeTab === 'directory'           && <SpeakerDirectorySettings />}
                {activeTab === 'Transcriptionmodels' && (
                  <TranscriptSettings
                    transcriptModelConfig={transcriptModelConfig}
                    setTranscriptModelConfig={setTranscriptModelConfig}
                  />
                )}
                {activeTab === 'summaryModels'       && <SummaryModelSettings />}
                {activeTab === 'templates'           && <TemplateSettings />}
                {activeTab === 'promptSettings'      && <PromptSettings />}
              </motion.div>
            </AnimatePresence>
          </div>
        </main>
      </div>
    </div>
  );
}
