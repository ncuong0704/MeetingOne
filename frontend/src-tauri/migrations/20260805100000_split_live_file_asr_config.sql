-- Live path ASR config
ALTER TABLE transcript_settings ADD COLUMN liveModel TEXT;
ALTER TABLE transcript_settings ADD COLUMN liveAsrVariant TEXT;
ALTER TABLE transcript_settings ADD COLUMN liveDecodingMethod TEXT;
ALTER TABLE transcript_settings ADD COLUMN liveNumActivePaths INTEGER;
ALTER TABLE transcript_settings ADD COLUMN liveMaxSegmentSeconds INTEGER;

-- File path ASR config
ALTER TABLE transcript_settings ADD COLUMN fileModel TEXT;
ALTER TABLE transcript_settings ADD COLUMN fileAsrVariant TEXT;
ALTER TABLE transcript_settings ADD COLUMN fileDecodingMethod TEXT;
ALTER TABLE transcript_settings ADD COLUMN fileNumActivePaths INTEGER;
ALTER TABLE transcript_settings ADD COLUMN fileMaxSegmentSeconds INTEGER;
ALTER TABLE transcript_settings ADD COLUMN fileRoverEnabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE transcript_settings ADD COLUMN fileRoverFamilyB TEXT;
ALTER TABLE transcript_settings ADD COLUMN fileRoverVariantB TEXT;

-- Backfill from legacy columns
UPDATE transcript_settings SET
  liveModel = model,
  liveAsrVariant = asrVariant,
  liveDecodingMethod = decodingMethod,
  liveNumActivePaths = numActivePaths,
  liveMaxSegmentSeconds = maxSegmentSeconds,
  fileModel = model,
  fileAsrVariant = asrVariant,
  fileDecodingMethod = decodingMethod,
  fileNumActivePaths = numActivePaths,
  fileMaxSegmentSeconds = maxSegmentSeconds,
  fileRoverEnabled = roverEnabled,
  fileRoverFamilyB = roverFamilyB,
  fileRoverVariantB = roverVariantB
WHERE id = '1';
