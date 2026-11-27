CREATE TABLE moment_confirmations (
    moment_id uuid NOT NULL,
    household_id uuid NOT NULL,
    revision integer NOT NULL,
    learner_id uuid NOT NULL,
    confirmed_at timestamptz NOT NULL DEFAULT now(),
    withdrawn_at timestamptz,
    PRIMARY KEY (moment_id, revision),
    FOREIGN KEY (moment_id, revision) REFERENCES moment_revisions(moment_id, revision),
    FOREIGN KEY (moment_id, household_id) REFERENCES moments(id, household_id),
    FOREIGN KEY (learner_id, household_id) REFERENCES actors(id, household_id)
);

CREATE TABLE moment_comments (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL,
    moment_id uuid NOT NULL,
    author_id uuid NOT NULL,
    submission_key uuid NOT NULL,
    kind text NOT NULL CHECK (kind IN ('comment', 'suggestion')),
    body text NOT NULL CHECK (btrim(body) <> '' AND octet_length(body) <= 4000),
    created_at timestamptz NOT NULL DEFAULT now(),
    withdrawn_at timestamptz,
    FOREIGN KEY (moment_id, household_id) REFERENCES moments(id, household_id),
    FOREIGN KEY (author_id, household_id) REFERENCES actors(id, household_id),
    UNIQUE (author_id, submission_key)
);
CREATE INDEX moment_comments_order ON moment_comments (moment_id, created_at, id);
