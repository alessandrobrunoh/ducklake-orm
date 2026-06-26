-- First migration: create the users table.
CREATE TABLE main.users (
    id       BIGINT PRIMARY KEY,
    username VARCHAR NOT NULL,
    email    VARCHAR NOT NULL
);
