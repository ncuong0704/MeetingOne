import { invoke } from '@tauri-apps/api/core';

export interface QualityMetrics {
  dnsmos_sig: number;
  dnsmos_bak: number;
  dnsmos_ovrl: number;
  asr_confidence: number;
  sample_text: string;
  duration_analyzed: number;
  num_segments: number;
}

export interface AnalysisResult {
  metrics: QualityMetrics;
  suggestions: string[];
  is_ready: boolean;
  error_message: string | null;
}

export interface MicQualityProgress {
  phase: string;
  percent: number;
}

export async function micQualityIsModelReady(): Promise<boolean> {
  return invoke<boolean>('mic_quality_is_model_ready');
}

export async function micQualityDownloadModel(): Promise<void> {
  return invoke('mic_quality_download_model');
}

export async function micQualityAnalyze(deviceName: string | null): Promise<AnalysisResult> {
  return invoke<AnalysisResult>('mic_quality_analyze', { deviceName });
}

export async function micQualityCancel(): Promise<void> {
  return invoke('mic_quality_cancel');
}

export function dnsmosLabel(score: number): string {
  if (score >= 4) return 'Tốt';
  if (score >= 3) return 'Khá';
  if (score >= 2) return 'Trung bình';
  return 'Kém';
}

export function dnsmosColorClass(score: number): string {
  if (score >= 4) return 'text-green-600';
  if (score >= 3) return 'text-amber-500';
  if (score >= 2) return 'text-orange-500';
  return 'text-red-600';
}

export function confidenceLabel(confidence: number): string {
  if (confidence >= 0.85) return 'Xuất sắc';
  if (confidence >= 0.75) return 'Tốt';
  if (confidence >= 0.6) return 'Trung bình';
  return 'Kém';
}

export function confidenceColorClass(confidence: number): string {
  if (confidence >= 0.85) return 'text-green-600';
  if (confidence >= 0.75) return 'text-amber-500';
  if (confidence >= 0.6) return 'text-orange-500';
  return 'text-red-600';
}

export function stripSuggestionEmoji(text: string): string {
  return text.replace(/^[🔴🟡🟢✅]\s*/, '').trim();
}
