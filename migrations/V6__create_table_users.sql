CREATE TABLE users (
    id bigserial PRIMARY KEY,
    username text NOT NULL UNIQUE CHECK (char_length(username) BETWEEN 3 AND 32),
    password_hash text NOT NULL,
    roles text[] NOT NULL DEFAULT '{user}',
    created_at timestamp NOT NULL DEFAULT NOW(),
    updated_at timestamp NOT NULL DEFAULT NOW()
);
