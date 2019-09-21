-- # Put the your SQL below migration seperator.
-- !UP

CREATE TABLE IF NOT EXISTS project_issues (
  id SERIAL PRIMARY KEY,
  project_id INTEGER REFERENCES projects(id),
  code VARCHAR(60) UNIQUE,
  overview TEXT,
  description TEXT,
  assignee_id INTEGER REFERENCES users(id),
  reporter_id INTEGER REFERENCES users(id),
  assigned_at TIMESTAMPTZ,
  reported_at TIMESTAMPTZ,
  priority SMALLINT,
  status SMALLINT,
  created_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ,
  UNIQUE(code)
);

-- !DOWN

DROP TABLE project_issues;
