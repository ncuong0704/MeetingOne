use anyhow::{anyhow, Context, Result};
use log::{error, info, warn};
use reqwest::Client;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::timeout;
use url::Url;
use url::form_urlencoded;

const STORE_FILE: &str = "sso-session.json";
const SESSION_KEY: &str = "session";
const CALLBACK_PORT: u16 = 34517;
const CALLBACK_PATH: &str = "/auth/callback";
const LOGIN_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone)]
struct SsoConfig {
    client_id: String,
    client_secret: String,
    authorize_url: String,
    token_url: String,
    userinfo_url: String,
    refresh_url: String,
    redirect_uri: String,
}

impl SsoConfig {
    fn from_env() -> Self {
        Self {
            client_id: std::env::var("SSO_CLIENT_ID").unwrap_or_else(|_| "AMS_AI".into()),
            client_secret: std::env::var("SSO_CLIENT_SECRET").unwrap_or_else(|_| "AMS_AI".into()),
            authorize_url: std::env::var("SSO_AUTHORIZE_URL")
                .unwrap_or_else(|_| "https://ams.vienthongact.vn/Api/Sso/Authorize".into()),
            token_url: std::env::var("SSO_TOKEN_URL")
                .unwrap_or_else(|_| "https://ams.vienthongact.vn/Api/Sso/Token".into()),
            userinfo_url: std::env::var("SSO_USERINFO_URL")
                .unwrap_or_else(|_| "https://ams.vienthongact.vn/Api/Sso/UserInfo".into()),
            refresh_url: std::env::var("SSO_REFRESH_URL")
                .unwrap_or_else(|_| "https://ams.vienthongact.vn/Api/Auth/Refresh_Token".into()),
            redirect_uri: std::env::var("SSO_REDIRECT_URI").unwrap_or_else(|_| {
                format!("http://127.0.0.1:{}{}", CALLBACK_PORT, CALLBACK_PATH)
            }),
        }
    }

