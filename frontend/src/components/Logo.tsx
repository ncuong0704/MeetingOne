'use client';

import React from "react";
import Image from "next/image";
import { useRouter } from "next/navigation";
import { BRAND_NAME, BRAND_LOGO_PATH } from "@/constants/brand";

interface LogoProps {
  isCollapsed: boolean;
}

const Logo = React.forwardRef<HTMLButtonElement, LogoProps>(({ isCollapsed }, ref) => {
  const router = useRouter();

  const handleGoHome = () => {
    router.push("/");
  };

  if (isCollapsed) {
    return (
      <button
        ref={ref}
        type="button"
        onClick={handleGoHome}
        title="Trang chủ"
        aria-label="Trang chủ"
        className="flex items-center justify-start mb-2 cursor-pointer bg-transparent border-none p-0 hover:opacity-80 transition-opacity duration-[var(--dur-micro)] ease-[var(--ease-out)]"
      >
        <Image
          src={BRAND_LOGO_PATH}
          alt={BRAND_NAME}
          width={40}
          height={36}
          className="object-contain max-h-9 w-auto"
        />
      </button>
    );
  }

  return (
    <button
      ref={ref}
      type="button"
      onClick={handleGoHome}
      title="Trang chủ"
      aria-label="Trang chủ"
      className="w-full text-left bg-transparent border-none font-semibold text-ink mb-1 flex items-center gap-2 px-1 py-1 cursor-pointer hover:opacity-80 transition-opacity duration-[var(--dur-micro)] ease-[var(--ease-out)]"
    >
      <Image
        src={BRAND_LOGO_PATH}
        alt=""
        width={140}
        height={40}
        className="object-contain flex-1 min-w-0 h-9 max-h-9"
      />
    </button>
  );
});

Logo.displayName = "Logo";

export default Logo;
