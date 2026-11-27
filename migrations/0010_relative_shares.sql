-- A rematched or restored attachment must be selected again for a guest.
ALTER TABLE moment_assets ADD COLUMN link_generation uuid NOT NULL DEFAULT gen_random_uuid();

CREATE TABLE relative_shares (
    id uuid NOT NULL,
    household_id uuid NOT NULL,
    scope_owner uuid GENERATED ALWAYS AS ('00000000-0000-0000-0000-000000000000'::uuid) STORED,
    moment_id uuid NOT NULL,
    revision integer NOT NULL,
    include_text boolean NOT NULL,
    guest_id uuid NOT NULL,
    approved_by uuid NOT NULL,
    invitation_expires_at timestamptz NOT NULL DEFAULT (clock_timestamp() + interval '24 hours'),
    accepted_at timestamptz,
    access_expires_at timestamptz,
    revoked_at timestamptz,
    CHECK ((accepted_at IS NULL) = (access_expires_at IS NULL)),
    FOREIGN KEY (moment_id, household_id, scope_owner) REFERENCES moments(id, household_id, scope_owner),
    FOREIGN KEY (moment_id, revision) REFERENCES moment_revisions(moment_id, revision),
    FOREIGN KEY (guest_id, household_id) REFERENCES actors(id, household_id),
    FOREIGN KEY (approved_by, household_id) REFERENCES actors(id, household_id),
    PRIMARY KEY (id, household_id)
);
CREATE INDEX relative_shares_revision ON relative_shares (moment_id, revision);

CREATE TABLE relative_share_originals (
    share_id uuid NOT NULL,
    household_id uuid NOT NULL,
    scope_owner uuid GENERATED ALWAYS AS ('00000000-0000-0000-0000-000000000000'::uuid) STORED,
    asset_id uuid NOT NULL,
    link_generation uuid NOT NULL,
    position integer NOT NULL,
    PRIMARY KEY (share_id, household_id, asset_id),
    FOREIGN KEY (share_id, household_id) REFERENCES relative_shares(id, household_id),
    FOREIGN KEY (asset_id, household_id, scope_owner) REFERENCES assets(id, household_id, scope_owner)
);

CREATE VIEW active_relative_shares AS
    SELECT * FROM relative_shares
    WHERE accepted_at IS NOT NULL AND revoked_at IS NULL AND access_expires_at > statement_timestamp();

CREATE VIEW active_relative_originals AS
    SELECT selection.share_id, selection.household_id, a.id, a.storage_key, a.label, a.uploader_id, selection.position
    FROM relative_share_originals selection
    JOIN active_relative_shares s ON s.id = selection.share_id AND s.household_id = selection.household_id
    JOIN moment_assets link ON link.moment_id = s.moment_id AND link.asset_id = selection.asset_id
        AND link.link_generation = selection.link_generation AND link.withdrawn_at IS NULL
    JOIN family_assets a ON a.id = selection.asset_id AND a.household_id = s.household_id
        AND a.source_url IS NULL;
