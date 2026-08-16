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
    <div className="login-page">
      <aside className="login-rail" aria-hidden="true" />
      <div className="login-body">
        <div className={cn('login-card', className)}>
          {showBrandLogo && (
            <div className="login-logo-wrap">
              <Image
                src={BRAND_LOGO_PATH}
                alt={BRAND_NAME}
                width={160}
                height={48}
                priority
                className="login-logo object-contain"
              />
            </div>
          )}
          <p className="font-mono text-[10px] uppercase tracking-[0.12em] text-ink-2">
            Thư ký cuộc họp
          </p>
          <h1 className="login-title">{title}</h1>
          {description && <p className="login-sub">{description}</p>}
          {children}
        </div>
      </div>
    </div>
  );
}
