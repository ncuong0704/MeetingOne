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

/// Sau một lần đăng nhập thành công, tin phiên cục bộ trong ngần này mà KHÔNG gọi
/// AMS mỗi lần mở app — chỉ kiểm tra lại (UserInfo/refresh) sau khi hết hạn mức này.
/// Lý do: gọi AMS ở mỗi lần khởi động vừa chậm vừa lôi theo mọi lỗi tạm thời của AMS
/// (đã thấy AMS tự trả token cũ hết hạn — xem `ams_stale_token`) vào trải nghiệm mở app.
const SESSION_TRUST_DAYS: i64 = 30;

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
    /// Mốc lần cuối phiên được XÁC THỰC với AMS thành công (đăng nhập mới, hoặc lần
    /// kiểm tra định kỳ gần nhất) — không phải mốc token được cấp. `session_needs_check`
    /// tính hạn 30 ngày từ đây.
    logged_in_at: String,
}

/// true nếu đã quá `SESSION_TRUST_DAYS` kể từ lần xác thực gần nhất — chỉ khi đó mới
/// cần gọi AMS lại. Mốc thời gian hỏng/thiếu được coi là đã quá hạn (an toàn hơn).
fn session_needs_check(session: &StoredSsoSession) -> bool {
    match chrono::DateTime::parse_from_rfc3339(&session.logged_in_at) {
        Ok(logged_in_at) => {
            let elapsed = chrono::Utc::now().signed_duration_since(logged_in_at.with_timezone(&chrono::Utc));
            elapsed >= chrono::Duration::days(SESSION_TRUST_DAYS)
        }
        Err(_) => true,
    }
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
    #[serde(default, deserialize_with = "string_or_number")]
    refresh_token: Option<String>,
}

/// AMS trả `refresh_token` là số (ID phiên) chứ không phải chuỗi — chấp nhận cả hai.
fn string_or_number<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::String(s) => Some(s),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

/// Đọc iat/exp/nbf từ JWT (chỉ các claim thời gian — không chạm phần còn lại).
fn jwt_time_claims(token: &str) -> Option<(Option<i64>, Option<i64>, Option<i64>)> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .ok()?;
    let claims = serde_json::from_slice::<serde_json::Value>(&bytes).ok()?;
    let num = |key: &str| claims.get(key).and_then(|v| v.as_i64());
    Some((num("iat"), num("exp"), num("nbf")))
}

/// Log claims + trả về exp để caller kiểm tra "token chết ngay khi cấp".
fn log_jwt_time_claims(token: &str) -> Option<i64> {
    let (iat, exp, nbf) =
        jwt_time_claims(token).unwrap_or((None, None, None));
    info!(
        "AMS token claims: iat={:?} exp={:?} nbf={:?} (now={})",
        iat,
        exp,
        nbf,
        chrono::Utc::now().timestamp()
    );
    exp
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

/// Bind the loopback callback listener. Must happen BEFORE the browser is opened:
/// with a cached AMS session the redirect can arrive within milliseconds, and a
/// callback hitting an unbound port is lost (browser shows "connection refused",
/// the app then hangs until the login timeout).
async fn bind_callback_listener() -> Result<TcpListener, String> {
    TcpListener::bind(format!("127.0.0.1:{}", CALLBACK_PORT))
        .await
        .map_err(|e| {
            format!(
                "callback_port_busy: cổng {} đang bị chiếm (có thể app ACT MeetingOne khác đang mở): {}",
                CALLBACK_PORT, e
            )
        })
}

async fn wait_for_auth_code(listener: TcpListener, expected_state: &str) -> Result<String, String> {
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

    let raw = response.text().await.context("Không đọc được phản hồi AMS Token")?;
    // Log field names + value sizes only (numbers shown raw — never token contents).
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
        let shape: Vec<String> = value
            .as_object()
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| match v {
                        serde_json::Value::String(s) => format!("{k}={}b", s.len()),
                        serde_json::Value::Object(o) => {
                            format!("{k}={{{}}}", o.keys().cloned().collect::<Vec<_>>().join(","))
                        }
                        other => format!("{k}={other}"),
                    })
                    .collect()
            })
            .unwrap_or_default();
        info!("AMS Token response fields: {:?}", shape);
    }
    let tokens: TokenResponse =
        serde_json::from_str(&raw).context("Phản hồi AMS Token không hợp lệ")?;
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
        // Capture why AMS rejected the freshly issued token: body text and any
        // WWW-Authenticate hint. Token contents are never logged — only shape.
        let www_auth = response
            .headers()
            .get("WWW-Authenticate")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body = response.text().await.unwrap_or_default();
        info!(
            "AMS UserInfo 401: token_len={} token_prefix={:?} www_authenticate={:?} body={}",
            access_token.len(),
            access_token.chars().take(8).collect::<String>(),
            www_auth,
            body
        );
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
            // Xác thực thành công: cập nhật thông tin (nếu đổi) và luôn đặt lại mốc
            // logged_in_at để khởi động lại đồng hồ tin cậy 30 ngày từ đây.
            session.email = user.email.clone();
            session.full_name = user.full_name.clone();
            session.logged_in_at = chrono::Utc::now().to_rfc3339();
            save_session(app, &session).await?;
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
                    session.logged_in_at = chrono::Utc::now().to_rfc3339();
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

