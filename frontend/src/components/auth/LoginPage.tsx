'use client';

import { useEffect, useState } from 'react';
import Image from 'next/image';
import { BRAND_LOGO_PATH, BRAND_NAME } from '@/constants/brand';
import { useAuth } from '@/contexts/AuthContext';

const SSO_ERROR_MESSAGES: Record<string, string> = {
  sso_denied: 'Bạn đã từ chối đăng nhập SSO.',
  invalid_callback: 'Yêu cầu đăng nhập không hợp lệ.',
  invalid_state: 'Phiên đăng nhập đã hết hạn. Vui lòng thử lại.',
  no_email: 'Tài khoản AMS chưa có email. Liên hệ đội AMS.',
  sso_timeout: 'Hết thời gian đăng nhập. Vui lòng thử lại.',
  sso_error: 'Lỗi kết nối AMS. Vui lòng thử lại sau.',
  session_revoked:
    'Phiên đăng nhập đã hết hạn hoặc tài khoản AMS đã bị thu hồi. Vui lòng đăng nhập lại.',
};

function mapLoginError(err: unknown): string {
  const message = String(err);
  const known = Object.keys(SSO_ERROR_MESSAGES).find(code => message.includes(code));
  return known ? SSO_ERROR_MESSAGES[known] : SSO_ERROR_MESSAGES.sso_error;
}

export function LoginPage() {
  const { login, sessionRevoked, clearSessionRevoked } = useAuth();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (sessionRevoked) {
      setError(SSO_ERROR_MESSAGES.session_revoked);
      clearSessionRevoked();
    }
  }, [sessionRevoked, clearSessionRevoked]);

  const handleLogin = async () => {
    setLoading(true);
    setError(null);
    try {
      await login();
    } catch (err) {
      setError(mapLoginError(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="login-page">
      <aside className="login-rail" aria-hidden="true" />
      <div className="login-body">
        <div className="login-card">
        <div className="login-logo-wrap">
          <Image
            src={BRAND_LOGO_PATH}
            alt={BRAND_NAME}
            width={160}
            height={48}
            className="login-logo object-contain"
          />
          <div className="login-title">{BRAND_NAME}</div>
          <div className="login-sub">Đăng nhập bằng tài khoản nội bộ ACT</div>
        </div>

        {error && <div className="login-error">{error}</div>}

        <button
          className={`login-btn${loading ? ' is-loading' : ''}`}
          onClick={handleLogin}
          disabled={loading}
          data-state={loading ? 'loading' : error ? 'error' : 'default'}
        >
          {loading ? 'Đang chờ đăng nhập AMS...' : 'Đăng nhập bằng AMS SSO'}
        </button>

        <p className="login-hint">
          Trình duyệt sẽ mở để bạn đăng nhập. Sau khi xong, quay lại app này.
        </p>
        </div>
      </div>
    </div>
  );
}
