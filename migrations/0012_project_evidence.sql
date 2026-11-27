CREATE TABLE household_labels (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL REFERENCES households(id),
    kind text NOT NULL CHECK (kind IN ('domain', 'project_type', 'event_type', 'role', 'skill', 'tag')),
    name text NOT NULL CHECK (btrim(name) <> '' AND octet_length(name) <= 200),
    retired_at timestamptz,
    UNIQUE (id, household_id, kind)
);
CREATE UNIQUE INDEX household_labels_names ON household_labels (household_id, kind, lower(name));

CREATE FUNCTION add_default_evidence_labels(target_household uuid) RETURNS void
LANGUAGE sql AS $$
    INSERT INTO household_labels (household_id, kind, name) VALUES
        (target_household, 'role', 'Participant'), (target_household, 'role', 'Host'),
        (target_household, 'role', 'Teacher'), (target_household, 'role', 'Helper'),
        (target_household, 'role', 'Observer'),
        (target_household, 'skill', 'Explain an idea'), (target_household, 'skill', 'Test and improve'),
        (target_household, 'skill', 'Solve with evidence'), (target_household, 'skill', 'Work with others'),
        (target_household, 'skill', 'Lead and serve'), (target_household, 'skill', 'Organize my work');
$$;

SELECT add_default_evidence_labels(id) FROM households;

CREATE FUNCTION new_household_evidence_labels() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM add_default_evidence_labels(NEW.id);
    RETURN NEW;
END;
$$;
CREATE TRIGGER household_evidence_labels AFTER INSERT ON households
    FOR EACH ROW EXECUTE FUNCTION new_household_evidence_labels();

CREATE TABLE projects (
    id uuid PRIMARY KEY,
    household_id uuid NOT NULL REFERENCES households(id),
    created_by uuid NOT NULL,
    title text NOT NULL CHECK (btrim(title) <> '' AND octet_length(title) <= 200),
    type_id uuid,
    type_kind text GENERATED ALWAYS AS ('project_type') STORED,
    question text,
    success_criterion text,
    state text NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'completed', 'paused')),
    UNIQUE (id, household_id),
    FOREIGN KEY (created_by, household_id) REFERENCES actors(id, household_id),
    FOREIGN KEY (type_id, household_id, type_kind) REFERENCES household_labels(id, household_id, kind)
);

CREATE TABLE project_domains (
    project_id uuid NOT NULL,
    household_id uuid NOT NULL,
    label_id uuid NOT NULL,
    kind text GENERATED ALWAYS AS ('domain') STORED,
    PRIMARY KEY (project_id, label_id),
    FOREIGN KEY (project_id, household_id) REFERENCES projects(id, household_id),
    FOREIGN KEY (label_id, household_id, kind) REFERENCES household_labels(id, household_id, kind)
);

CREATE TABLE project_moments (
    project_id uuid NOT NULL,
    moment_id uuid NOT NULL,
    household_id uuid NOT NULL,
    scope_owner uuid GENERATED ALWAYS AS ('00000000-0000-0000-0000-000000000000'::uuid) STORED,
    linked_by uuid NOT NULL,
    withdrawn_at timestamptz,
    PRIMARY KEY (project_id, moment_id),
    UNIQUE (project_id, moment_id, household_id),
    FOREIGN KEY (project_id, household_id) REFERENCES projects(id, household_id),
    FOREIGN KEY (moment_id, household_id, scope_owner) REFERENCES moments(id, household_id, scope_owner),
    FOREIGN KEY (linked_by, household_id) REFERENCES actors(id, household_id)
);
CREATE INDEX project_moments_by_moment ON project_moments (moment_id, project_id);

CREATE TABLE moment_attempts (
    moment_id uuid PRIMARY KEY,
    household_id uuid NOT NULL,
    project_id uuid NOT NULL,
    revision integer NOT NULL DEFAULT 1,
    UNIQUE (moment_id, household_id),
    FOREIGN KEY (project_id, moment_id, household_id) REFERENCES project_moments(project_id, moment_id, household_id)
);

