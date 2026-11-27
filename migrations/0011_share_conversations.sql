ALTER TABLE relative_shares ADD UNIQUE (id, household_id, moment_id);
ALTER TABLE moment_comments
    ADD COLUMN share_id uuid,
    ADD COLUMN feedback_writer text,
    ADD COLUMN feedback_date date,
    ADD COLUMN feedback_asset_id uuid,
    ADD FOREIGN KEY (share_id, household_id, moment_id) REFERENCES relative_shares(id, household_id, moment_id),
    ADD FOREIGN KEY (feedback_asset_id, household_id) REFERENCES assets(id, household_id),
    ADD CHECK (feedback_writer IS NOT NULL OR (feedback_date IS NULL AND feedback_asset_id IS NULL)),
    ADD CHECK (share_id IS NULL OR (feedback_writer IS NULL AND kind = 'comment'));
CREATE INDEX moment_comments_share ON moment_comments (share_id, household_id, created_at);
CREATE INDEX relative_shares_guest ON relative_shares (household_id, guest_id);
