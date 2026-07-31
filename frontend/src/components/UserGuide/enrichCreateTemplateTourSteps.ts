import type { Step } from 'react-joyride';
import {
  dispatchSettingsTourTab,
  type SettingsTourTab,
} from './settingsTourNavigation';
import {
  BUILTIN_ACT_TEMPLATE_ID,
  dispatchTemplateTourActionAsync,
  type TemplateTourAction,
} from './templateTourNavigation';

const SIDEBAR_TRANSITION_MS = 350;
const NAVIGATION_DELAY_MS = 450;
const TAB_SWITCH_DELAY_MS = 250;
const TEMPLATE_ACTION_DELAY_MS = 600;

function delay(ms: number) {
  return new Promise<void>((resolve) => {
    window.setTimeout(resolve, ms);
  });
}

interface CreateTemplateStepMeta {
  expandSidebar?: boolean;
  route?: '/' | '/settings';
  settingsTab?: SettingsTourTab;
  templateAction?: TemplateTourAction;
  templateActions?: TemplateTourAction[];
  templateActionOnNext?: TemplateTourAction;
}

interface CreateTemplateTourControls {
  expandSidebar: () => void;
  navigate: (path: '/' | '/settings') => void;
  pathname: string;
}

function getStepMeta(step: Step): CreateTemplateStepMeta | undefined {
  return step.data as CreateTemplateStepMeta | undefined;
}

export function enrichCreateTemplateTourSteps(
  steps: Step[],
  { expandSidebar, navigate, pathname }: CreateTemplateTourControls,
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

        if (meta.templateActions?.length) {
          for (const action of meta.templateActions) {
            await dispatchTemplateTourActionAsync(action);
            await delay(TEMPLATE_ACTION_DELAY_MS);
          }
        } else if (meta.templateAction) {
          await dispatchTemplateTourActionAsync(meta.templateAction);
          await delay(TEMPLATE_ACTION_DELAY_MS);
        }
      },
    };
  });
}

export { BUILTIN_ACT_TEMPLATE_ID };
