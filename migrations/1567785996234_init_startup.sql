-- # Put the your SQL below migration seperator.
-- !UP

create extension if not exists moddatetime;
create extension if not exists pgcrypto;

-- disable default execution of functions by PUBLIC (all roles)
-- https://docs.postgrest.org/en/v12/explanations/db_authz.html#functions
alter default privileges in schema public revoke execute on functions from PUBLIC;

create role sesame_su_startup_data nologin;
grant all privileges on database startup to sesame_su_startup_data;
grant all privileges on schema public to sesame_su_startup_data;
grant all privileges on all tables in schema public to sesame_su_startup_data;
grant all privileges on all sequences in schema public to sesame_su_startup_data;
grant all privileges on all functions in schema public to sesame_su_startup_data;
alter default privileges in schema public grant all on tables to sesame_su_startup_data;
alter default privileges in schema public grant all on sequences to sesame_su_startup_data;
alter default privileges in schema public grant all on functions to sesame_su_startup_data;
alter default privileges in schema public grant all on types to sesame_su_startup_data;

create role sesame_read_startup_data nologin;
grant usage on schema public to sesame_read_startup_data;
grant select on all tables in schema public to sesame_read_startup_data;
grant usage, select on all sequences in schema public to sesame_read_startup_data;
grant execute on all functions in schema public to sesame_read_startup_data;
alter default privileges in schema public grant select on tables to sesame_read_startup_data;
alter default privileges in schema public grant usage, select on sequences to sesame_read_startup_data;
alter default privileges in schema public grant execute on functions to sesame_read_startup_data;

create role sesame_write_startup_data nologin;
grant usage on schema public to sesame_write_startup_data;
grant insert, update, delete on all tables in schema public to sesame_write_startup_data;
grant usage, update on all sequences in schema public to sesame_write_startup_data;
grant execute on all functions in schema public to sesame_write_startup_data;
alter default privileges in schema public grant insert, update, delete on tables to sesame_write_startup_data;
alter default privileges in schema public grant usage, update on sequences to sesame_write_startup_data;
alter default privileges in schema public grant execute on functions to sesame_write_startup_data;

create role webuser nologin;
grant sesame_read_startup_data to webuser;
grant sesame_write_startup_data to webuser;

create role anon nologin;
grant sesame_read_startup_data to anon;

create or replace function get_user_uid()
returns text
language sql stable
as $$
  select
  coalesce(
    nullif(current_setting('request.jwt.claim.sub', true), ''),
    (nullif(current_setting('request.jwt.claims', true), '')::jsonb ->> 'sub')
  )::text
$$;

create or replace function get_user_role()
returns text
language sql stable
as $$
  select
  coalesce(
    nullif(current_setting('request.jwt.claim.role', true), ''),
    (nullif(current_setting('request.jwt.claims', true), '')::jsonb ->> 'role')
  )::text
$$;

create or replace function get_user_email()
returns text
language sql stable
as $$
  select
  coalesce(
    nullif(current_setting('request.jwt.claim.email', true), ''),
    (nullif(current_setting('request.jwt.claims', true), '')::jsonb ->> 'email')
  )::text
$$;

-- !DOWN

drop function if exists get_user_email;
drop function if exists get_user_role;
drop function if exists get_user_uid;

drop role if exists anon;
drop role if exists webuser;
drop owned by sesame_write_startup_data;
drop role if exists sesame_write_startup_data;
drop owned by sesame_read_startup_data;
drop role if exists sesame_read_startup_data;
drop owned by sesame_su_startup_data;
drop role if exists sesame_su_startup_data;

drop extension pgcrypto;
drop extension moddatetime;
