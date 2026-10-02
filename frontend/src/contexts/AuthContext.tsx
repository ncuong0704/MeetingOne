'use client';

import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { clearBrowserOnboardingCompleted, isTauriRuntime } from '@/lib/tauriRuntime';

export interface SsoUser {
  email: string;
  fullName: string;
}

interface AuthContextValue {
  user: SsoUser | null;
  loading: boolean;
  authRequired: boolean;
  login: () => Promise<void>;
  logout: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<SsoUser | null>(null);
  // SSR và lần render đầu trên client phải giống nhau — isTauriRuntime() chỉ đúng sau mount.
  const [authRequired, setAuthRequired] = useState(false);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const required = isTauriRuntime();
    setAuthRequired(required);

    if (!required) {
      setLoading(false);
      return;
    }

    invoke<SsoUser | null>('get_sso_session')
      .then((sessionUser) => setUser(sessionUser))
      .catch(() => setUser(null))
      .finally(() => setLoading(false));
  }, []);

  const login = useCallback(async () => {
    const session = await invoke<SsoUser>('sso_login');
    setUser(session);
  }, []);

  const logout = useCallback(async () => {
    if (isTauriRuntime()) {
      await invoke('sso_logout');
    } else {
      clearBrowserOnboardingCompleted();
      await invoke('sso_logout').catch(() => undefined);
    }
    setUser(null);
  }, []);

  const value = useMemo(
    () => ({ user, loading, authRequired, login, logout }),
    [user, loading, authRequired, login, logout],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth() {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within AuthProvider');
  }
  return context;
}
