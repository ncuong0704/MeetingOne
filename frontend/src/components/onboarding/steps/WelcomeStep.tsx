import React, { useState } from 'react';
import { Mic, FileText, ArrowRight, Wand2, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { motion } from 'framer-motion';

const fadeUp = (delay = 0) => ({
  initial: { opacity: 0, y: 16 },
  animate: { opacity: 1, y: 0 },
  transition: { duration: 0.4, ease: 'easeOut', delay },
});

const workflow = [
  { icon: Mic,      label: 'Ghi âm',     bg: 'bg-blue-500',    ring: 'ring-blue-100' },
  { icon: FileText, label: 'Transcript',  bg: 'bg-violet-500',  ring: 'ring-violet-100' },
  { icon: Wand2,    label: 'Tóm tắt AI', bg: 'bg-emerald-500', ring: 'ring-emerald-100' },
];

export function WelcomeStep({ onComplete }: { onComplete?: () => void }) {
  const { completeOnboarding } = useOnboarding();
  const [isCompleting, setIsCompleting] = useState(false);

  const handleStart = async () => {
    setIsCompleting(true);
    try {
      await completeOnboarding();
      onComplete?.();
    } catch {
      setIsCompleting(false);
    }
  };

  return (
    <OnboardingContainer
      title="Chào mừng đến với ACT MeetingOne"
      description="AI thư ký cuộc họp — tự động tóm tắt và phân tích nội dung sau khi cuộc họp kết thúc."
      showBrandLogo
    >
      <div className="flex flex-col items-center gap-8">

        {/* ── Workflow visual ─────────────────────────────────────────── */}
        <motion.div {...fadeUp(0.05)} className="flex items-center gap-2">
          {workflow.map(({ icon: Icon, label, bg, ring }, i) => (
            <React.Fragment key={label}>
              <div className="flex flex-col items-center gap-2">
                <div className={`w-12 h-12 rounded-2xl ${bg} flex items-center justify-center shadow-sm ring-4 ${ring}`}>
                  <Icon className="w-5 h-5 text-white" />
                </div>
                <span className="text-[11px] font-medium text-gray-500 tracking-wide">{label}</span>
              </div>
              {i < workflow.length - 1 && (
                <div className="flex items-center gap-0.5 mb-6 opacity-40">
                  <div className="w-5 h-px bg-gray-400" />
                  <ArrowRight className="w-3 h-3 text-gray-400" />
                </div>
              )}
            </React.Fragment>
          ))}
        </motion.div>

        {/* ── CTA ─────────────────────────────────────────────────────── */}
        <motion.div {...fadeUp(0.38)} className="w-full max-w-xs space-y-2">
          <Button
            onClick={handleStart}
            disabled={isCompleting}
            className="w-full h-11 bg-gray-900 hover:bg-gray-700 text-white rounded-xl group transition-colors disabled:opacity-50"
          >
            {isCompleting ? (
              <><Loader2 className="w-4 h-4 mr-2 animate-spin" />Đang chuẩn bị...</>
            ) : (
              <>Bắt đầu ngay <ArrowRight className="w-4 h-4 ml-2 transition-transform group-hover:translate-x-0.5" /></>
            )}
          </Button>
        </motion.div>

      </div>
    </OnboardingContainer>
  );
}