    fn authorize_url(&self, state: &str) -> Result<String> {
        let mut url = Url::parse(&self.authorize_url).context("SSO_AUTHORIZE_URL không hợp lệ")?;
        url.query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", &self.redirect_uri)
            .append_pair("state", state)
            .append_pair("response_type", "code");
        Ok(url.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSsoSession {
    email: String,
    full_name: String,
    sso_access_token: String,
    sso_refresh_token: String,
    logged_in_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsoUser {
    pub email: String,
    pub full_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsoSessionCheck {
    pub user: Option<SsoUser>,
    /// true khi AMS từ chối token (tài khoản thu hồi / refresh hết hạn)
    pub session_revoked: bool,
}

#[derive(Debug)]
enum UserInfoError {
    Unauthorized,
    NoEmail,
    Transient(String),
}

impl From<&StoredSsoSession> for SsoUser {
    fn from(value: &StoredSsoSession) -> Self {
        Self {
            email: value.email.clone(),
            full_name: value.full_name.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SsoUserInfo {
    email: Option<String>,
    #[serde(alias = "fullName")]
    full_name: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AmsRefreshCustoms {
    #[serde(rename = "AccessToken")]
    access_token: Option<String>,
    #[serde(rename = "RefreshToken")]
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AmsRefreshResponse {
    #[serde(rename = "O_RESULT")]
    o_result: i32,
    #[serde(rename = "O_CUSTOMS")]
    o_customs: Option<AmsRefreshCustoms>,
}

fn generate_state() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn parse_query(query: &str) -> HashMap<String, String> {
    form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect()
}

fn parse_request_path(request: &str) -> Option<String> {
    request.lines().next().and_then(|line| {
        let mut parts = line.split_whitespace();
        let _method = parts.next()?;
        let path = parts.next()?;
        Some(path.to_string())
    })
}

async fn write_html_response(stream: &mut tokio::net::TcpStream, body: &str) -> Result<()> {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}

async fn wait_for_auth_code(expected_state: &str) -> Result<String, String> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", CALLBACK_PORT))
        .await
        .map_err(|e| format!("Không mở được cổng đăng nhập ({}): {}", CALLBACK_PORT, e))?;

    let wait = async {
        loop {
            let (mut stream, _) = listener
                .accept()
                .await
                .map_err(|e| format!("Lỗi callback SSO: {}", e))?;

            let mut buf = vec![0u8; 8192];
            let n = stream
                .read(&mut buf)
                .await
                .map_err(|e| format!("Lỗi đọc callback SSO: {}", e))?;
            let request = String::from_utf8_lossy(&buf[..n]);
            let path = parse_request_path(&request).unwrap_or_default();

            if !path.starts_with(CALLBACK_PATH) {
                continue;
            }

            let query = path.split('?').nth(1).unwrap_or("");
            let params = parse_query(query);

            let body = r#"<!DOCTYPE html><html lang="vi"><head><meta charset="utf-8"><title>Đăng nhập thành công</title></head><body style="font-family:sans-serif;text-align:center;padding:48px"><h2>Đăng nhập thành công</h2><p>Bạn có thể đóng tab này và quay lại ACT MeetingOne.</p></body></html>"#;
            write_html_response(&mut stream, body)
                .await
                .map_err(|e| format!("Lỗi phản hồi callback SSO: {}", e))?;

            if params.contains_key("error") {
                return Err("sso_denied".into());
            }

            let state = params.get("state").cloned().unwrap_or_default();
            if state != expected_state {
                return Err("invalid_state".into());
            }

            let code = params
                .get("code")
                .cloned()
                .ok_or_else(|| "invalid_callback".to_string())?;
            return Ok(code);
        }
    };

    timeout(Duration::from_secs(LOGIN_TIMEOUT_SECS), wait)
        .await
        .map_err(|_| "sso_timeout".to_string())?
}

async fn exchange_code(config: &SsoConfig, code: &str) -> Result<(String, String)> {
    let client = Client::new();
    let response = client
        .post(&config.token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", config.redirect_uri.as_str()),
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
        ])
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .context("Không kết nối được AMS Token")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("AMS Token trả lỗi {}: {}", status, body));
    }

    let tokens: TokenResponse = response.json().await.context("Phản hồi AMS Token không hợp lệ")?;
    let access = tokens
        .access_token
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow!("AMS không trả access_token"))?;
    let refresh = tokens.refresh_token.unwrap_or_default();
    Ok((access, refresh))
}

async fn fetch_user_info(config: &SsoConfig, access_token: &str) -> Result<SsoUser, UserInfoError> {
    let client = Client::new();
    let response = client
        .get(&config.userinfo_url)
        .header("Authorization", format!("Bearer {}", access_token))
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| UserInfoError::Transient(format!("Không kết nối được AMS UserInfo: {}", e)))?;

    if response.status() == StatusCode::UNAUTHORIZED {
        return Err(UserInfoError::Unauthorized);
    }

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(UserInfoError::Transient(format!(
            "AMS UserInfo trả lỗi {}: {}",
            status, body
        )));
    }

    let info: SsoUserInfo = response
        .json()
        .await
        .map_err(|e| UserInfoError::Transient(format!("Phản hồi AMS UserInfo không hợp lệ: {}", e)))?;
    let email = info.email.unwrap_or_default().trim().to_string();
    if email.is_empty() {
        return Err(UserInfoError::NoEmail);
    }

    let full_name = info
        .full_name
        .or(info.name)
        .unwrap_or_else(|| email.clone())
        .trim()
        .to_string();

    Ok(SsoUser { email, full_name })
}

async fn refresh_access_token(config: &SsoConfig, refresh_token: &str) -> Result<(String, String)> {
    let client = Client::new();
    let response = client
        .get(&config.refresh_url)
        .query(&[("refreshToken", refresh_token)])
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .context("Không kết nối được AMS Refresh")?;

    if response.status() == StatusCode::UNAUTHORIZED {
        return Err(anyhow!("unauthorized"));
    }

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("AMS Refresh trả lỗi {}: {}", status, body));
    }

    let body: AmsRefreshResponse = response
        .json()
        .await
        .context("Phản hồi AMS Refresh không hợp lệ")?;

    if body.o_result != 1 {
        return Err(anyhow!("refresh_failed"));
    }

    let customs = body
        .o_customs
        .ok_or_else(|| anyhow!("refresh_failed"))?;
    let access = customs
        .access_token
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow!("refresh_failed"))?;
    let refresh = customs
        .refresh_token
        .unwrap_or_else(|| refresh_token.to_string());

