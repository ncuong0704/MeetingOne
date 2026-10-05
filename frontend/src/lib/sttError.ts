export function shouldStopRecordingOnTranscriptionError(isRecording: boolean): boolean {
  return !isRecording;
}

export function shouldOpenModelSelectorOnTranscriptionError(): boolean {
  return true;
}
