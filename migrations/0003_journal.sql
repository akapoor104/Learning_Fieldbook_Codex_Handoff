-- A row in this table indexes an immutable, finalized private copy.
-- B3 stages uploads separately and publishes a row only after verification.
CREATE TABLE assets (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL REFERENCES households(id),
    uploader_id uuid NOT NULL,
    label text NOT NULL CHECK (btrim(label) <> ''),
    storage_key text NOT NULL UNIQUE CHECK (btrim(storage_key) <> ''),
    byte_count bigint NOT NULL CHECK (byte_count > 0),
    sha256 bytea NOT NULL CHECK (octet_length(sha256) = 32),
    FOREIGN KEY (uploader_id, household_id) REFERENCES actors(id, household_id),
    UNIQUE (id, household_id)
);

CREATE TABLE moments (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL REFERENCES households(id),
    learner_id uuid NOT NULL,
    recorder_id uuid NOT NULL,
    submission_key uuid NOT NULL,
    submission_hash bytea NOT NULL CHECK (octet_length(submission_hash) = 32),
    voice text NOT NULL CHECK (voice IN ('student', 'scribed', 'guardian')),
    visibility text NOT NULL DEFAULT 'family' CHECK (visibility = 'family'),
    created_at timestamptz NOT NULL DEFAULT now(),
    revision integer NOT NULL DEFAULT 1 CHECK (revision > 0),
    CHECK ((voice = 'student') = (learner_id = recorder_id)),
    FOREIGN KEY (learner_id, household_id) REFERENCES actors(id, household_id),
    FOREIGN KEY (recorder_id, household_id) REFERENCES actors(id, household_id),
    UNIQUE (recorder_id, submission_key),
    UNIQUE (id, household_id)
);
CREATE INDEX moments_household ON moments (household_id, created_at DESC, id DESC);

CREATE TABLE moment_revisions (
    moment_id uuid NOT NULL,
    household_id uuid NOT NULL,
    revision integer NOT NULL CHECK (revision > 0),
    editor_id uuid NOT NULL,
    note text CHECK (note IS NULL OR btrim(note) <> ''),
    activity_date date,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (moment_id, revision),
    FOREIGN KEY (moment_id, household_id) REFERENCES moments(id, household_id),
    FOREIGN KEY (editor_id, household_id) REFERENCES actors(id, household_id)
);
ALTER TABLE moments ADD CONSTRAINT moments_current_revision
    FOREIGN KEY (id, revision) REFERENCES moment_revisions(moment_id, revision)
    DEFERRABLE INITIALLY DEFERRED;

CREATE TABLE moment_assets (
    moment_id uuid NOT NULL,
    household_id uuid NOT NULL,
    asset_id uuid NOT NULL,
    position smallint NOT NULL CHECK (position >= 0),
    PRIMARY KEY (moment_id, position),
    UNIQUE (moment_id, asset_id),
    FOREIGN KEY (moment_id, household_id) REFERENCES moments(id, household_id),
    FOREIGN KEY (asset_id, household_id) REFERENCES assets(id, household_id)
);
