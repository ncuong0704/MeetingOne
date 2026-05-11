import React from "react";
import { Info as InfoIcon } from "lucide-react";
import { Dialog, DialogContent, DialogTitle, DialogTrigger } from "./ui/dialog";
import { VisuallyHidden } from "./ui/visually-hidden";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { About } from "./About";
import { BRAND_NAME } from "@/constants/brand";

interface InfoProps {
    isCollapsed: boolean;
}

const Info = React.forwardRef<HTMLButtonElement, InfoProps>(({ isCollapsed }, ref) => {
  const button = (
    <Dialog aria-describedby={undefined}>
      <DialogTrigger asChild>
        <button
          ref={ref}
          className={`flex items-center justify-center mb-2 cursor-pointer border-none transition-colors ${
            isCollapsed
              ? "bg-transparent p-2 hover:bg-gray-100 rounded-lg"
              : "w-full px-3 py-1.5 mt-1 text-sm font-medium text-gray-700 bg-gray-200 hover:bg-gray-200 rounded-lg shadow-sm"
          }`}
        >
          <InfoIcon className={`text-gray-600 ${isCollapsed ? "w-5 h-5" : "w-4 h-4"}`} />
          {!isCollapsed && (
            <span className="ml-2 text-sm text-gray-700">Giới thiệu</span>
          )}
        </button>
      </DialogTrigger>
      <DialogContent>
        <VisuallyHidden>
          <DialogTitle>Giới thiệu {BRAND_NAME}</DialogTitle>
        </VisuallyHidden>
        <About />
      </DialogContent>
    </Dialog>
  );

  if (isCollapsed) {
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <span>{button}</span>
        </TooltipTrigger>
        <TooltipContent side="right">
          <p>Giới thiệu {BRAND_NAME}</p>
        </TooltipContent>
      </Tooltip>
    );
  }

  return button;
});

Info.displayName = "Info";

export default Info;
