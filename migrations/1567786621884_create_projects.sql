-- # Put the your SQL below migration seperator.
-- !UP

CREATE TABLE IF NOT EXISTS projects (
  id SERIAL PRIMARY KEY,
  employer_id INTEGER REFERENCES users(id),
  code VARCHAR(60) UNIQUE,
  name VARCHAR(255),
  start_date DATE,
  end_date DATE,
  status SMALLINT,
  created_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ,
  UNIQUE(code)
);

-- !DOWN

DROP TABLE projects;