    Ok((access, refresh))
}

async fn validate_stored_session<R: Runtime>(
    app: &AppHandle<R>,
    config: &SsoConfig,
    mut session: StoredSsoSession,
) -> Result<SsoSessionCheck, String> {
    match fetch_user_info(config, &session.sso_access_token).await {
        Ok(user) => {
            if user.email != session.email || user.full_name != session.full_name {
                session.email = user.email.clone();
                session.full_name = user.full_name.clone();
                save_session(app, &session).await?;
            }
            return Ok(SsoSessionCheck {
                user: Some(user),
                session_revoked: false,
            });
        }
        Err(UserInfoError::Unauthorized) => {}
        Err(UserInfoError::NoEmail) => {
            clear_session(app).await?;
            info!("SSO session cleared: AMS account has no email");
            return Ok(SsoSessionCheck {
                user: None,
                session_revoked: true,
            });
        }
        Err(UserInfoError::Transient(e)) => {
            warn!("AMS UserInfo không khả dụng, dùng phiên cục bộ: {}", e);
            return Ok(SsoSessionCheck {
                user: Some(SsoUser::from(&session)),
                session_revoked: false,
            });
        }
    }

    if session.sso_refresh_token.is_empty() {
        clear_session(app).await?;
        info!("SSO session cleared: access token expired, no refresh token");
        return Ok(SsoSessionCheck {
            user: None,
            session_revoked: true,
        });
    }

    match refresh_access_token(config, &session.sso_refresh_token).await {
        Ok((access, refresh)) => {
            session.sso_access_token = access;
            session.sso_refresh_token = refresh;
            match fetch_user_info(config, &session.sso_access_token).await {
                Ok(user) => {
                    session.email = user.email.clone();
                    session.full_name = user.full_name.clone();
                    save_session(app, &session).await?;
                    Ok(SsoSessionCheck {
                        user: Some(user),
                        session_revoked: false,
                    })
                }
                Err(_) => {
                    clear_session(app).await?;
                    info!("SSO session cleared: AMS rejected refreshed token");
                    Ok(SsoSessionCheck {
                        user: None,
                        session_revoked: true,
                    })
                }
            }
        }
        Err(_) => {
            clear_session(app).await?;
            info!("SSO session cleared: refresh token invalid or revoked");
            Ok(SsoSessionCheck {
                user: None,
                session_revoked: true,
            })
        }
    }
}

fn open_browser(url: &str) -> Result<(), String> {
    use std::process::Command;

    // Windows: không dùng `cmd /C start` — ký tự `&` trong query string bị cmd cắt,
    // khiến trình duyệt chỉ mở `...Authorize?client_id=AMS_AI` thay vì URL đầy đủ.
    let result = if cfg!(target_os = "windows") {
        Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn()
    } else {
        Command::new("xdg-open").arg(url).spawn()
    };

    result.map(|_| ()).map_err(|e| format!("Không mở được trình duyệt: {}", e))
}

async fn load_session<R: Runtime>(app: &AppHandle<R>) -> Result<Option<StoredSsoSession>, String> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| format!("Không đọc được phiên SSO: {}", e))?;

    let Some(value) = store.get(SESSION_KEY) else {
        return Ok(None);
    };

    match serde_json::from_value::<StoredSsoSession>(value.clone()) {
        Ok(session) => Ok(Some(session)),
        Err(e) => {
            warn!("Phiên SSO không hợp lệ, xóa: {}", e);
            store.delete(SESSION_KEY);
            store.save().ok();
            Ok(None)
        }
    }
}

