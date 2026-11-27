CREATE TABLE actors (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL REFERENCES households(id),
    username text NOT NULL UNIQUE CHECK (username ~ '^[a-z0-9][a-z0-9._-]{0,62}$'),
    display_name text NOT NULL CHECK (btrim(display_name) <> ''),
    role text NOT NULL CHECK (role IN ('learner', 'guardian', 'guest')),
    credential_generation bigint NOT NULL DEFAULT 0,
    UNIQUE (id, household_id)
);

CREATE TABLE passkeys (
    actor_id uuid PRIMARY KEY REFERENCES actors(id) ON DELETE CASCADE,
    credential_id bytea NOT NULL UNIQUE,
    signature_counter bigint NOT NULL DEFAULT 0 CHECK (signature_counter >= 0),
    credential jsonb NOT NULL
);

CREATE TABLE enrollment_grants (
    actor_id uuid PRIMARY KEY REFERENCES actors(id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE,
    expires_at timestamptz NOT NULL
);

CREATE TABLE sessions (
    token_hash bytea PRIMARY KEY,
    actor_id uuid NOT NULL,
    household_id uuid NOT NULL,
    FOREIGN KEY (actor_id, household_id) REFERENCES actors(id, household_id) ON DELETE CASCADE,
    idle_until timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    elevated_until timestamptz
);
CREATE INDEX sessions_actor ON sessions (actor_id);
CREATE INDEX sessions_idle_until ON sessions (idle_until);

CREATE TABLE webauthn_ceremonies (
    token_hash bytea PRIMARY KEY,
    actor_id uuid NOT NULL REFERENCES actors(id) ON DELETE CASCADE,
    generation bigint NOT NULL,
    purpose text NOT NULL CHECK (purpose IN ('enroll', 'sign-in', 'step-up')),
    session_hash bytea REFERENCES sessions(token_hash) ON DELETE CASCADE,
    CHECK ((purpose = 'step-up') = (session_hash IS NOT NULL)),
    challenge_state jsonb NOT NULL,
    expires_at timestamptz NOT NULL
);
CREATE INDEX webauthn_ceremonies_expiry ON webauthn_ceremonies (expires_at);
CREATE INDEX webauthn_ceremonies_actor ON webauthn_ceremonies (actor_id);
CREATE INDEX webauthn_ceremonies_session ON webauthn_ceremonies (session_hash);
