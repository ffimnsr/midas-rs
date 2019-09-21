-- # Put the your SQL below migration seperator.
-- !UP

CREATE TABLE IF NOT EXISTS users (
  id SERIAL PRIMARY KEY,
  username VARCHAR(90) UNIQUE,
  email VARCHAR(120) UNIQUE,
  email_confirmed BOOL DEFAULT false,
  password_hash TEXT,
  created_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ
);

-- !DOWN

DROP TABLE users;
