import type { Step } from 'react-joyride';
import { TOUR_TARGETS, tourTargetSelector } from './tourTargets';

const SIDEBAR_TRANSITION_MS = 350;

function delay(ms: number) {
  return new Promise<void>((resolve) => {
    window.setTimeout(resolve, ms);
  });
}

interface SidebarTourControls {
  expandSidebar: () => void;
  collapseSidebar: () => void;
}

export function enrichGettingStartedSteps(
  steps: Step[],
  { expandSidebar, collapseSidebar }: SidebarTourControls,
): Step[] {
  return steps.map((step) => {
    const target = typeof step.target === 'string' ? step.target : '';

    if (target === tourTargetSelector(TOUR_TARGETS.SIDEBAR_TOGGLE)) {
      return {
        ...step,
        before: async () => {
          collapseSidebar();
          await delay(SIDEBAR_TRANSITION_MS);
        },
      };
    }

    if (target === tourTargetSelector(TOUR_TARGETS.SIDEBAR)) {
      return {
        ...step,
        before: async () => {
          expandSidebar();
          await delay(SIDEBAR_TRANSITION_MS);
        },
      };
    }

    return step;
  });
}
