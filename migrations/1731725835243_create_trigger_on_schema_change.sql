-- # Put the your SQL below migration seperator.
-- !UP

-- watch CREATE and ALTER
create or replace function pgrst_ddl_watch() returns event_trigger as $$
declare
  cmd record;
begin
  for cmd in select * from pg_event_trigger_ddl_commands()
  loop
    if cmd.command_tag in (
      'create schema', 'alter schema'
    , 'create table', 'create table as', 'select into', 'alter table'
    , 'create foreign table', 'alter foreign table'
    , 'create view', 'alter view'
    , 'create materialized view', 'alter materialized view'
    , 'create function', 'alter function'
    , 'create trigger'
    , 'create type', 'alter type'
    , 'create rule'
    , 'comment'
    )
    -- don't notify in case of create temp table or other objects created on pg_temp
    and cmd.schema_name is distinct from 'pg_temp'
    then
      notify pgrst, 'reload schema';
    end if;
  end loop;
end; $$ language plpgsql;

-- watch DROP
create or replace function pgrst_drop_watch() returns event_trigger as $$
declare
  obj record;
begin
  for obj in select * from pg_event_trigger_dropped_objects()
  loop
    if obj.object_type in (
      'schema'
    , 'table'
    , 'foreign table'
    , 'view'
    , 'materialized view'
    , 'function'
    , 'trigger'
    , 'type'
    , 'rule'
    )
    and obj.is_temporary is false -- no pg_temp objects
    then
      notify pgrst, 'reload schema';
    end if;
  end loop;
end; $$ language plpgsql;

create event trigger pgrst_ddl_watch
  on ddl_command_end
  execute procedure pgrst_ddl_watch();

create event trigger pgrst_drop_watch
  on sql_drop
  execute procedure pgrst_drop_watch();

-- !DOWN

drop event trigger if exists pgrst_drop_watch;
drop event trigger if exists pgrst_ddl_watch;
drop function if exists pgrst_drop_watch();
drop function if exists pgrst_ddl_watch();
