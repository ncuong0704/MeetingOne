/** Shared display rules for the meeting transcript pane (`FlowingTranscriptView`) and clipboard copy. */

export type TranscriptDisplaySegment = {
    id: string;
    text: string;
    speakerId?: string | null;
    speakerName?: string | null;
    speakerColor?: string | null;
};

export type SpeakerBlock<T extends TranscriptDisplaySegment = TranscriptDisplaySegment> = {
    key: string;
    speakerId: string | null;
    speakerName: string | null;
    speakerColor: string | null;
    segments: T[];
};

export function cleanStopWords(text: string): string {
    const stopWords = ['uh', 'um', 'er', 'ah', 'hmm', 'hm', 'eh', 'oh'];
    let cleanedText = text;
    stopWords.forEach((word) => {
        const pattern = new RegExp(`\\b${word}\\b[,\\s]*`, 'gi');
        cleanedText = cleanedText.replace(pattern, ' ');
    });
    return cleanedText.replace(/\s+/g, ' ').trim();
}

/** A sentence ending in '.', '?', or '!' (optionally followed by a closing quote). */
export function endsSentence(text: string): boolean {
    return /[.?!][)"'”]?\s*$/.test(text.trim());
}

export function displaySegmentText(text: string): string {
    const cleaned = cleanStopWords(text);
    if (cleaned) return cleaned;
    return text.trim() === '' ? '[Im lặng]' : cleaned;
}

export function groupBySpeaker<T extends TranscriptDisplaySegment>(segments: T[]): SpeakerBlock<T>[] {
    const blocks: SpeakerBlock<T>[] = [];
    for (const segment of segments) {
        const sid = segment.speakerId ?? null;
        const last = blocks[blocks.length - 1];
        if (last && last.speakerId === sid && sid !== null) {
            last.segments.push(segment);
        } else {
            blocks.push({
                key: `${sid ?? 'none'}-${segment.id}`,
                speakerId: sid,
                speakerName: segment.speakerName ?? null,
                speakerColor: segment.speakerColor ?? null,
                segments: [segment],
            });
        }
    }
    return blocks;
}

function flowingPlain(segments: TranscriptDisplaySegment[]): string {
    let out = '';
    for (let i = 0; i < segments.length; i++) {
        if (i > 0) {
            out += endsSentence(segments[i - 1].text) ? '\n' : ' ';
        }
        out += displaySegmentText(segments[i].text);
    }
    return out;
}

export function formatTranscriptPlainText(segments: TranscriptDisplaySegment[]): string {
    if (segments.length === 0) return '';

    const blocks = groupBySpeaker(segments);
    const hasAnySpeaker = blocks.some((b) => b.speakerId);

    if (!hasAnySpeaker) {
        return flowingPlain(segments);
    }

    return blocks
        .map((block) => {
            const body = flowingPlain(block.segments);
            if (block.speakerId) {
                const name = block.speakerName?.trim() || 'Người nói';
                return `${name}\n${body}`;
            }
            return body;
        })
        .join('\n\n');
}

function escapeHtml(s: string): string {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function flowingHtml(segments: TranscriptDisplaySegment[]): string {
    let inner = '';
    for (let i = 0; i < segments.length; i++) {
        if (i > 0) {
            inner += endsSentence(segments[i - 1].text) ? '<br>' : ' ';
        }
        inner += escapeHtml(displaySegmentText(segments[i].text));
    }
    return `<p>${inner}</p>`;
}

const FONT = "Calibri,'Segoe UI',Arial,sans-serif";

export function formatTranscriptHtml(segments: TranscriptDisplaySegment[]): string {
    if (segments.length === 0) return '';

    const pStyle = `font-family:${FONT};font-size:11pt;margin:3pt 0;`;
    const nameStyle = `font-family:${FONT};font-size:10pt;font-weight:bold;margin:10pt 0 3pt;`;

    const blocks = groupBySpeaker(segments);
    const hasAnySpeaker = blocks.some((b) => b.speakerId);

    if (!hasAnySpeaker) {
        return flowingHtml(segments).replace('<p>', `<p style="${pStyle}">`);
    }

    return blocks
        .map((block) => {
            const body = flowingHtml(block.segments).replace('<p>', `<p style="${pStyle}">`);
            if (block.speakerId) {
                const name = escapeHtml(block.speakerName?.trim() || 'Người nói');
                return `<p style="${nameStyle}">${name}</p>${body}`;
            }
            return body;
        })
        .join('');
}
