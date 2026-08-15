import { ASR_MODELS, AsrModelFamily, ModelVariant } from '@/lib/asr';

export const DEFAULT_FAMILY: AsrModelFamily = 'zipformer-vi-30m';
export const DEFAULT_LIVE_FAMILY: AsrModelFamily = 'zipformer-vi-30m-streaming';
export const DEFAULT_VARIANT: ModelVariant = 'int8';
export const DEFAULT_DECODING = 'modified_beam_search' as const;
export const DEFAULT_PATHS = 15;
export const DEFAULT_MAX_SEGMENT_SECONDS = 25;
export const MIN_MAX_SEGMENT_SECONDS = 5;
export const MAX_MAX_SEGMENT_SECONDS = 30;
export const DEFAULT_CAPU_PUNCTUATION_LEVEL = 7;
export const DEFAULT_CAPU_CASE_LEVEL = 3;
export const FALLBACK_PHYSICAL_CORES = 4;

export const LEVEL_LABELS: Record<number, string> = {
  1: 'Rất ít',
  3: 'Ít',
  5: 'Vừa',
  7: 'Nhiều',
  10: 'Rất nhiều',
};

export const levelLabel = (v: number) => LEVEL_LABELS[v] ?? String(v);

export const VARIANT_OPTIONS: { id: ModelVariant; label: string }[] = [
  { id: 'int8', label: 'int8 (quantized)' },
  { id: 'full', label: 'full (precision)' },
];

export function parseAsrFamily(id: string | undefined | null): AsrModelFamily {
  if (
    id === 'zipformer-vi-30m' ||
    id === 'gipformer-65m-rnnt' ||
    id === 'sherpa-onnx-zipformer-vi-2025-04-20' ||
    id === 'zipformer-vi-30m-streaming'
  ) {
    return id;
  }
  return DEFAULT_FAMILY;
}

export function resolveVariantForFamily(
  family: AsrModelFamily,
  variant: ModelVariant
): ModelVariant {
  const info = ASR_MODELS.find((m) => m.id === family);
  if (!info) return variant;
  return info.availableVariants.includes(variant) ? variant : info.availableVariants[0];
}
