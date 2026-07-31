export const SETTINGS_TOUR_TAB_EVENT = 'user-guide:settings-tab';

export type SettingsTourTab = 'general' | 'summaryModels' | 'templates' | 'promptSettings';

export function dispatchSettingsTourTab(tab: SettingsTourTab) {
  window.dispatchEvent(
    new CustomEvent(SETTINGS_TOUR_TAB_EVENT, { detail: { tab } }),
  );
}
