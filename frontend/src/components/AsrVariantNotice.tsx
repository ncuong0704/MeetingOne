'use client';

import { asrVariantNotice } from './asrSettingsConstants';
import type { AsrModelFamily, AsrPathKind, ModelVariant } from '@/lib/asr';

export function AsrVariantNotice({
  family,
  variant,
  path,
}: {
  family: AsrModelFamily;
  variant: ModelVariant;
  path: AsrPathKind;
}) {
  const notice = asrVariantNotice(family, variant, path);
  if (!notice) return null;

  return (
    <>
      <p className="text-xs text-ink-2 leading-snug">{notice.description}</p>
      {notice.warning ? (
        <p className="text-xs text-ink-2 leading-snug">{notice.warning}</p>
      ) : null}
    </>
  );
}
