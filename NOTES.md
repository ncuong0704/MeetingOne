- window.__TAURI_INTERNALS__.invoke("reset_onboarding_status_cmd", {}).then(() => window.location.reload());
- pnpm run tauri:dev:cpu 2>&1
- pnpm run tauri:build:cpu

-lấy chữ ký
- pnpm tauri signer generate -w "$USERPROFILE/.tauri/meetingone.key"

- build có chữ ký
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$USERPROFILE/.tauri/meetingone.key" -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "123456"   # hoặc password bạn đã nhập
pnpm run tauri:build