CREATE TABLE households (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    label text NOT NULL CHECK (btrim(label) <> '')
);