/// Classify a login failure into a stable `<code>: <detail>` string the frontend can
/// map to a specific message. Production Windows builds have no console, so this
/// detail (status codes, AMS response bodies, network errors) is the only place the
/// real cause is visible — it must reach the UI, not just the log.
fn map_login_error(err: anyhow::Error) -> String {
    let msg = err.to_string();
    for code in [
        "no_email",
        "sso_denied",
        "invalid_state",
        "invalid_callback",
        "sso_timeout",
        "callback_port_busy",
        "browser_open_failed",
        "ams_stale_token",
    ] {
        if msg.contains(code) {
            return msg;
        }
    }
    error!("SSO login failed: {}", msg);
    let lower = msg.to_lowercase();
    let network = lower.contains("không kết nối được")
        || lower.contains("error sending request")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("connection")
        || lower.contains("dns");
    if lower.contains("ams userinfo") {
        return if network {
            format!("ams_unreachable: {msg}")
        } else {
            format!("ams_userinfo_error: {msg}")
        };
    }
    if lower.contains("ams token") || lower.contains("access_token") {
        return if network {
            format!("ams_unreachable: {msg}")
        } else {
            format!("ams_token_error: {msg}")
        };
    }
    if network {
        return format!("ams_unreachable: {msg}");
    }
    format!("sso_error: {msg}")
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

    if !session_needs_check(&session) {
        // Trong hạn tin cậy 30 ngày kể từ lần xác thực gần nhất: không gọi AMS,
        // dùng thẳng phiên cục bộ để app mở nhanh và không phụ thuộc AMS mỗi lần mở.
        return Ok(SsoSessionCheck {
            user: Some(SsoUser::from(&session)),
            session_revoked: false,
        });
    }

    validate_stored_session(&app, &config, session).await
}

