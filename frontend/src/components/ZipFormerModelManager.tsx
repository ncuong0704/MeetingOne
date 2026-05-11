'use client';

import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { ModelStatus, ZipFormerAPI } from '../lib/zipformer';

export default function ZipFormerModelManager() {
  const [status, setStatus] = useState<ModelStatus>({ type: 'NotLoaded' });
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    ZipFormerAPI.init().catch(console.error);
    refreshStatus();

    const unlistenProgress = listen<{ progress: number }>(
      'zipformer-model-download-progress',
      (event) => {
        setProgress(event.payload.progress);
        setStatus({ type: 'Downloading', value: event.payload.progress });
      }
    );

    const unlistenComplete = listen('zipformer-model-download-complete', () => {
      setDownloading(false);
      setProgress(100);
      setError(null);
      ZipFormerAPI.loadModel()
        .then(() => setStatus({ type: 'Ready' }))
        .catch((e) => setError(String(e)));
    });

    const unlistenError = listen<{ error: string }>(
      'zipformer-model-download-error',
      (event) => {
        setDownloading(false);
        setError(event.payload.error);
        setStatus({ type: 'Error', value: event.payload.error });
      }
    );

    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenComplete.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, []);

  const refreshStatus = async () => {
    try {
      const s = await ZipFormerAPI.getModelStatus();
      setStatus(s);
    } catch (e) {
      console.error('Failed to get ZipFormer status:', e);
    }
  };

  const handleDownload = async () => {
    setError(null);
    setDownloading(true);
    setProgress(0);
    try {
      await ZipFormerAPI.downloadModel();
    } catch (e) {
      setDownloading(false);
      setError(String(e));
    }
  };

  const isReady = status.type === 'Ready';
  const isDownloading = status.type === 'Downloading' || downloading;

  return (
    <div className="space-y-3 p-4 rounded-lg border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800">
      <div className="flex items-center gap-2">
        <span className="text-lg">🇻🇳</span>
        <div>
          <p className="font-medium text-sm text-gray-900 dark:text-white">
            Vietnamese Speech Recognition
          </p>
          <p className="text-xs text-gray-500 dark:text-gray-400">
            ZipFormer-30M · ~30 MB · RNNT
          </p>
        </div>
        <div className="ml-auto">
          {isReady && (
            <span className="inline-flex items-center gap-1 text-xs text-green-600 dark:text-green-400 font-medium">
              ✓ Sẵn sàng
            </span>
          )}
          {status.type === 'NotLoaded' && !isDownloading && (
            <span className="text-xs text-gray-400">Chưa tải</span>
          )}
        </div>
      </div>

      {isDownloading && (
        <div className="space-y-1">
          <div className="flex justify-between text-xs text-gray-500">
            <span>Đang tải xuống...</span>
            <span>{progress}%</span>
          </div>
          <div className="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-1.5">
            <div
              className="bg-blue-500 h-1.5 rounded-full transition-all duration-300"
              style={{ width: `${progress}%` }}
            />
          </div>
        </div>
      )}

      {error && (
        <p className="text-xs text-red-500 dark:text-red-400">{error}</p>
      )}

      {!isReady && !isDownloading && (
        <button
          onClick={handleDownload}
          className="w-full py-1.5 px-3 text-sm rounded-md bg-blue-600 hover:bg-blue-700 text-white font-medium transition-colors"
        >
          Tải xuống model (~30 MB)
        </button>
      )}
    </div>
  );
}
