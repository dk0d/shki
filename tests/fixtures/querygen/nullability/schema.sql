CREATE TABLE users (
    id integer PRIMARY KEY,
    email text NOT NULL,
    bio text
);

CREATE TYPE marker AS ENUM ('retain', 'shared', 'deactivated');

CREATE TABLE attributes (
    id bigint PRIMARY KEY,
    name text NOT NULL,
    annotation text,
    tag marker,
    meta jsonb,
    scores bigint[],
    verified_at timestamptz,
    ref_id uuid
);
