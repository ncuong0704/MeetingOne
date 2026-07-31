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
        className="flex items-center justify-start mb-2 cursor-pointer bg-transparent border-none p-0 hover:opacity-80 transition-opacity"
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
      className="w-full text-left border rounded-lg bg-blue-50/80 border-gray-100 font-semibold text-gray-800 mb-2 flex items-center gap-2 px-2 py-1.5 cursor-pointer hover:opacity-90 transition-opacity"
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
