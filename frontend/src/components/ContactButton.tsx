'use client';

import React, { useState } from 'react';
import { Headset, Mail, Phone } from 'lucide-react';
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
      <DialogContent className="max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] gap-0 overflow-y-auto p-0 sm:max-w-md">
        <DialogHeader className="space-y-0 px-6 pb-5 pt-6 text-left">
          <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-md border border-primary/15 bg-primary/10 text-primary">
            <Headset aria-hidden="true" className="h-5 w-5" />
          </div>
          <DialogTitle className="pr-6 text-lg font-semibold tracking-tight text-ink">
            Cần hỗ trợ thêm?
          </DialogTitle>
          <DialogDescription className="pt-3 text-sm leading-relaxed text-ink-2">
            Nếu anh chị em gặp khó khăn trong quá trình sử dụng hoặc muốn đóng góp ý kiến nhằm cải thiện sản phẩm, vui lòng liên hệ:
          </DialogDescription>
        </DialogHeader>
        <div className="px-6 pb-6 text-sm leading-relaxed">
          <div className="rounded-md border border-rule bg-paper px-4 py-4">
            <p className="text-ink">
              <strong className="font-bold">Đ/c Nguyễn Văn Cường</strong>
              <span className="mt-1 block text-ink-2">– Nhân viên Phát triển Ứng dụng AI</span>
            </p>
            <dl className="mt-4 grid grid-cols-[5.5rem_minmax(0,1fr)] items-start gap-x-3 gap-y-3 border-t border-rule pt-4">
              <dt className="flex items-center gap-1.5 text-ink-2">
                <Phone aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
                SĐT/Zalo:
              </dt>
              <dd className="select-text text-ink">
                <strong className="whitespace-nowrap font-bold tabular-nums">0901 800 274</strong>
              </dd>
              <dt className="flex items-center gap-1.5 text-ink-2">
                <Mail aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
                Email:
              </dt>
              <dd className="min-w-0 select-text break-words text-ink">
                <strong className="font-bold">{SUPPORT_EMAIL}</strong>
              </dd>
            </dl>
          </div>
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
