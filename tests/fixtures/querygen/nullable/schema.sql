CREATE TYPE user_status AS ENUM ('active', 'inactive', 'banned');

CREATE TABLE users (
    id integer PRIMARY KEY,
    email text NOT NULL,
    status user_status NOT NULL,
    bio text
);
