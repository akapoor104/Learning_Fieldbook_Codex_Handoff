CREATE TABLE family_photo_inbox (
    asset_id uuid PRIMARY KEY,
    household_id uuid NOT NULL,
    learner_id uuid NOT NULL,
    activity_date date,
    added_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (asset_id, household_id) REFERENCES assets(id, household_id),
    FOREIGN KEY (learner_id, household_id) REFERENCES actors(id, household_id)
);
CREATE INDEX family_photo_inbox_learner ON family_photo_inbox (household_id, learner_id, added_at DESC, asset_id);
CREATE INDEX moment_assets_by_asset ON moment_assets (asset_id);

-- Existing attachments were linked at capture time. The recorder supplied those links.
ALTER TABLE moment_assets ADD COLUMN matched_by uuid;
ALTER TABLE moment_assets ADD FOREIGN KEY (matched_by, household_id) REFERENCES actors(id, household_id);
ALTER TABLE moment_assets ALTER COLUMN position TYPE integer;

CREATE TABLE family_photo_match_events (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL,
    asset_id uuid NOT NULL REFERENCES family_photo_inbox(asset_id),
    moment_id uuid NOT NULL,
    actor_id uuid NOT NULL,
    matched boolean NOT NULL,
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (asset_id, household_id) REFERENCES assets(id, household_id),
    FOREIGN KEY (moment_id, household_id) REFERENCES moments(id, household_id),
    FOREIGN KEY (actor_id, household_id) REFERENCES actors(id, household_id)
);
