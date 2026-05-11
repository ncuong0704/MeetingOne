'use client';

import { motion } from 'framer-motion';
import { Sparkles, AlertCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';

interface EmptyStateSummaryProps {
  onGenerate: () => void;
  hasModel: boolean;
  isGenerating?: boolean;
}

export function EmptyStateSummary({ onGenerate, hasModel, isGenerating = false }: EmptyStateSummaryProps) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.35, ease: 'easeOut' }}
      className="flex flex-col items-center justify-center h-full px-8 py-12 text-center"
    >
      {/* Icon */}
      <div className="relative mb-5">
        <div className="w-16 h-16 rounded-2xl bg-gray-100 flex items-center justify-center">
          <Sparkles className="w-7 h-7 text-gray-400" />
        </div>
        {/* Pulse ring for "ready to generate" state */}
        {hasModel && !isGenerating && (
          <span className="absolute -top-1 -right-1 w-4 h-4 rounded-full bg-[#16478e] flex items-center justify-center">
            <span className="w-2 h-2 rounded-full bg-white" />
          </span>
        )}
      </div>

      <h3 className="text-base font-semibold text-gray-800 mb-1.5">
        Chưa có báo cáo tóm tắt
      </h3>
      <p className="text-sm text-gray-500 mb-6 max-w-xs leading-relaxed">
        Nhấn nút bên dưới để AI tự động tóm tắt nội dung, điểm chính và công việc cần làm.
      </p>

      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <div>
              <Button
                onClick={onGenerate}
                disabled={!hasModel || isGenerating}
                className="gap-2 px-5 h-10 rounded-lg text-white bg-gradient-to-r from-[#0f2f63] via-[#16478e] to-[#2b7de0] shadow-sm ring-1 ring-inset ring-white/15 hover:shadow-md hover:from-[#123a78] hover:via-[#1a55a8] hover:to-[#3b8cf3] active:shadow-sm active:from-[#0d2a58] active:via-[#143f80] active:to-[#2572cc] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#16478e]/35 focus-visible:ring-offset-2 disabled:shadow-none disabled:opacity-60"
              >
                <Sparkles className="w-4 h-4" />
                {isGenerating ? 'Đang tạo...' : 'Tạo tóm tắt AI'}
              </Button>
            </div>
          </TooltipTrigger>
          {!hasModel && (
            <TooltipContent>
              <p>Vui lòng chọn mô hình trong Cài đặt trước</p>
            </TooltipContent>
          )}
        </Tooltip>
      </TooltipProvider>

      {!hasModel && (
        <div className="flex items-center gap-1.5 mt-4 text-xs text-amber-600 bg-amber-50 border border-amber-100 rounded-lg px-3 py-2">
          <AlertCircle className="w-3.5 h-3.5 shrink-0" />
          <span>Chọn mô hình AI trong Cài đặt trước khi tạo</span>
        </div>
      )}
    </motion.div>
  );
}
