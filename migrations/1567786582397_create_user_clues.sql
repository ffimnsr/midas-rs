-- # Put the your SQL below migration seperator.
-- !UP

CREATE TABLE IF NOT EXISTS user_clues (
  id SERIAL PRIMARY KEY,
  user_id INTEGER REFERENCES users(id),
  first_name VARCHAR(40),
  last_name VARCHAR(40),
  middle_name VARCHAR(40),
  phone_number VARCHAR(25),
  mobile_number VARCHAR(25),
  gender SMALLINT,
  birth_date DATE,
  tax_identification_no VARCHAR(60),
  created_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ
);

-- !DOWN

DROP TABLE user_clues;
