import React from 'react';
import { Upload } from 'lucide-react';
import { getAudioFormatsDisplayList } from '@/constants/audioFormats';

interface ImportDropOverlayProps {
  visible: boolean;
}

export function ImportDropOverlay({ visible }: ImportDropOverlayProps) {
  if (!visible) return null;

  return (
    <div
      className="fixed inset-0 z-[var(--z-modal)] app-modal-overlay
                 flex items-center justify-center pointer-events-none
                 transition-opacity duration-[var(--dur-short)] ease-[var(--ease-out)]"
    >
      <div className="border border-dashed border-rule rounded-md
                      p-12 text-center bg-paper-2
                      transform scale-100 transition-transform">
        <Upload className="h-16 w-16 text-primary mx-auto mb-4" />
        <p className="text-xl font-medium text-foreground">Thả file âm thanh để nhập</p>
        <p className="text-sm text-muted-foreground mt-2">{getAudioFormatsDisplayList()}</p>
      </div>
    </div>
  );
}
