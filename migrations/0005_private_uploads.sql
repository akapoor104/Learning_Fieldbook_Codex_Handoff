ALTER TABLE assets ADD COLUMN media_type text NOT NULL DEFAULT 'application/octet-stream';

CREATE TABLE private_uploads (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL,
    uploader_id uuid NOT NULL,
    submission_key uuid NOT NULL,
    label text NOT NULL CHECK (btrim(label) <> ''),
    byte_count bigint NOT NULL CHECK (byte_count BETWEEN 1 AND 8589934592),
    media_type text NOT NULL,
    sha256 bytea NOT NULL CHECK (octet_length(sha256) = 32),
    received bigint NOT NULL DEFAULT 0 CHECK (received >= 0 AND received <= byte_count),
    asset_id uuid,
    staging_expires_at timestamptz DEFAULT (clock_timestamp() + interval '24 hours'),
    CHECK (asset_id IS NULL OR (asset_id = id AND received = byte_count)),
    CHECK (asset_id IS NOT NULL OR staging_expires_at IS NOT NULL),
    FOREIGN KEY (uploader_id, household_id) REFERENCES actors(id, household_id),
    FOREIGN KEY (asset_id, household_id) REFERENCES assets(id, household_id),
    UNIQUE (uploader_id, submission_key)
);
CREATE INDEX private_uploads_staging_expiry ON private_uploads (staging_expires_at)
    WHERE staging_expires_at IS NOT NULL;
