'use client';

import React, { useState } from 'react';
import { Headset } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { isTauriRuntime } from '@/lib/tauriRuntime';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';

interface ContactButtonProps {
  isCollapsed: boolean;
}

const SUPPORT_EMAIL = 'cuongnv@vienthongact.vn';

async function openEmail(event: React.MouseEvent<HTMLAnchorElement>): Promise<void> {
  if (!isTauriRuntime()) return;
  event.preventDefault();
  try {
    await invoke('open_external_url', { url: `mailto:${SUPPORT_EMAIL}` });
  } catch {
    toast.error('Không mở được ứng dụng email', {
      description: `Vui lòng gửi email tới ${SUPPORT_EMAIL}.`,
    });
  }
}

const ContactButton = React.forwardRef<HTMLButtonElement, ContactButtonProps>(
  ({ isCollapsed }, ref) => {
    const [open, setOpen] = useState(false);
    const triggerClassName = isCollapsed
      ? 'flex items-center justify-center mb-2 cursor-pointer border-none transition-colors bg-transparent p-2 hover:bg-secondary rounded-md'
      : 'flex items-center justify-center w-full px-3 py-1.5 mt-1 text-sm font-medium text-muted-foreground bg-transparent hover:bg-secondary rounded-md border-none cursor-pointer transition-colors';

    const triggerButton = (
      <button ref={ref} type="button" aria-label="Liên hệ" className={triggerClassName}>
        <Headset aria-hidden="true" className={`text-muted-foreground ${isCollapsed ? 'w-5 h-5' : 'w-4 h-4'}`} />
        {!isCollapsed && <span className="ml-2 text-sm text-muted-foreground">Liên hệ</span>}
      </button>
    );

    const dialogContent = (
      <DialogContent className="gap-0 overflow-hidden p-0 sm:max-w-md">
        <DialogHeader className="px-5 pb-4 pt-5 pr-12 text-left">
          <DialogTitle className="text-base font-semibold text-ink">
            Cần hỗ trợ thêm?
          </DialogTitle>
          <DialogDescription className="pt-2 text-sm leading-relaxed text-ink-2">
            Nếu anh chị em gặp khó khăn trong quá trình sử dụng hoặc muốn đóng góp ý cải thiện sản phẩm. Vui lòng liên hệ:
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-3 px-5 pb-5 text-sm leading-relaxed">
          <p className="font-medium text-ink">
            Đ/c Nguyễn Văn Cường – NV Phát triển Ứng dụng AI.
          </p>
          <p className="text-ink-2">
            SĐT/Zalo: 0901 800 274 · Email:{' '}
            <a
              href={`mailto:${SUPPORT_EMAIL}`}
              onClick={(event) => void openEmail(event)}
              className="break-words font-semibold text-primary underline underline-offset-4 hover:text-primary-hover"
            >
              {SUPPORT_EMAIL}
            </a>
          </p>
        </div>
      </DialogContent>
    );

    return (
      <Dialog open={open} onOpenChange={setOpen}>
        {isCollapsed ? (
          <Tooltip delayDuration={150}>
            <TooltipTrigger asChild>
              <DialogTrigger asChild>{triggerButton}</DialogTrigger>
            </TooltipTrigger>
            <TooltipContent side="right" sideOffset={6}>Liên hệ</TooltipContent>
          </Tooltip>
        ) : (
          <DialogTrigger asChild>{triggerButton}</DialogTrigger>
        )}
        {dialogContent}
      </Dialog>
    );
  },
);

ContactButton.displayName = 'ContactButton';

export default ContactButton;
