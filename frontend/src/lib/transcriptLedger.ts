import type { Transcript, TranscriptUpdate } from '../types/index.ts';

/** Incrementally ordered history. Partial edits do not re-sort all segments. */
export class TranscriptLedger {
  private records = new Map<number, Transcript>();
  private order: number[] = [];
  private cached: Transcript[] | null = null;

  has(sequence: number): boolean { return this.records.has(sequence); }

  clear(): void {
    this.records.clear();
    this.order = [];
    this.cached = null;
  }

  upsert(update: TranscriptUpdate): void {
    const previous = this.records.get(update.sequence_id);
    if (previous && !previous.is_partial && update.is_partial) return;
    const record: Transcript = {
      ...previous, ...update, id: previous?.id ?? `seg_${update.sequence_id}`,
      text: previous?.user_edited ? previous.text : update.text,
      speaker_name: update.speaker_name ?? previous?.speaker_name,
      speaker_color: update.speaker_color ?? previous?.speaker_color,
    };
    this.records.set(update.sequence_id, record);
    if (!previous || previous.chunk_start_time !== record.chunk_start_time) {
      if (previous) this.order.splice(this.order.indexOf(update.sequence_id), 1);
      let lo = 0, hi = this.order.length;
      while (lo < hi) {
        const mid = (lo + hi) >>> 1;
        const other = this.records.get(this.order[mid])!;
        const diff = (other.chunk_start_time ?? 0) - (record.chunk_start_time ?? 0);
        if (diff < 0 || (diff === 0 && this.order[mid] < update.sequence_id)) lo = mid + 1;
        else hi = mid;
      }
      this.order.splice(lo, 0, update.sequence_id);
    }
    this.cached = null;
  }

  edit(sequence: number, text: string): void {
    const record = this.records.get(sequence);
    if (record) {
      this.records.set(sequence, { ...record, text, user_edited: true });
      this.cached = null;
    }
  }

  snapshot(): Transcript[] {
    return this.cached ??= this.order.map((sequence) => this.records.get(sequence)!);
  }
}
