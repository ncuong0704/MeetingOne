import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
    formatTranscriptHtml,
    formatTranscriptPlainText,
    type TranscriptDisplaySegment,
} from './transcriptDisplay.ts';

function seg(
    partial: Partial<TranscriptDisplaySegment> & { id: string; text: string },
): TranscriptDisplaySegment {
    return partial;
}

const TIME_TOKEN = /\[\d{2}:\d{2}\]/;

test('plain text has no timestamps or meeting header', () => {
    const text = formatTranscriptPlainText([
        seg({ id: '1', text: 'Xin chào mọi người.' }),
        seg({ id: '2', text: 'Hôm nay họp ngân sách' }),
    ]);
    assert.equal(text.includes('Bản ghi cuộc họp'), false);
    assert.equal(TIME_TOKEN.test(text), false);
    assert.equal(text.includes('['), false);
});

test('without speakers, segments flow and break after a sentence', () => {
    const text = formatTranscriptPlainText([
        seg({ id: '1', text: 'Xin chào mọi người.' }),
        seg({ id: '2', text: 'Hôm nay họp ngân sách' }),
    ]);
    assert.equal(text, 'Xin chào mọi người.\nHôm nay họp ngân sách');
});

test('with speakers, copy includes names and merges consecutive same speaker', () => {
    const text = formatTranscriptPlainText([
        seg({ id: '1', text: 'Xin chào.', speakerId: 's1', speakerName: 'Lan' }),
        seg({ id: '2', text: 'Bắt đầu họp', speakerId: 's1', speakerName: 'Lan' }),
        seg({ id: '3', text: 'Đồng ý.', speakerId: 's2', speakerName: 'Minh' }),
    ]);
    assert.equal(TIME_TOKEN.test(text), false);
    assert.equal(text, 'Lan\nXin chào.\nBắt đầu họp\n\nMinh\nĐồng ý.');
});

test('unnamed speaker falls back to Người nói', () => {
    const text = formatTranscriptPlainText([
        seg({ id: '1', text: 'Hello there everyone', speakerId: 's1', speakerName: null }),
    ]);
    assert.match(text, /^Người nói\nHello there everyone$/);
});

test('empty segment becomes [Im lặng]', () => {
    const text = formatTranscriptPlainText([seg({ id: '1', text: '   ' })]);
    assert.equal(text, '[Im lặng]');
});

test('html matches structure and has no timestamp spans', () => {
    const html = formatTranscriptHtml([
        seg({ id: '1', text: 'Xin chào.', speakerId: 's1', speakerName: 'Lan' }),
        seg({ id: '2', text: 'Ok', speakerId: 's2', speakerName: 'Minh' }),
    ]);
    assert.equal(html.includes('[00:'), false);
    assert.equal(html.includes('Courier'), false);
    assert.match(html, /<p[^>]*>Lan<\/p>/);
    assert.match(html, /<p[^>]*>Minh<\/p>/);
    assert.match(html, /Xin chào\./);
});

test('empty list is empty string', () => {
    assert.equal(formatTranscriptPlainText([]), '');
    assert.equal(formatTranscriptHtml([]), '');
});
