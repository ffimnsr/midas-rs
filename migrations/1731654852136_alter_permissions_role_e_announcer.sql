-- # Put the your SQL below migration seperator.
-- !UP

grant connect on database startup to e_announcer;
grant usage on schema public to e_announcer;
grant select on table pubsub_job_announcements to e_announcer;
grant update on table pubsub_job_announcements to e_announcer;

-- !DOWN

revoke update on table pubsub_job_announcements from e_announcer;
revoke select on table pubsub_job_announcements from e_announcer;
revoke usage on schema public from e_announcer;
revoke connect on database startup from e_announcer;
