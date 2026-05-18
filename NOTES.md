- window.__TAURI_INTERNALS__.invoke("reset_onboarding_status_cmd", {}).then(() => window.location.reload());
- pnpm run tauri:dev:cpu 2>&1
- pnpm run tauri:build:cpu

-lấy chữ ký
- pnpm tauri signer generate -w "$USERPROFILE/.tauri/meetingone.key"

- build có chữ ký
$env:TAURI_SIGNING_PRIVATE_KEY = "dW50cnVzdGVkIGNvbW1lbnQ6IHJzaWduIGVuY3J5cHRlZCBzZWNyZXQga2V5ClJXUlRZMEl5L3pibkFmTlMrdU1vMHo0VG5YaldTaW0ralQyU0w4dkZwU0FoTjBMeTBFQUFBQkFBQUFBQUFBQUFBQUlBQUFBQU5WTW9lWlhweFdlV1hlYVhGQ055NVpJb2ZmaU1KbHVGY3pORThhQk5sbjc3SE9TVjlPakpDNXNsbTVySHNwZnJPYXNKaDFGdmlENENrb0hqNFFFMzNnZVNVaEhYVTV3UkpzL1JsVFA0M2N5MXU3RW9qSm11NWpqQnJSd0YzQXlmWjExSGVoZDl6ODQ9Cg=="
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "123456"   # hoặc password bạn đã nhập
pnpm run tauri:build


# 1. Tạo release (script đã viết sẵn)
.\release.ps1 -Version "0.0.3"

# 2. Sau ~15 phút build xong, publish
gh release edit v0.0.3 --repo ncuong0704/MeetingOne --draft=false


