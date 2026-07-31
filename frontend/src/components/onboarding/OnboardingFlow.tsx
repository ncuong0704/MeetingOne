import React from 'react';
import { WelcomeStep } from './steps';

interface OnboardingFlowProps {
  onComplete: () => void;
}

export function OnboardingFlow({ onComplete }: OnboardingFlowProps) {
  return (
    <div className="onboarding-flow">
      <WelcomeStep onComplete={onComplete} />
    </div>
  );
}
