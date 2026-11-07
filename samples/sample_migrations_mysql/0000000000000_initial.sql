-- # Put the your SQL below migration seperator.
-- !UP
CREATE TABLE IF NOT EXISTS users (id SERIAL PRIMARY KEY);

-- !DOWN
-- DROP TABLE users;
