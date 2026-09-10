export function shouldStopRecordingOnTranscriptionError(isRecording: boolean): boolean {
  return !isRecording;
}

export function isGeminiKeyOrQuotaError(error: string, userMessage: string): boolean {
  const text = `${error} ${userMessage}`.toLowerCase();
  return (
    text.includes('api key') ||
    text.includes('gemini') ||
    text.includes('quota') ||
    text.includes('hạn mức')
  );
}

export function shouldOpenModelSelectorOnTranscriptionError(
  isRecording: boolean,
  error: string,
  userMessage: string,
): boolean {
  if (!isRecording) {
    return true;
  }
  return !isGeminiKeyOrQuotaError(error, userMessage);
}
