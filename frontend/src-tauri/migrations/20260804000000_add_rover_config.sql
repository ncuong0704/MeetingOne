ALTER TABLE transcript_settings ADD COLUMN roverEnabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE transcript_settings ADD COLUMN roverFamilyB TEXT;
ALTER TABLE transcript_settings ADD COLUMN roverVariantB TEXT;
