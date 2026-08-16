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
      <p className="text-xs text-gray-500 dark:text-gray-400">{notice.description}</p>
      {notice.warning ? (
        <p className="text-xs text-amber-700 dark:text-amber-400 bg-amber-50 dark:bg-amber-900/20 px-3 py-2 rounded-md">
          {notice.warning}
        </p>
      ) : null}
    </>
  );
}
