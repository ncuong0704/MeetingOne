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
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Hướng dẫn sử dụng</DialogTitle>
          <DialogDescription>
            Chọn một video để xem hướng dẫn trên YouTube.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-2 mt-2 max-h-[60vh] overflow-y-auto">
          {GUIDE_VIDEOS.length === 0 ? (
            <p className="text-sm text-gray-500 py-4 text-center">
              Chưa có video hướng dẫn.
            </p>
          ) : (
            GUIDE_VIDEOS.map((item) => (
              <button
                key={item.id}
                type="button"
                onClick={() => void openGuideVideo(item)}
                className="flex items-center gap-3 w-full p-3 rounded-md border border-rule text-left hover:bg-secondary hover:border-primary/30 transition-colors group"
              >
                <div className="flex-shrink-0 flex items-center justify-center w-9 h-9 rounded-md bg-primary/10">
                  <Play className="w-4 h-4 text-primary" />
                </div>
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium text-foreground">{item.title}</p>
                  <p className="text-xs text-muted-foreground mt-0.5 line-clamp-2">{item.description}</p>
                </div>
              </button>
            ))
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
