'use client';

import React, { useState } from 'react';
import { BookOpen, Play } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { GUIDE_VIDEOS, youtubeOpenUrl, type GuideVideo } from './guideVideos';

interface UserGuideButtonProps {
  isCollapsed: boolean;
}

async function openGuideVideo(item: GuideVideo): Promise<void> {
  const url = youtubeOpenUrl(item.youtubeUrl);
  if (!url) {
    toast.error('Liên kết YouTube chưa hợp lệ', {
      description: item.title,
    });
    return;
  }
  try {
    await invoke('open_external_url', { url });
  } catch {
    window.open(url, '_blank', 'noopener,noreferrer');
  }
}

const UserGuideButton = React.forwardRef<HTMLButtonElement, UserGuideButtonProps>(
  ({ isCollapsed }, ref) => {
    const [open, setOpen] = useState(false);

    const triggerClassName = isCollapsed
      ? 'flex items-center justify-center mb-2 cursor-pointer border-none transition-colors bg-transparent p-2 hover:bg-secondary rounded-md'
      : 'flex items-center justify-center w-full px-3 py-1.5 mt-1 text-sm font-medium text-muted-foreground bg-transparent hover:bg-secondary rounded-md border-none cursor-pointer transition-colors';

    const triggerButton = (
      <button ref={ref} type="button" className={triggerClassName}>
        <BookOpen className={`text-muted-foreground ${isCollapsed ? 'w-5 h-5' : 'w-4 h-4'}`} />
        {!isCollapsed && <span className="ml-2 text-sm text-muted-foreground">Hướng dẫn</span>}
      </button>
    );

    const dialogContent = (
      <DialogContent className="gap-0 overflow-hidden p-0 sm:max-w-md">
        <DialogHeader className="space-y-0 px-5 pb-4 pt-5 pr-12 text-left">
          <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-ink-2">
            YouTube
          </p>
          <DialogTitle className="mt-1 text-base font-semibold tracking-[-0.02em] text-ink">
            Hướng dẫn sử dụng
          </DialogTitle>
          <DialogDescription className="mt-1.5 text-xs text-ink-2">
            Chọn một video để xem trên YouTube.
          </DialogDescription>
        </DialogHeader>

        <div className="px-5 pb-5">
          {GUIDE_VIDEOS.length === 0 ? (
            <div className="rounded-md border border-rule px-3 py-8 text-center">
              <p className="text-sm text-ink-2">Chưa có video hướng dẫn.</p>
            </div>
          ) : (
            <div className="max-h-[60vh] overflow-y-auto rounded-md border border-rule">
              {GUIDE_VIDEOS.map((item, index) => (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => void openGuideVideo(item)}
                  className="group flex w-full items-center gap-2.5 border-b border-rule px-2.5 py-2.5 text-left last:border-b-0 hover:bg-secondary"
                >
                  <span className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-rule bg-paper font-mono text-[11px] font-medium text-ink-2">
                    {String(index + 1).padStart(2, '0')}
                  </span>
                  <div className="min-w-0 flex-1">
                    <p className="text-sm font-medium text-ink">{item.title}</p>
                    <p className="mt-0.5 line-clamp-2 text-xs text-ink-2">{item.description}</p>
                  </div>
                  <Play className="h-4 w-4 shrink-0 text-ink-2 group-hover:text-primary" />
                </button>
              ))}
            </div>
          )}
        </div>
      </DialogContent>
    );

    if (isCollapsed) {
      return (
        <Dialog open={open} onOpenChange={setOpen}>
          <Tooltip delayDuration={150}>
            <TooltipTrigger asChild>
              <DialogTrigger asChild>{triggerButton}</DialogTrigger>
            </TooltipTrigger>
            <TooltipContent
              side="right"
              sideOffset={6}
              className="duration-200 ease-out motion-reduce:duration-0"
            >
              <p>Hướng dẫn sử dụng</p>
            </TooltipContent>
          </Tooltip>
          {dialogContent}
        </Dialog>
      );
    }

    return (
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogTrigger asChild>{triggerButton}</DialogTrigger>
        {dialogContent}
      </Dialog>
    );
  },
);

UserGuideButton.displayName = 'UserGuideButton';

export default UserGuideButton;
