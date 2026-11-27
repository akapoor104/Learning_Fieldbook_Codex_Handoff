CREATE TABLE diary_access_events (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL,
    learner_id uuid NOT NULL,
    guardian_id uuid NOT NULL,
    kind text NOT NULL CHECK (kind IN ('view', 'recovery')),
    recorded_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    CHECK (learner_id <> guardian_id),
    CHECK (expires_at > recorded_at AND expires_at <= recorded_at + interval '5 minutes'),
    FOREIGN KEY (learner_id, household_id) REFERENCES actors(id, household_id),
    FOREIGN KEY (guardian_id, household_id) REFERENCES actors(id, household_id),
    UNIQUE (id, guardian_id, household_id)
);
CREATE INDEX diary_access_history
    ON diary_access_events (household_id, learner_id, recorded_at DESC, id DESC);

CREATE FUNCTION preserve_diary_access_event() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'diary access history is append-only';
END;
$$;
CREATE TRIGGER diary_access_events_append_only
    BEFORE UPDATE OR DELETE OR TRUNCATE ON diary_access_events
    FOR EACH STATEMENT EXECUTE FUNCTION preserve_diary_access_event();

-- The event survives deletion of the session. A session can only
-- refer to its own guardian's event in the same household.
ALTER TABLE sessions
    ADD COLUMN diary_event_id uuid UNIQUE,
    ADD COLUMN diary_previous_hash bytea UNIQUE,
    ADD FOREIGN KEY (diary_event_id, actor_id, household_id)
        REFERENCES diary_access_events(id, guardian_id, household_id),
    ADD CHECK (diary_event_id IS NULL OR elevated_until IS NULL),
    ADD CHECK ((diary_event_id IS NULL) = (diary_previous_hash IS NULL));

-- Both session metadata and protected SELECTs consult the same current scope.
CREATE VIEW active_diary_sessions AS
    SELECT s.token_hash, s.household_id, d.learner_id, d.expires_at
    FROM sessions s
    JOIN diary_access_events d ON d.id = s.diary_event_id
    JOIN actors g ON g.id = s.actor_id AND g.household_id = s.household_id AND g.role = 'guardian'
    JOIN actors l ON l.id = d.learner_id AND l.household_id = s.household_id AND l.role = 'learner'
    WHERE least(s.idle_until, s.expires_at, d.expires_at) > statement_timestamp();

ALTER TABLE webauthn_ceremonies
    DROP CONSTRAINT webauthn_ceremonies_purpose_check,
    DROP CONSTRAINT webauthn_ceremonies_check,
    ADD CHECK (purpose IN ('enroll', 'sign-in', 'step-up', 'diary')),
    ADD CHECK ((purpose IN ('step-up', 'diary')) = (session_hash IS NOT NULL));
