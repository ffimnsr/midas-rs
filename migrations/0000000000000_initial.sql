-- # Put the your SQL below migration seperator.
-- !UP
-- CREATE TABLE IF NOT EXISTS users (id SERIAL PRIMARY KEY);
-- CREATE TABLE IF NOT EXISTS users (id SERIAL PRIMARY KEY);

-- CREATE TABLE IF NOT EXISTS users (id SERIAL PRIMARY KEY);
-- CREATE TABLE IF NOT EXISTS users (id SERIAL PRIMARY KEY);
create table if not exists countries (
  id serial primary key,
  name text,
  code varchar(4) unique,
  idd_code varchar(4),
  currency varchar(4),
  status smallint default 1,
  created_at timestamptz default current_timestamp,
  updated_at timestamptz default current_timestamp
);
create table hello (id serial primary key, name text);


-- !DOWN
-- DROP table hello;
DROP TABLE countries;
-- DROP TABLE users;
