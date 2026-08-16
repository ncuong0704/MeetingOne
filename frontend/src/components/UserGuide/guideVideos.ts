/**
 * Danh sách video hướng dẫn: điền file
 * `frontend/src-tauri/resources/mac-dinh/video-huong-dan.json`
 */
import catalog from '../../../src-tauri/resources/mac-dinh/video-huong-dan.json';

export type GuideVideo = {
  id: string;
  title: string;
  description: string;
  youtubeUrl: string;
};

export const GUIDE_VIDEOS: GuideVideo[] = catalog;

const YOUTUBE_HOSTS = new Set(['youtube.com', 'youtu.be', 'm.youtube.com']);

function hostnameWithoutWww(hostname: string): string {
  return hostname.replace(/^www\./i, '').toLowerCase();
}

/** True for https YouTube watch / youtu.be / m.youtube.com URLs. */
export function isAllowedYoutubeUrl(url: string): boolean {
  try {
    const parsed = new URL(url.trim());
    if (parsed.protocol !== 'https:') return false;
    return YOUTUBE_HOSTS.has(hostnameWithoutWww(parsed.hostname));
  } catch {
    return false;
  }
}

/**
 * Canonical https://youtu.be/{id} so Windows `cmd start` is not split on `&`
 * in watch?v= query strings.
 */
export function youtubeOpenUrl(url: string): string | null {
  if (!isAllowedYoutubeUrl(url)) return null;
  try {
    const parsed = new URL(url.trim());
    const host = hostnameWithoutWww(parsed.hostname);
    if (host === 'youtu.be') {
      const id = parsed.pathname.replace(/^\//, '').split('/')[0];
      return id ? `https://youtu.be/${id}` : null;
    }
    const fromQuery = parsed.searchParams.get('v');
    if (fromQuery) return `https://youtu.be/${fromQuery}`;
    const embed = parsed.pathname.match(/\/embed\/([^/]+)/);
    if (embed?.[1]) return `https://youtu.be/${embed[1]}`;
    const shorts = parsed.pathname.match(/\/shorts\/([^/]+)/);
    if (shorts?.[1]) return `https://youtu.be/${shorts[1]}`;
    return null;
  } catch {
    return null;
  }
}