CREATE TABLE attempt_revisions (
    moment_id uuid NOT NULL,
    household_id uuid NOT NULL,
    revision integer NOT NULL CHECK (revision > 0),
    editor_id uuid NOT NULL,
    event text,
    event_type_id uuid,
    event_kind text GENERATED ALWAYS AS ('event_type') STORED,
    outcome text,
    my_part text,
    others_part text,
    help text,
    checking text,
    prior_attempt_id uuid,
    PRIMARY KEY (moment_id, revision),
    UNIQUE (moment_id, revision, household_id),
    CHECK (prior_attempt_id <> moment_id),
    FOREIGN KEY (moment_id, household_id) REFERENCES moment_attempts(moment_id, household_id),
    FOREIGN KEY (editor_id, household_id) REFERENCES actors(id, household_id),
    FOREIGN KEY (event_type_id, household_id, event_kind) REFERENCES household_labels(id, household_id, kind),
    FOREIGN KEY (prior_attempt_id, household_id) REFERENCES moment_attempts(moment_id, household_id)
);

ALTER TABLE moment_attempts ADD FOREIGN KEY (moment_id, revision) REFERENCES attempt_revisions(moment_id, revision)
    DEFERRABLE INITIALLY DEFERRED;

CREATE TABLE attempt_roles (
    moment_id uuid NOT NULL,
    revision integer NOT NULL,
    household_id uuid NOT NULL,
    label_id uuid NOT NULL,
    kind text GENERATED ALWAYS AS ('role') STORED,
    PRIMARY KEY (moment_id, revision, label_id),
    FOREIGN KEY (moment_id, revision, household_id) REFERENCES attempt_revisions(moment_id, revision, household_id),
    FOREIGN KEY (label_id, household_id, kind) REFERENCES household_labels(id, household_id, kind)
);

CREATE TABLE moment_skills (
    moment_id uuid NOT NULL,
    label_id uuid NOT NULL,
    household_id uuid NOT NULL,
    learner_id uuid NOT NULL,
    reason text NOT NULL CHECK (btrim(reason) <> '' AND octet_length(reason) <= 4000),
    independence text CHECK (independence IN ('with_help', 'on_my_own')),
    withdrawn_at timestamptz,
    kind text GENERATED ALWAYS AS ('skill') STORED,
    scope_owner uuid GENERATED ALWAYS AS ('00000000-0000-0000-0000-000000000000'::uuid) STORED,
    PRIMARY KEY (moment_id, label_id),
    UNIQUE (moment_id, label_id, household_id),
    FOREIGN KEY (moment_id, household_id, scope_owner) REFERENCES moments(id, household_id, scope_owner),
    FOREIGN KEY (label_id, household_id, kind) REFERENCES household_labels(id, household_id, kind),
    FOREIGN KEY (learner_id, household_id) REFERENCES actors(id, household_id)
);
CREATE INDEX moment_skills_by_skill ON moment_skills (household_id, label_id, moment_id);

CREATE TABLE skill_transfers (
    id uuid NOT NULL,
    household_id uuid NOT NULL,
    learner_id uuid NOT NULL,
    label_id uuid NOT NULL,
    from_project_id uuid NOT NULL,
    from_moment_id uuid NOT NULL,
    to_project_id uuid NOT NULL,
    to_moment_id uuid NOT NULL,
    reason text NOT NULL CHECK (btrim(reason) <> '' AND octet_length(reason) <= 4000),
    withdrawn_at timestamptz,
    PRIMARY KEY (id, household_id),
    CHECK (from_moment_id <> to_moment_id AND from_project_id <> to_project_id),
    UNIQUE (label_id, from_project_id, from_moment_id, to_project_id, to_moment_id),
    FOREIGN KEY (learner_id, household_id) REFERENCES actors(id, household_id),
    FOREIGN KEY (from_project_id, from_moment_id, household_id) REFERENCES project_moments(project_id, moment_id, household_id),
    FOREIGN KEY (to_project_id, to_moment_id, household_id) REFERENCES project_moments(project_id, moment_id, household_id),
    FOREIGN KEY (from_moment_id, label_id, household_id) REFERENCES moment_skills(moment_id, label_id, household_id),
    FOREIGN KEY (to_moment_id, label_id, household_id) REFERENCES moment_skills(moment_id, label_id, household_id)
);
CREATE INDEX skill_transfers_by_skill ON skill_transfers (household_id, label_id, id);
