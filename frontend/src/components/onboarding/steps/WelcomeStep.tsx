import React, { useState } from 'react';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';

const workflow = [
  { step: '01', label: 'Ghi âm', hint: 'Mic và âm thanh hệ thống' },
  { step: '02', label: 'Transcript', hint: 'Nhận dạng giọng nói trên máy' },
  { step: '03', label: 'Tóm tắt AI', hint: 'Báo cáo sau khi cuộc họp kết thúc' },
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
      title="Chào mừng"
      description="AI thư ký cuộc họp — tóm tắt và phân tích nội dung sau cuộc họp, chạy trên máy của bạn."
      showBrandLogo
    >
      <ol className="flex flex-col border-t border-rule">
        {workflow.map(({ step, label, hint }) => (
          <li key={step} className="flex items-baseline gap-3 py-2.5 border-b border-rule">
            <span className="font-mono text-[11px] tabular-nums text-ink-2 w-6 shrink-0">
              {step}
            </span>
            <span className="text-sm text-ink">{label}</span>
            <span className="text-xs text-ink-2 ml-auto text-right">{hint}</span>
          </li>
        ))}
      </ol>

      <button
        type="button"
        className={`login-btn${isCompleting ? ' is-loading' : ''}`}
        onClick={handleStart}
        disabled={isCompleting}
      >
        {isCompleting ? 'Đang chuẩn bị...' : 'Bắt đầu ngay'}
      </button>
    </OnboardingContainer>
  );
}
