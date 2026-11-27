ALTER TABLE assets
    ALTER COLUMN storage_key DROP NOT NULL,
    ALTER COLUMN byte_count DROP NOT NULL,
    ALTER COLUMN sha256 DROP NOT NULL,
    ADD COLUMN source_url text,
    ADD COLUMN link_submission_key uuid,
    ADD COLUMN copy_asset_id uuid,
    ADD FOREIGN KEY (copy_asset_id, household_id) REFERENCES assets(id, household_id),
    ADD UNIQUE (uploader_id, link_submission_key),
    ADD CONSTRAINT assets_source CHECK (
        (source_url IS NULL AND link_submission_key IS NULL AND copy_asset_id IS NULL
            AND storage_key IS NOT NULL AND byte_count IS NOT NULL AND sha256 IS NOT NULL)
        OR
        (source_url IS NOT NULL AND link_submission_key IS NOT NULL
            AND storage_key IS NULL AND byte_count IS NULL AND sha256 IS NULL
            AND copy_asset_id IS DISTINCT FROM id)
    );

-- A withdrawn link keeps its position and identity for restoration.
ALTER TABLE moment_assets ADD COLUMN withdrawn_at timestamptz;
