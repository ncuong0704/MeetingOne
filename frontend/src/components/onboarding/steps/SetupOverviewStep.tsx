import React, { useEffect, useState } from 'react';
import { Mic, ArrowRight, Clock, Brain, Info } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { motion } from 'framer-motion';

const fadeUp = (delay = 0) => ({
  initial: { opacity: 0, y: 14 },
  animate: { opacity: 1, y: 0 },
  transition: { duration: 0.38, ease: 'easeOut', delay },
});

export function SetupOverviewStep() {
  const { goNext } = useOnboarding();
  const [isMac, setIsMac] = useState(false);

  useEffect(() => {
    const checkPlatform = async () => {
      try {
        const { platform } = await import('@tauri-apps/plugin-os');
        setIsMac(platform() === 'macos');
      } catch {
        setIsMac(navigator.userAgent.includes('Mac'));
      }
    };
    checkPlatform();
  }, []);

  const steps = [
    {
      number: '01',
      icon: Mic,
      title: 'Bộ nhận dạng giọng nói tiếng Việt',
      detail: 'zipformer-vi-30m · ~30 MB',
      time: '~2 phút',
      iconBg: 'bg-blue-50',
      iconColor: 'text-blue-600',
      badgeBg: 'bg-blue-600',
      borderColor: 'border-blue-100',
      highlightBar: 'bg-blue-600',
    },
    {
      number: '02',
      icon: Brain,
      title: 'Ollama — AI tóm tắt cuộc họp',
      detail: 'Model AI tóm tắt sẽ được gợi ý',
      time: 'Tùy kết nối',
      iconBg: 'bg-violet-50',
      iconColor: 'text-violet-600',
      badgeBg: 'bg-violet-600',
      borderColor: 'border-violet-100',
      highlightBar: 'bg-violet-600',
    },
  ];

  return (
    <OnboardingContainer
      title="Chuẩn bị mô hình AI"
      description="ACT MeetingOne cần thêm mô hình AI để hoạt động đầy đủ. Chúng tôi sẽ hướng dẫn từng bước."
      step={2}
      totalSteps={isMac ? 4 : 3}
    >
      <div className="flex flex-col items-center gap-7">

        {/* ── Step cards ──────────────────────────────────────────────── */}
        <div className="w-full max-w-md space-y-3">
          {steps.map(({ number, icon: Icon, title, detail, time, iconBg, iconColor, badgeBg, borderColor, highlightBar }, i) => (
            <motion.div
              key={number}
              {...fadeUp(0.08 + i * 0.1)}
              className={`relative bg-white rounded-xl border ${borderColor} shadow-sm overflow-hidden`}
            >
              {/* Left accent bar */}
              <div className={`absolute left-0 top-0 bottom-0 w-1 ${highlightBar} rounded-l-xl`} />

              <div className="flex items-center gap-3.5 px-4 py-4 pl-5">
                {/* Icon */}
                <div className={`w-9 h-9 rounded-lg ${iconBg} flex items-center justify-center shrink-0`}>
                  <Icon className={`w-4.5 h-4.5 ${iconColor}`} />
                </div>

                {/* Text */}
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-semibold text-gray-800 leading-snug">{title}</p>
                  <p className="text-xs text-gray-500 mt-0.5">{detail}</p>
                </div>

                {/* Time badge */}
                <div className="flex items-center gap-1 shrink-0 bg-gray-50 rounded-full px-2 py-1 border border-gray-100">
                  <Clock className="w-2.5 h-2.5 text-gray-400" />
                  <span className="text-[10px] text-gray-500 font-medium">{time}</span>
                </div>
              </div>
            </motion.div>
          ))}
        </div>

        {/* ── Info note ───────────────────────────────────────────────── */}
        <motion.div
          {...fadeUp(0.28)}
          className="w-full max-w-md flex items-start gap-3 bg-gray-50 rounded-xl border border-gray-200 px-4 py-3"
        >
          <div className="w-5 h-5 rounded-full bg-gray-200 flex items-center justify-center shrink-0 mt-0.5">
            <Info className="w-3 h-3 text-gray-500" />
          </div>
          <p className="text-xs text-gray-600 leading-relaxed">
            Bạn có thể bỏ qua và cài sau trong <span className="font-semibold text-gray-800">Cài đặt</span> nếu chưa sẵn sàng ngay bây giờ.
          </p>
        </motion.div>

        {/* ── CTA ─────────────────────────────────────────────────────── */}
        <motion.div {...fadeUp(0.36)} className="w-full max-w-xs space-y-2.5">
          <Button
            onClick={goNext}
            className="w-full h-11 bg-gray-900 hover:bg-gray-700 text-white rounded-xl group transition-colors"
          >
            Bắt đầu cài đặt
            <ArrowRight className="w-4 h-4 ml-2 transition-transform group-hover:translate-x-0.5" />
          </Button>
        </motion.div>

      </div>
    </OnboardingContainer>
  );
}
