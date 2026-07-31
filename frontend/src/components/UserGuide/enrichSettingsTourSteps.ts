import type { Step } from 'react-joyride';
import {
  dispatchSettingsTourTab,
  type SettingsTourTab,
} from './settingsTourNavigation';

const SIDEBAR_TRANSITION_MS = 350;
const NAVIGATION_DELAY_MS = 450;
const TAB_SWITCH_DELAY_MS = 250;

function delay(ms: number) {
  return new Promise<void>((resolve) => {
    window.setTimeout(resolve, ms);
  });
}

interface SettingsStepMeta {
  expandSidebar?: boolean;
  route?: '/' | '/settings';
  settingsTab?: SettingsTourTab;
}

interface SettingsTourControls {
  expandSidebar: () => void;
  navigate: (path: '/' | '/settings') => void;
  pathname: string;
}

function getStepMeta(step: Step): SettingsStepMeta | undefined {
  return step.data as SettingsStepMeta | undefined;
}

export function enrichSettingsTourSteps(
  steps: Step[],
  { expandSidebar, navigate, pathname }: SettingsTourControls,
): Step[] {
  return steps.map((step) => {
    const meta = getStepMeta(step);
    if (!meta) return step;

    return {
      ...step,
      before: async () => {
        if (meta.expandSidebar) {
          expandSidebar();
          await delay(SIDEBAR_TRANSITION_MS);
        }

        if (meta.route && pathname !== meta.route) {
          navigate(meta.route);
          await delay(NAVIGATION_DELAY_MS);
        }

        if (meta.settingsTab) {
          dispatchSettingsTourTab(meta.settingsTab);
          await delay(TAB_SWITCH_DELAY_MS);
        }
      },
    };
  });
}
