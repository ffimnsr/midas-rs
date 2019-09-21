-- # Put the your SQL below migration seperator.
-- !UP

CREATE TABLE IF NOT EXISTS project_clues (
  id SERIAL PRIMARY KEY,
  project_id INTEGER REFERENCES projects(id),
  description TEXT,
  repo_http_url TEXT,
  repo_ssh_url TEXT,
  web_url TEXT,
  created_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ
);

-- !DOWN

DROP TABLE project_clues;
