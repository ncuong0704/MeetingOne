declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

/** True when running inside the Tauri desktop shell (not plain browser dev). */
export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && !!window.__TAURI_INTERNALS__;
}

const BROWSER_ONBOARDING_KEY = 'onboarding_completed';

export function getBrowserOnboardingCompleted(): boolean {
  if (typeof window === 'undefined') return false;
  return localStorage.getItem(BROWSER_ONBOARDING_KEY) === 'true';
}

export function setBrowserOnboardingCompleted(): void {
  if (typeof window === 'undefined') return;
  localStorage.setItem(BROWSER_ONBOARDING_KEY, 'true');
}

export function clearBrowserOnboardingCompleted(): void {
  if (typeof window === 'undefined') return;
  localStorage.removeItem(BROWSER_ONBOARDING_KEY);
}
