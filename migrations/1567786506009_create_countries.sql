-- # Put the your SQL below migration seperator.
-- !UP

CREATE TABLE IF NOT EXISTS countries (
  id SERIAL PRIMARY KEY,
  name TEXT,
  code VARCHAR(4) UNIQUE,
  idd_code VARCHAR(4),
  currency VARCHAR(4),
  status SMALLINT,
  created_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ,
  UNIQUE(code)
);

-- !DOWN

DROP TABLE countries;
