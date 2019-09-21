-- # Put the your SQL below migration seperator.
-- !UP

CREATE TABLE IF NOT EXISTS bank_accounts (
  id SERIAL PRIMARY KEY,
  user_id INTEGER REFERENCES users(id),
  account_name VARCHAR(60),
  account_no VARCHAR(40),
  bank_address TEXT,
  bank_branch TEXT,
  swift_iban_code VARCHAR(90),
  created_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ
);


-- !DOWN

DROP TABLE bank_accounts;
