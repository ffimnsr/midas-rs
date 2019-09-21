-- # Put the your SQL below migration seperator.
-- !UP

CREATE TABLE IF NOT EXISTS ask_questions (
  id SERIAL PRIMARY KEY,
  question TEXT,
  answer TEXT,
  created_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ
);

-- !DOWN

DROP TABLE ask_questions;
