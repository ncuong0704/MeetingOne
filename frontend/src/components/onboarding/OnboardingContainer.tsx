import React from 'react';
import Image from 'next/image';
import { cn } from '@/lib/utils';
import { BRAND_NAME, BRAND_LOGO_PATH } from '@/constants/brand';
import type { OnboardingContainerProps } from '@/types/onboarding';

export function OnboardingContainer({
  title,
  description,
  children,
  showBrandLogo = false,
  className,
}: OnboardingContainerProps) {
  return (
    <div className="fixed inset-0 bg-gray-50 flex items-center justify-center z-50 overflow-hidden">
      <div className={cn('w-full max-w-2xl h-full max-h-screen flex flex-col px-6 py-10', className)}>
        <div className="mb-4 text-center space-y-3 flex-shrink-0">
          {showBrandLogo && (
            <div className="flex justify-center animate-fade-in-up">
              <Image
                src={BRAND_LOGO_PATH}
                alt={BRAND_NAME}
                width={240}
                height={96}
                priority
                className="w-56 sm:w-60 h-auto object-contain"
              />
            </div>
          )}
          <h1 className="text-4xl font-semibold text-gray-900 animate-fade-in-up">{title}</h1>
          {description && (
            <p className="text-base text-gray-600 max-w-md mx-auto animate-fade-in-up delay-75">
              {description}
            </p>
          )}
        </div>

        <div className="flex-1 overflow-y-auto pr-2">
          <div className="space-y-6">{children}</div>
        </div>
      </div>
    </div>
  );
}
