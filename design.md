# Design — ACT MeetingOne

A locked design system for this app. Every page redesign reads this file before
emitting code. Do not regenerate per page — extend or amend this file when the
system needs to grow.

Hallmark · genre: modern-minimal · tone: utilitarian · designed-as-app
Audience: nhân sự ACT, ưu tiên người tổ chức / thư ký cuộc họp.
Use: ghi âm, theo dõi transcript sống, xem summary / action items, xuất biên bản.

## Genre

modern-minimal (enterprise workbench, not a marketing site)

## Macrostructure family

- Marketing pages: none. This is a desktop product.
- App pages: Workbench (N3 side-rail + canvas). Home, meeting details, notes.
- Content pages: Long Document inside Workbench chrome. Settings.
- Auth: Letter (left-biased, one job). Login, onboarding.

## Theme

Custom OKLCH anchored on ACT navy `#16478e` (hue 262). Cool neutrals. One accent.

- `--color-paper`   oklch(97.4% 0.008 262)   /* canvas */
- `--color-paper-2` oklch(99.2% 0.004 262)   /* raised surfaces */
- `--color-rail`    oklch(91.8% 0.022 262)   /* N3 sidebar — darker than canvas on purpose */
- `--color-ink`     oklch(22% 0.024 262)
- `--color-ink-2`   oklch(46% 0.018 262)
- `--color-rule`    oklch(88% 0.012 262)
- `--color-accent`  oklch(39.6% 0.105 262)   /* #16478e */
- `--color-accent-hover` oklch(44.8% 0.114 262)  /* #1a55ab */
- `--color-focus`   oklch(39.6% 0.105 262)
- `--color-danger`  oklch(59.9% 0.207 29)    /* #e63027 */

Accent occupies ≤ 5% of a viewport. Danger red is reserved for record / stop / delete.

## Typography

- Display: IBM Plex Sans, weight 600–700, style normal. Vietnamese subset required.
- Body:    IBM Plex Sans, weight 400–500. Same family (UI density).
- Mono:    IBM Plex Mono, weight 400–500 (timestamps, hotkeys, sequence ids).
- Display tracking: -0.02em on titles ≥ 1.25rem.
- Type scale anchor: body 16px, ratio 1.25. Headings in app chrome stay ≤ `--text-xl`.

IBM Plex Sans is the workbench pair for IBM Plex Mono and covers Vietnamese.
Do not swap to Inter / Geist / Source Sans 3 (the previous UI font).

## Spacing

4-point named scale. Values live in `tokens.css`. Use named tokens
(`var(--space-md)`), never ad-hoc gray-50 / px soup for new chrome.

## Motion

- Easings: `--ease-out` cubic-bezier(0.16, 1, 0.3, 1); `--ease-in` cubic-bezier(0.7, 0, 0.84, 0); `--ease-in-out` cubic-bezier(0.65, 0, 0.35, 1).
- Reveal: none on app pages. Color / opacity on control states only, `--dur-micro` 120ms / `--dur-short` 220ms.
- Reduced-motion: opacity-only, ≤ 150ms. Never animate `:focus-visible`.

## Microinteractions stance

- Silent success (existing toasts stay for durable errors / exports).
- Tooltip hover delay 800ms · focus delay 0ms.
- Record / stop: instant press (`translateY(1px)` on `:active`). No glow.

## CTA voice

- Primary CTA: filled accent, 8px radius, 14px/500, one line. "Đăng nhập bằng AMS SSO", "Lưu".
- Secondary CTA: 1px rule + accent text, same radius. "Nhập file âm thanh".
- Destructive CTA: filled danger. Only record, stop, delete, logout-when-destructive.

## Per-page allowances

- Marketing pages MAY use enrichment: not applicable.
- App pages MUST NOT use enrichment — function carries the page.
- Content pages: typography only.
- Login MAY left-bias the card; MUST NOT center a marketing hero.

## What pages MUST share

- Wordmark / logo ACT MeetingOne (`/act-meetingone-logo.png`).
- Accent colour and placement (≤ 5% per viewport).
- Display + body + mono pairing above.
- CTA voice (8px radius, navy fill / navy outline / red only for danger).
- Sidebar as the only primary nav (N3).

## What pages MAY differ on

- Macrostructure within the page-type family (Workbench vs Letter vs Long Document).
- Local control density (full-width recording bar vs settings form).
- Settings uses a vertical Long Document rail, not a horizontal tab strip.
- No hero enrichment anywhere.

## Exports

See `tokens.css` at the project root (mirrored in `frontend/src/app/tokens.css`).
