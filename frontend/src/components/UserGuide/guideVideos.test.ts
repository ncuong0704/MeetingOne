import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { GUIDE_VIDEOS, isAllowedYoutubeUrl, youtubeOpenUrl } from './guideVideos.ts';

test('GUIDE_VIDEOS is an array of items with id, title, description, youtubeUrl', () => {
  assert.equal(Array.isArray(GUIDE_VIDEOS), true);
  for (const item of GUIDE_VIDEOS) {
    assert.equal(typeof item.id, 'string');
    assert.ok(item.id.trim().length > 0);
    assert.equal(typeof item.title, 'string');
    assert.equal(typeof item.description, 'string');
    assert.equal(typeof item.youtubeUrl, 'string');
  }
});

test('isAllowedYoutubeUrl accepts youtube and youtu.be https', () => {
  assert.equal(isAllowedYoutubeUrl('https://www.youtube.com/watch?v=dQw4w9WgXcQ'), true);
  assert.equal(isAllowedYoutubeUrl('https://youtu.be/dQw4w9WgXcQ'), true);
  assert.equal(isAllowedYoutubeUrl('https://m.youtube.com/watch?v=dQw4w9WgXcQ'), true);
  assert.equal(isAllowedYoutubeUrl('https://www.youtube.com/shorts/dQw4w9WgXcQ'), true);
});

test('isAllowedYoutubeUrl rejects unsafe or non-youtube URLs', () => {
  assert.equal(isAllowedYoutubeUrl('javascript:alert(1)'), false);
  assert.equal(isAllowedYoutubeUrl('http://www.youtube.com/watch?v=dQw4w9WgXcQ'), false);
  assert.equal(isAllowedYoutubeUrl('https://example.com/watch?v=dQw4w9WgXcQ'), false);
  assert.equal(isAllowedYoutubeUrl('not a url'), false);
  assert.equal(isAllowedYoutubeUrl(''), false);
});

test('youtubeOpenUrl canonicalizes to youtu.be without query ampersands', () => {
  assert.equal(
    youtubeOpenUrl('https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=12s'),
    'https://youtu.be/dQw4w9WgXcQ',
  );
  assert.equal(youtubeOpenUrl('https://youtu.be/abc123xyz'), 'https://youtu.be/abc123xyz');
  assert.equal(
    youtubeOpenUrl('https://www.youtube.com/embed/dQw4w9WgXcQ'),
    'https://youtu.be/dQw4w9WgXcQ',
  );
  assert.equal(
    youtubeOpenUrl('https://www.youtube.com/shorts/dQw4w9WgXcQ'),
    'https://youtu.be/dQw4w9WgXcQ',
  );
  assert.equal(youtubeOpenUrl('https://example.com/watch?v=x'), null);
});

test('GUIDE_VIDEOS comes from mac-dinh/video-huong-dan.json', () => {
  const catalog = JSON.parse(
    readFileSync(
      join(
        dirname(fileURLToPath(import.meta.url)),
        '../../../src-tauri/resources/mac-dinh/video-huong-dan.json',
      ),
      'utf8',
    ),
  );
  assert.deepEqual(GUIDE_VIDEOS, catalog);
});
