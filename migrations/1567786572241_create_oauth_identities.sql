-- # Put the your SQL below migration seperator.
-- !UP

CREATE TABLE IF NOT EXISTS oauth_identities (
  id SERIAL PRIMARY KEY,
  user_id INTEGER REFERENCES users(id),
  account_email VARCHAR(60),
  account_id VARCHAR(20),
  provider TEXT,
  created_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ
);

-- !DOWN

DROP TABLE oauth_identities;