#[tauri::command]
pub async fn sso_login<R: Runtime>(app: AppHandle<R>) -> Result<SsoUser, String> {
    let config = SsoConfig::from_env();
    let state = generate_state();
    let authorize_url = config
        .authorize_url(&state)
        .map_err(|e| format!("sso_error: {}", e))?;

    // Bind the callback listener BEFORE the browser can redirect anywhere —
    // a fast (cached-session) login would otherwise race the bind.
    let listener = bind_callback_listener().await?;

    info!("Starting SSO login, opening browser");
    open_browser(&authorize_url).map_err(|e| format!("browser_open_failed: {e}"))?;

    let code = wait_for_auth_code(listener, &state).await?;

    let (mut access_token, mut refresh_token) = exchange_code(&config, &code)
        .await
        .map_err(map_login_error)?;
    if let Some(exp) = log_jwt_time_claims(&access_token) {
        let now = chrono::Utc::now().timestamp();
        if exp <= now {
            // AMS đã xảy ra tình trạng tái sử dụng session cũ trên trình duyệt và trả
            // lại token đã hết hạn (exp nằm trong quá khứ ngay khi vừa "cấp").
            return Err(format!(
                "ams_stale_token: AMS trả token có exp trong quá khứ (exp={exp}, now={now}) \
                 — token tái sử dụng từ phiên cũ. Hãy đăng xuất AMS trên trình duyệt rồi thử lại."
            ));
        }
    }

    // AMS đã từng cấp token mà chính UserInfo của nó tuyên bố "Token has expired"
    // ngay lập tức (O_RESULT:-3). Endpoint Refresh là code path khác — thử đúng một
    // lần refresh rồi gọi lại UserInfo trước khi chịu thua.
    let mut user = match fetch_user_info(&config, &access_token).await {
        Ok(user) => Some(user),
        Err(UserInfoError::Unauthorized) => None,
        Err(e) => {
            return Err(map_login_error(match e {
                UserInfoError::NoEmail => anyhow!("no_email"),
                UserInfoError::Transient(msg) => anyhow!(msg),
                UserInfoError::Unauthorized => unreachable!(),
            }))
        }
    };
    if user.is_none() && !refresh_token.is_empty() {
        warn!("UserInfo từ chối token vừa cấp (401); thử refresh một lần rồi gọi lại");
        match refresh_access_token(&config, &refresh_token).await {
            Ok((new_access, new_refresh)) => {
                access_token = new_access;
                refresh_token = new_refresh;
                log_jwt_time_claims(&access_token);
                user = fetch_user_info(&config, &access_token).await.ok();
            }
            Err(e) => warn!("Refresh sau khi token bị từ chối thất bại: {e}"),
        }
    }
    let Some(user) = user else {
        return Err(
            "ams_userinfo_error: AMS UserInfo từ chối token (401) cả sau khi refresh".into(),
        );
    };

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

    #[test]
    fn token_response_accepts_numeric_refresh_token() {
        let json = r#"{"access_token":"abc","refresh_token":123456,"expires_in":300}"#;
        let tokens: TokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(tokens.access_token.as_deref(), Some("abc"));
        assert_eq!(tokens.refresh_token.as_deref(), Some("123456"));
    }

    #[test]
    fn token_response_accepts_string_refresh_token() {
        let json = r#"{"access_token":"abc","refresh_token":"opaque"}"#;
        let tokens: TokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(tokens.refresh_token.as_deref(), Some("opaque"));
    }

    fn session_with_logged_in_at(logged_in_at: String) -> StoredSsoSession {
        StoredSsoSession {
            email: "user@vienthongact.vn".into(),
            full_name: "User".into(),
            sso_access_token: "access".into(),
            sso_refresh_token: "refresh".into(),
            logged_in_at,
        }
    }

    #[test]
    fn session_needs_check_false_within_trust_window() {
        let recent = chrono::Utc::now() - chrono::Duration::days(1);
        let session = session_with_logged_in_at(recent.to_rfc3339());
        assert!(!session_needs_check(&session));
    }

    #[test]
    fn session_needs_check_false_just_under_30_days() {
        let almost = chrono::Utc::now() - chrono::Duration::days(29);
        let session = session_with_logged_in_at(almost.to_rfc3339());
        assert!(!session_needs_check(&session));
    }

    #[test]
    fn session_needs_check_true_after_30_days() {
        let stale = chrono::Utc::now() - chrono::Duration::days(31);
        let session = session_with_logged_in_at(stale.to_rfc3339());
        assert!(session_needs_check(&session));
    }

    #[test]
    fn session_needs_check_true_for_malformed_timestamp() {
        let session = session_with_logged_in_at("not-a-timestamp".into());
        assert!(session_needs_check(&session));
    }

    #[test]
    fn jwt_time_claims_parses_exp_and_iat() {
        use base64::Engine;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"iat":100,"exp":200,"nbf":100,"sub":"user"}"#);
        let token = format!("eyJhbGciOiJIUzI1NiJ9.{payload}.signature");
        assert_eq!(
            jwt_time_claims(&token),
            Some((Some(100), Some(200), Some(100)))
        );
        assert_eq!(jwt_time_claims("not-a-jwt"), None);
    }

    #[test]
    fn login_error_classification() {
        let mapped = |msg: &str| map_login_error(anyhow!("{}", msg));
        assert_eq!(mapped("sso_denied"), "sso_denied");
        assert_eq!(mapped("no_email"), "no_email");
        assert_eq!(
            mapped("callback_port_busy: cổng 34517 đang bị chiếm"),
            "callback_port_busy: cổng 34517 đang bị chiếm"
        );
        assert!(mapped("Không kết nối được AMS Token: error sending request").starts_with("ams_unreachable"));
        assert!(mapped("AMS Token trả lỗi 500: oops").starts_with("ams_token_error"));
        assert!(mapped("Không kết nối được AMS UserInfo: timed out").starts_with("ams_unreachable"));
        assert!(mapped("AMS UserInfo trả lỗi 503: busy").starts_with("ams_userinfo_error"));
        assert!(mapped("Phản hồi AMS Refresh không hợp lệ").starts_with("sso_error"));
    }
}
