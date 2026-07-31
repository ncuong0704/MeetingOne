'use client';

import React, { createContext, useCallback, useContext, useState } from 'react';
import { usePathname, useRouter } from 'next/navigation';
import type { Step } from 'react-joyride';
import { toast } from 'sonner';
import { USER_GUIDE_TOURS } from '@/components/UserGuide/tours';
import { enrichGettingStartedSteps } from '@/components/UserGuide/enrichGettingStartedSteps';
import { enrichSettingsTourSteps } from '@/components/UserGuide/enrichSettingsTourSteps';
import { enrichCreateTemplateTourSteps } from '@/components/UserGuide/enrichCreateTemplateTourSteps';
import { UserGuideJoyride } from '@/components/UserGuide/UserGuideJoyride';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';

const NAVIGATION_DELAY_MS = 400;

function delay(ms: number) {
  return new Promise<void>((resolve) => {
    window.setTimeout(resolve, ms);
  });
}

interface UserGuideContextType {
  run: boolean;
  steps: Step[];
  activeTourId: string | null;
  startTour: (tourId: string) => void;
  stopTour: () => void;
}

const UserGuideContext = createContext<UserGuideContextType | undefined>(undefined);

export function UserGuideProvider({ children }: { children: React.ReactNode }) {
  const router = useRouter();
  const pathname = usePathname();
  const { expandSidebar, collapseSidebar } = useSidebar();

  const [run, setRun] = useState(false);
  const [steps, setSteps] = useState<Step[]>([]);
  const [activeTourId, setActiveTourId] = useState<string | null>(null);

  const stopTour = useCallback(() => {
    setRun(false);
    setActiveTourId(null);
    setSteps([]);
  }, []);

  const prepareTour = useCallback(
    async (tourId: string) => {
      const tour = USER_GUIDE_TOURS.find((item) => item.id === tourId);
      if (!tour) return;

      if (tour.requiresHomePage && pathname !== '/') {
        router.push('/');
        await delay(NAVIGATION_DELAY_MS);
      }

      if (tourId === 'settings' || tourId === 'create-template') {
        expandSidebar();
        await delay(350);
      }

      await delay(50);
    },
    [expandSidebar, pathname, router],
  );

  const resolveTourSteps = useCallback(
    (tourId: string, tourSteps: Step[]) => {
      if (tourId === 'getting-started') {
        return enrichGettingStartedSteps(tourSteps, { expandSidebar, collapseSidebar });
      }
      if (tourId === 'settings') {
        return enrichSettingsTourSteps(tourSteps, {
          expandSidebar,
          navigate: (path) => router.push(path),
          pathname,
        });
      }
      if (tourId === 'create-template') {
        return enrichCreateTemplateTourSteps(tourSteps, {
          expandSidebar,
          navigate: (path) => router.push(path),
          pathname,
        });
      }
      return tourSteps;
    },
    [collapseSidebar, expandSidebar, pathname, router],
  );

  const startTour = useCallback(
    (tourId: string) => {
      const tour = USER_GUIDE_TOURS.find((item) => item.id === tourId);
      if (!tour) return;

      if (tour.steps.length === 0) {
        toast.info('Hướng dẫn đang được cập nhật', {
          description: `"${tour.title}" sẽ sớm có sẵn.`,
        });
        return;
      }

      void (async () => {
        await prepareTour(tourId);
        setSteps(resolveTourSteps(tourId, tour.steps));
        setActiveTourId(tourId);
        setRun(true);
      })();
    },
    [prepareTour, resolveTourSteps],
  );

  return (
    <UserGuideContext.Provider
      value={{ run, steps, activeTourId, startTour, stopTour }}
    >
      {children}
      <UserGuideJoyride />
    </UserGuideContext.Provider>
  );
}

export function useUserGuide() {
  const context = useContext(UserGuideContext);
  if (!context) {
    throw new Error('useUserGuide must be used within UserGuideProvider');
  }
  return context;
}
