-- Family rows keep their existing IDs and use a reserved scope owner.
-- A diary scope is identified by the learner's actor ID within the household.
ALTER TABLE moments
    DROP CONSTRAINT moments_visibility_check,
    ADD CONSTRAINT moments_visibility_check CHECK (
        visibility = 'family' OR (visibility = 'diary' AND voice = 'student'
            AND learner_id <> '00000000-0000-0000-0000-000000000000')
    ),
    ADD COLUMN scope_owner uuid GENERATED ALWAYS AS (
        CASE WHEN visibility = 'diary' THEN learner_id ELSE '00000000-0000-0000-0000-000000000000'::uuid END
    ) STORED,
    ADD UNIQUE (id, household_id, scope_owner);

ALTER TABLE assets
    ADD COLUMN scope_owner uuid NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    ADD CHECK (scope_owner = '00000000-0000-0000-0000-000000000000'
        OR (scope_owner = uploader_id AND source_url IS NULL)),
    ADD UNIQUE (id, household_id, scope_owner),
    DROP CONSTRAINT assets_copy_asset_id_household_id_fkey,
    ADD FOREIGN KEY (copy_asset_id, household_id, scope_owner) REFERENCES assets(id, household_id, scope_owner);

-- A paper trail of withdrawn links also stays in its original scope.
ALTER TABLE moment_assets
    ADD COLUMN scope_owner uuid NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    DROP CONSTRAINT moment_assets_moment_id_household_id_fkey,
    DROP CONSTRAINT moment_assets_asset_id_household_id_fkey,
    ADD FOREIGN KEY (moment_id, household_id, scope_owner) REFERENCES moments(id, household_id, scope_owner),
    ADD FOREIGN KEY (asset_id, household_id, scope_owner) REFERENCES assets(id, household_id, scope_owner);

ALTER TABLE family_photo_inbox
    ADD COLUMN scope_owner uuid GENERATED ALWAYS AS ('00000000-0000-0000-0000-000000000000'::uuid) STORED,
    DROP CONSTRAINT family_photo_inbox_asset_id_household_id_fkey,
    ADD FOREIGN KEY (asset_id, household_id, scope_owner) REFERENCES assets(id, household_id, scope_owner);

-- The existing upload and link-copy paths handle family assets only.
ALTER TABLE private_uploads
    ADD COLUMN scope_owner uuid GENERATED ALWAYS AS ('00000000-0000-0000-0000-000000000000'::uuid) STORED,
    DROP CONSTRAINT private_uploads_asset_id_household_id_fkey,
    ADD FOREIGN KEY (asset_id, household_id, scope_owner) REFERENCES assets(id, household_id, scope_owner);

CREATE INDEX moments_scope ON moments (household_id, scope_owner, created_at DESC, id DESC);

-- Use these sources for ordinary family counts, search, matches and exports.
CREATE VIEW family_moments AS
    SELECT * FROM moments WHERE scope_owner = '00000000-0000-0000-0000-000000000000'
    WITH LOCAL CHECK OPTION;
CREATE VIEW family_assets AS
    SELECT * FROM assets WHERE scope_owner = '00000000-0000-0000-0000-000000000000'
    WITH LOCAL CHECK OPTION;
