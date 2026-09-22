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
  callback_port_busy:
    'Cổng đăng nhập (34517) đang bị chiếm — có thể một app ACT MeetingOne khác đang mở. Đóng app khác rồi thử lại.',
  browser_open_failed: 'Không mở được trình duyệt. Kiểm tra trình duyệt mặc định của máy rồi thử lại.',
  ams_unreachable:
    'Không kết nối được máy chủ AMS (ams.vienthongact.vn). Kiểm tra kết nối mạng nội bộ / VPN rồi thử lại.',
  ams_stale_token:
    'AMS đang trả token của phiên đăng nhập cũ đã hết hạn. Mở ams.vienthongact.vn trên trình duyệt, đăng xuất, rồi đăng nhập lại.',
  ams_token_error: 'Máy chủ AMS từ chối bước xác thực token. Liên hệ đội AMS kèm chi tiết bên dưới.',
  ams_userinfo_error: 'Máy chủ AMS trả lỗi thông tin tài khoản. Liên hệ đội AMS kèm chi tiết bên dưới.',
  session_revoked:
    'Phiên đăng nhập đã hết hạn hoặc tài khoản AMS đã bị thu hồi. Vui lòng đăng nhập lại.',
};

type LoginError = { message: string; detail?: string };

function mapLoginError(err: unknown): LoginError {
  const raw = String(err);
  const separator = raw.indexOf(':');
  const code = (separator === -1 ? raw : raw.slice(0, separator)).trim();
  const detail = (separator === -1 ? '' : raw.slice(separator + 1)).trim() || undefined;
  const message = SSO_ERROR_MESSAGES[code] ?? SSO_ERROR_MESSAGES.sso_error;
  return { message, detail };
}

export function LoginPage() {
  const { login, sessionRevoked, clearSessionRevoked } = useAuth();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [errorDetail, setErrorDetail] = useState<string | null>(null);

  useEffect(() => {
    if (sessionRevoked) {
      setError(SSO_ERROR_MESSAGES.session_revoked);
      setErrorDetail(null);
      clearSessionRevoked();
    }
  }, [sessionRevoked, clearSessionRevoked]);

  const handleLogin = async () => {
    setLoading(true);
    setError(null);
    setErrorDetail(null);
    try {
      await login();
    } catch (err) {
      const mapped = mapLoginError(err);
      setError(mapped.message);
      setErrorDetail(mapped.detail ?? null);
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

        {error && (
          <div className="login-error">
            {error}
            {errorDetail && (
              <div className="login-error-detail" title={errorDetail}>
                {errorDetail}
              </div>
            )}
          </div>
        )}

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
