/**
 * Kiểm tra bản phát hành mới nhất trên GitHub (REST releases/latest).
 * Repo mặc định trùng README; override khi build: NEXT_PUBLIC_GITHUB_REPO=owner/repo
 */
const DEFAULT_GITHUB_REPO = 'ncuong0704/MeetingOne';

export function getGithubReleaseRepo(): string {
  const fromEnv = process.env.NEXT_PUBLIC_GITHUB_REPO?.trim();
  if (fromEnv) return fromEnv;
  return DEFAULT_GITHUB_REPO;
}

export type GitHubLatestRelease = {
  tag_name: string;
  html_url: string;
  name?: string;
};

export async function fetchLatestGitHubRelease(
  repo: string
): Promise<GitHubLatestRelease> {
  const res = await fetch(
    `https://api.github.com/repos/${repo}/releases/latest`,
    {
      headers: {
        Accept: 'application/vnd.github+json',
        'X-GitHub-Api-Version': '2022-11-28',
      },
    }
  );
  if (res.status === 404) {
    throw new Error('Chưa có release nào trên GitHub cho repo này.');
  }
  if (!res.ok) {
    throw new Error(`GitHub trả lỗi HTTP ${res.status}.`);
  }
  const data = (await res.json()) as GitHubLatestRelease;
  if (!data.tag_name || !data.html_url) {
    throw new Error('Dữ liệu release không hợp lệ.');
  }
  return data;
}

/** Chuẩn hóa "v1.2.3" / "1.2.3-beta" → các phần số để so sánh */
function versionParts(version: string): number[] {
  const stripped = version.replace(/^v/i, '').trim();
  const head = stripped.split(/[-+]/)[0] ?? stripped;
  return head.split('.').map((p) => {
    const n = parseInt(p, 10);
    return Number.isFinite(n) ? n : 0;
  });
}

/** true nếu remote mới hơn current (semver đơn giản theo từng phần số) */
export function isRemoteVersionNewer(
  remoteTag: string,
  currentVersion: string
): boolean {
  const a = versionParts(remoteTag);
  const b = versionParts(currentVersion);
  const n = Math.max(a.length, b.length);
  for (let i = 0; i < n; i++) {
    const da = a[i] ?? 0;
    const db = b[i] ?? 0;
    if (da !== db) return da > db;
  }
  return false;
}
