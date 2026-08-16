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
  const triggerClassName = isCollapsed
    ? "flex items-center justify-center mb-2 cursor-pointer border-none transition-colors bg-transparent p-2 hover:bg-secondary rounded-md"
    : "flex items-center justify-center w-full px-3 py-1.5 mt-1 text-sm font-medium text-muted-foreground bg-transparent hover:bg-secondary rounded-md border-none cursor-pointer transition-colors";

  const triggerButton = (
    <button
      ref={ref}
      type="button"
      className={triggerClassName}
    >
      <InfoIcon className={`text-muted-foreground ${isCollapsed ? "w-5 h-5" : "w-4 h-4"}`} />
      {!isCollapsed && (
        <span className="ml-2 text-sm text-muted-foreground">Giới thiệu</span>
      )}
    </button>
  );

  if (isCollapsed) {
    return (
      <Dialog aria-describedby={undefined}>
        <Tooltip delayDuration={150}>
          <TooltipTrigger asChild>
            <DialogTrigger asChild>
              {triggerButton}
            </DialogTrigger>
          </TooltipTrigger>
          <TooltipContent
            side="right"
            sideOffset={6}
            className="duration-200 ease-out motion-reduce:duration-0"
          >
            <p>Giới thiệu {BRAND_NAME}</p>
          </TooltipContent>
        </Tooltip>
        <DialogContent className="sm:max-w-md p-0 gap-0 overflow-hidden">
          <VisuallyHidden>
            <DialogTitle>Giới thiệu {BRAND_NAME}</DialogTitle>
          </VisuallyHidden>
          <About />
        </DialogContent>
      </Dialog>
    );
  }

  return (
    <Dialog aria-describedby={undefined}>
      <DialogTrigger asChild>
        {triggerButton}
      </DialogTrigger>
      <DialogContent className="sm:max-w-md p-0 gap-0 overflow-hidden">
        <VisuallyHidden>
          <DialogTitle>Giới thiệu {BRAND_NAME}</DialogTitle>
        </VisuallyHidden>
        <About />
      </DialogContent>
    </Dialog>
  );
});

Info.displayName = "Info";

export default Info;
