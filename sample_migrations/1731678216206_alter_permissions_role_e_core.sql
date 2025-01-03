-- # Put the your SQL below migration seperator.
-- !UP

do $$
begin
create role e_core;
exception when duplicate_object then raise notice '%, skipping', sqlerrm using errcode = sqlstate;
end
$$;

grant connect on database startup to e_core;
grant usage on schema public to e_core;
grant sesame_read_startup_data to e_core;
grant sesame_write_startup_data to e_core;

-- !DOWN

revoke sesame_write_startup_data from e_core;
revoke sesame_read_startup_data from e_core;
revoke usage on schema public from e_core;
revoke connect on database startup from e_core;
