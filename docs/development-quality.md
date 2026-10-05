# Recording architecture and verification

## Persistence rules

- A recording has one UUID shared by Rust, IndexedDB recovery and SQLite. The unique `meetings.recording_session_id` index makes saving a committed session idempotent.
- ASR workers write through a session-owned `TranscriptWriter` before notifying the UI. Removing the global manager during stop does not disconnect transcript persistence.
- Stop closes capture, drains the audio pipeline and recognition workers, waits for the audio saver, applies CAPU, then commits SQLite. UI events notify the frontend after commit. A queryable completion result handles missed events and WebView reloads.
- A failed SQLite save retains the manager for retry. Audio or recognition failures are reported explicitly; successful transcript persistence must not conceal them.
- Capture callbacks never block. Bounded queues apply backpressure downstream. Capture overflow stops capture and reports a fatal error. Queue capacities are in `audio/constants.rs`; the offline worker queue is also bounded.
- JSON snapshots are throttled to at most one per second during recognition. User edits and finalization force a snapshot. This limits disk I/O but leaves a short recovery window between snapshots if the entire process crashes.
- Meeting folders are unique even when titles and start times match. Recovery cleanup validates meeting metadata and checkpoint containment. Checkpoints are removed only after successful audio finalization and SQLite persistence.
- IndexedDB version 2 upserts `(meetingId, sequence_id)`, preserves final results over late partials, and waits for transaction commit. An aborted upgrade preserves version 1 data. Retention never deletes unsaved or audio-pending sessions.

## Database migrations

Published migration files must remain unchanged. Add a new migration for schema changes. Checksum mismatches now stop initialization instead of silently rewriting migration history. Opening failures preserve WAL and SHM files. If an existing installation has mismatched migration history, back up the database and reconcile its schema explicitly before changing migration metadata.

## Local checks

Use Node 22 and pnpm 9.15.9. Commit `frontend/pnpm-lock.yaml`; CI installs with `--frozen-lockfile`.

```powershell
cd frontend
pnpm install --frozen-lockfile
pnpm typecheck
pnpm lint
pnpm test
pnpm build
cd ..
cargo fmt --all -- --check
cargo check -p meetingone --lib --locked
cargo clippy -p meetingone --lib --locked
```

The PR workflow runs frontend checks and selected Rust persistence/recovery tests on Windows. Audio fixtures are generated in memory and encoded with the bundled FFmpeg. These tests do not require microphone access or downloaded recognition models. Native dependencies may still be downloaded during compilation.

## Manual acceptance checks

1. Start a real recording, speak, stop mid-sentence, and verify the last recognized words and audio ending.
2. Stop from the tray and from the window close together; verify exactly one meeting.
3. Reload the WebView during recording and during stop; verify restored transcript state and the saved meeting.
4. Edit a transcript while recording; verify the correction survives ASR updates and final persistence.
5. Simulate FFmpeg failure, then recover the session; verify checkpoints remain until audio recovery succeeds.
6. Simulate SQLite failure, retry stop, and verify one meeting with the same complete audio.
7. Run a long recording on a slow CPU while observing queue pressure, memory, disk usage and stop duration.

## Remaining priorities

- Stress-test real capture and native recognition on Windows, macOS and Linux before release. Automated tests cover persistence contracts, not microphone drivers or timing under production load.
- Reduce the existing React hook dependency warnings and Rust Clippy warnings incrementally. Do not suppress them globally or enable a warnings-as-errors gate until the baseline is clean.
- Profile the meeting-details bundle, then load export/editor features on demand where the UI allows it. The local production build currently reports about 537 kB of first-load JavaScript for this route.
- For large meeting libraries, measure search and listing latency before adding pagination and an indexed search strategy. Unicode-safe snippet generation fixes the panic but does not make SQLite's built-in `LOWER()` Unicode-aware.
- Review API-key storage separately from local developer notes; sensitive settings should use the operating system credential store.

`NOTES.md` remains local, is ignored, and has been removed from the Git index. This does not rewrite previous commits.
