CREATE TABLE users (
    id integer PRIMARY KEY,
    email text NOT NULL
);

CREATE TABLE posts (
    id integer PRIMARY KEY,
    user_id integer NOT NULL REFERENCES users(id),
    title text NOT NULL
);
