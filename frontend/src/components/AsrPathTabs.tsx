'use client';

import { useCallback, useEffect, useState } from 'react';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { TranscriptConfigAPI, TranscriptConfigBundle } from '@/lib/asr';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import LiveAsrPanel from './LiveAsrPanel';
import FileAsrPanel from './FileAsrPanel';
import SharedTranscriptPanel from './SharedTranscriptPanel';

export default function AsrPathTabs() {
  const { isRecording } = useRecordingState();
  const [bundle, setBundle] = useState<TranscriptConfigBundle | null>(null);

  const loadConfig = useCallback(async () => {
    try {
      const data = await TranscriptConfigAPI.get();
      setBundle(data);
    } catch (e) {
      console.error('Failed to load transcript config bundle:', e);
    }
  }, []);

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  const disabled = isRecording;

  return (
    <div className="space-y-6">
      <section>
        <h2 className="text-sm font-semibold text-ink tracking-tight">Mô hình</h2>
        <p className="text-xs text-ink-2 mt-0.5 mb-2">
          {isRecording
            ? 'Không thể thay đổi mô hình khi đang ghi âm.'
            : 'Ghi trực tiếp hoặc nhập file. CAPU chỉ chạy sau khi kết thúc cuộc họp.'}
        </p>
        <div className="app-surface overflow-hidden">
          <Tabs defaultValue="live" className="w-full">
            <div className="px-4 pt-3">
              <TabsList className="grid h-8 w-full grid-cols-2 rounded-md border border-rule bg-paper p-0.5 text-ink-2">
                <TabsTrigger
                  value="live"
                  className="h-7 rounded-md text-xs shadow-none data-[state=active]:bg-primary data-[state=active]:text-primary-foreground data-[state=active]:shadow-none"
                >
                  Ghi trực tiếp
                </TabsTrigger>
                <TabsTrigger
                  value="file"
                  className="h-7 rounded-md text-xs shadow-none data-[state=active]:bg-primary data-[state=active]:text-primary-foreground data-[state=active]:shadow-none"
                >
                  Nhập file
                </TabsTrigger>
              </TabsList>
            </div>
            <div className="px-4 py-3">
              <TabsContent value="live" className="mt-0">
                <LiveAsrPanel config={bundle?.live} disabled={disabled} onSaved={loadConfig} />
              </TabsContent>
              <TabsContent value="file" className="mt-0">
                <FileAsrPanel config={bundle?.file} disabled={disabled} onSaved={loadConfig} />
              </TabsContent>
            </div>
          </Tabs>
        </div>
      </section>

      <SharedTranscriptPanel config={bundle?.shared} disabled={disabled} onSaved={loadConfig} />
    </div>
  );
}
