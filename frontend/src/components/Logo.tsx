import React from "react";
import Image from "next/image";
import { Dialog, DialogContent, DialogTitle, DialogTrigger } from "./ui/dialog";
import { VisuallyHidden } from "./ui/visually-hidden";
import { About } from "./About";
import { BRAND_NAME, BRAND_LOGO_PATH } from "@/constants/brand";

interface LogoProps {
    isCollapsed: boolean;
}

const Logo = React.forwardRef<HTMLButtonElement, LogoProps>(({ isCollapsed }, ref) => {
  return (
    <Dialog aria-describedby={undefined}>
      {isCollapsed ? (
        <DialogTrigger asChild>
          <button ref={ref} className="flex items-center justify-start mb-2 cursor-pointer bg-transparent border-none p-0 hover:opacity-80 transition-opacity" type="button">
            <Image
              src={BRAND_LOGO_PATH}
              alt={BRAND_NAME}
              width={40}
              height={36}
              className="object-contain max-h-9 w-auto"
            />
          </button>
        </DialogTrigger>
      ) : (
        <DialogTrigger asChild>
          <button
            type="button"
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
        </DialogTrigger>
      )}
      <DialogContent>
        <VisuallyHidden>
          <DialogTitle>Về {BRAND_NAME}</DialogTitle>
        </VisuallyHidden>
        <About />
      </DialogContent>
    </Dialog>
  );
});

Logo.displayName = "Logo";

export default Logo;
