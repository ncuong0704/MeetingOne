'use client';

import React, { useState } from 'react';
import { BookOpen, ChevronRight } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { useUserGuide } from '@/contexts/UserGuideContext';
import { USER_GUIDE_TOURS } from './tours';
import { TOUR_TARGETS } from './tourTargets';

interface UserGuideButtonProps {
  isCollapsed: boolean;
}

const UserGuideButton = React.forwardRef<HTMLButtonElement, UserGuideButtonProps>(
  ({ isCollapsed }, ref) => {
    const { startTour } = useUserGuide();
    const [open, setOpen] = useState(false);

    const triggerClassName = isCollapsed
      ? 'flex items-center justify-center mb-2 cursor-pointer border-none transition-colors bg-transparent p-2 hover:bg-gray-100 rounded-lg'
      : 'flex items-center justify-center w-full px-3 py-1.5 mt-1 text-sm font-medium text-gray-700 bg-gray-200 hover:bg-gray-200 rounded-lg shadow-sm border-none cursor-pointer transition-colors';

    const handleSelectTour = (tourId: string) => {
      setOpen(false);
      requestAnimationFrame(() => startTour(tourId));
    };

    const triggerButton = (
      <button
        ref={ref}
        type="button"
        className={triggerClassName}
        data-tour={TOUR_TARGETS.USER_GUIDE_BUTTON}
      >
        <BookOpen className={`text-gray-600 ${isCollapsed ? 'w-5 h-5' : 'w-4 h-4'}`} />
        {!isCollapsed && (
          <span className="ml-2 text-sm text-gray-700">Hướng dẫn</span>
        )}
      </button>
    );

    const dialogContent = (
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Hướng dẫn sử dụng</DialogTitle>
          <DialogDescription>
            Chọn một chủ đề để bắt đầu tour hướng dẫn làm quen với ứng dụng.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-2 mt-2">
          {USER_GUIDE_TOURS.map((tour) => (
            <button
              key={tour.id}
              type="button"
              onClick={() => handleSelectTour(tour.id)}
              className="flex items-center gap-3 w-full p-3 rounded-lg border border-gray-200 text-left hover:bg-gray-50 hover:border-[#16478e]/30 transition-colors group"
            >
              <div className="flex-shrink-0 flex items-center justify-center w-9 h-9 rounded-lg bg-[rgba(22,71,142,0.08)]">
                <BookOpen className="w-4 h-4 text-[#16478e]" />
              </div>
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium text-gray-900">{tour.title}</p>
                <p className="text-xs text-gray-500 mt-0.5 line-clamp-2">
                  {tour.description}
                </p>
              </div>
              <ChevronRight className="w-4 h-4 text-gray-400 group-hover:text-[#16478e] flex-shrink-0 transition-colors" />
            </button>
          ))}
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
