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
    <div className="space-y-4">
      {isRecording && (
        <p className="text-xs text-amber-600 dark:text-amber-400">
          Không thể thay đổi mô hình khi đang ghi âm.
        </p>
      )}

      <Tabs defaultValue="live" className="w-full">
        <TabsList className="grid w-full grid-cols-2">
          <TabsTrigger value="live">Ghi âm trực tiếp</TabsTrigger>
          <TabsTrigger value="file">Nhập file</TabsTrigger>
        </TabsList>
        <TabsContent value="live" className="mt-4">
          <LiveAsrPanel config={bundle?.live} disabled={disabled} onSaved={loadConfig} />
        </TabsContent>
        <TabsContent value="file" className="mt-4">
          <FileAsrPanel config={bundle?.file} disabled={disabled} onSaved={loadConfig} />
        </TabsContent>
      </Tabs>

      <SharedTranscriptPanel config={bundle?.shared} disabled={disabled} onSaved={loadConfig} />
    </div>
  );
}
