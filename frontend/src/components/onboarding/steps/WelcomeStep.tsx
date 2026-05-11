import React from 'react';
import { Lock, Sparkles, Cpu, Mic, FileText, ArrowRight, Wand2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { motion } from 'framer-motion';

const fadeUp = (delay = 0) => ({
  initial: { opacity: 0, y: 16 },
  animate: { opacity: 1, y: 0 },
  transition: { duration: 0.4, ease: 'easeOut', delay },
});

const features = [
  {
    icon: Lock,
    title: 'Hoàn toàn riêng tư',
    desc: 'Dữ liệu không rời khỏi thiết bị — không cloud, không theo dõi.',
    bg: 'bg-blue-50',
    ring: 'ring-blue-100',
    color: 'text-blue-600',
  },
  {
    icon: Sparkles,
    title: 'AI tóm tắt thông minh',
    desc: 'Tự động tóm tắt và phân tích nội dung sau khi cuộc họp kết thúc.',
    bg: 'bg-violet-50',
    ring: 'ring-violet-100',
    color: 'text-violet-600',
  },
  {
    icon: Cpu,
    title: 'Chạy offline hoàn toàn',
    desc: 'Mô hình AI chạy trực tiếp trên máy, không cần kết nối internet.',
    bg: 'bg-emerald-50',
    ring: 'ring-emerald-100',
    color: 'text-emerald-600',
  },
];

const workflow = [
  { icon: Mic,      label: 'Ghi âm',      bg: 'bg-gray-900' },
  { icon: FileText, label: 'Transcript',   bg: 'bg-gray-700' },
  { icon: Wand2,    label: 'Tóm tắt AI',  bg: 'bg-gray-500' },
];

export function WelcomeStep() {
  const { goNext } = useOnboarding();

  return (
    <OnboardingContainer
      title="Chào mừng đến với ACT MeetingOne"
      description="Ghi âm · Chuyển văn bản · Tóm tắt — tất cả trên thiết bị của bạn."
      step={1}
      hideProgress={true}
    >
      <div className="flex flex-col items-center gap-8">

        {/* ── Workflow visual ─────────────────────────────────────────── */}
        <motion.div {...fadeUp(0.05)} className="flex items-center gap-2">
          {workflow.map(({ icon: Icon, label, bg }, i) => (
            <React.Fragment key={label}>
              <div className="flex flex-col items-center gap-2">
                <div className={`w-12 h-12 rounded-2xl ${bg} flex items-center justify-center shadow-sm ring-4 ring-gray-100`}>
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

        {/* ── Feature cards ───────────────────────────────────────────── */}
        <div className="w-full max-w-md space-y-2.5">
          {features.map(({ icon: Icon, title, desc, bg, ring, color }, i) => (
            <motion.div
              key={title}
              {...fadeUp(0.14 + i * 0.08)}
              className="flex items-start gap-3.5 bg-white rounded-xl border border-gray-100 px-4 py-3.5 shadow-sm"
            >
              <div className={`w-8 h-8 rounded-lg ${bg} ring-4 ${ring} flex items-center justify-center shrink-0 mt-0.5`}>
                <Icon className={`w-4 h-4 ${color}`} />
              </div>
              <div className="min-w-0">
                <p className="text-sm font-semibold text-gray-800 leading-snug">{title}</p>
                <p className="text-xs text-gray-500 mt-0.5 leading-relaxed">{desc}</p>
              </div>
            </motion.div>
          ))}
        </div>

        {/* ── CTA ─────────────────────────────────────────────────────── */}
        <motion.div {...fadeUp(0.38)} className="w-full max-w-xs space-y-2">
          <Button
            onClick={goNext}
            className="w-full h-11 bg-gray-900 hover:bg-gray-700 text-white rounded-xl group transition-colors"
          >
            Bắt đầu ngay
            <ArrowRight className="w-4 h-4 ml-2 transition-transform group-hover:translate-x-0.5" />
          </Button>
          <p className="text-xs text-center text-gray-400">Mất chưa đến 3 phút để thiết lập</p>
        </motion.div>

      </div>
    </OnboardingContainer>
  );
}