async fn save_session<R: Runtime>(app: &AppHandle<R>, session: &StoredSsoSession) -> Result<(), String> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| format!("Không lưu được phiên SSO: {}", e))?;

    let value = serde_json::to_value(session)
        .map_err(|e| format!("Không serialize phiên SSO: {}", e))?;
    store.set(SESSION_KEY, value);
    store
        .save()
        .map_err(|e| format!("Không ghi phiên SSO ra đĩa: {}", e))?;
    Ok(())
}

async fn clear_session<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| format!("Không xóa được phiên SSO: {}", e))?;
    store.delete(SESSION_KEY);
    store
        .save()
        .map_err(|e| format!("Không ghi phiên SSO ra đĩa: {}", e))?;
    Ok(())
}

fn map_login_error(err: anyhow::Error) -> String {
    let msg = err.to_string();
    if msg.contains("no_email") {
        return "no_email".into();
    }
    if msg.contains("sso_denied")
        || msg.contains("invalid_state")
        || msg.contains("invalid_callback")
        || msg.contains("sso_timeout")
    {
        return msg;
    }
    error!("SSO login failed: {}", msg);
    "sso_error".into()
}

#[tauri::command]
pub async fn get_sso_session<R: Runtime>(app: AppHandle<R>) -> Result<SsoSessionCheck, String> {
    let config = SsoConfig::from_env();
    let Some(session) = load_session(&app).await? else {
        return Ok(SsoSessionCheck {
            user: None,
            session_revoked: false,
        });
    };

    validate_stored_session(&app, &config, session).await
}

#[tauri::command]
pub async fn sso_login<R: Runtime>(app: AppHandle<R>) -> Result<SsoUser, String> {
    let config = SsoConfig::from_env();
    let state = generate_state();
    let authorize_url = config
        .authorize_url(&state)
        .map_err(|e| format!("sso_error: {}", e))?;

    info!("Starting SSO login, opening browser");
    open_browser(&authorize_url)?;

    let code = wait_for_auth_code(&state).await?;

    let (access_token, refresh_token) = exchange_code(&config, &code)
        .await
        .map_err(map_login_error)?;

    let user = fetch_user_info(&config, &access_token)
        .await
        .map_err(|e| match e {
            UserInfoError::NoEmail => anyhow!("no_email"),
            UserInfoError::Unauthorized => anyhow!("sso_error"),
            UserInfoError::Transient(msg) => anyhow!(msg),
        })
        .map_err(map_login_error)?;

    let stored = StoredSsoSession {
        email: user.email.clone(),
        full_name: user.full_name.clone(),
        sso_access_token: access_token,
        sso_refresh_token: refresh_token,
        logged_in_at: chrono::Utc::now().to_rfc3339(),
    };
    save_session(&app, &stored).await?;

    info!("SSO login successful for {}", user.email);
    Ok(user)
}

#[tauri::command]
pub async fn sso_logout<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    clear_session(&app).await?;
    crate::onboarding::reset_onboarding_status(&app)
        .await
        .map_err(|e| format!("Không reset được onboarding: {}", e))?;
    info!("SSO session cleared, onboarding reset for next login");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_callback_query() {
        let params = parse_query("code=abc123&state=xyz");
        assert_eq!(params.get("code").map(String::as_str), Some("abc123"));
        assert_eq!(params.get("state").map(String::as_str), Some("xyz"));
    }

    #[test]
    fn build_authorize_url_contains_required_params() {
        let config = SsoConfig::from_env();
        let url = config.authorize_url("state123").unwrap();
        assert!(url.contains("client_id="));
        assert!(url.contains("redirect_uri="));
        assert!(url.contains("state=state123"));
        assert!(url.contains("response_type=code"));
    }

    #[test]
    fn parse_refresh_response() {
        let json = r#"{"O_RESULT":1,"O_CUSTOMS":{"AccessToken":"new-access","RefreshToken":"new-refresh"}}"#;
        let body: AmsRefreshResponse = serde_json::from_str(json).unwrap();
        assert_eq!(body.o_result, 1);
        let customs = body.o_customs.unwrap();
        assert_eq!(customs.access_token.as_deref(), Some("new-access"));
        assert_eq!(customs.refresh_token.as_deref(), Some("new-refresh"));
    }
}
