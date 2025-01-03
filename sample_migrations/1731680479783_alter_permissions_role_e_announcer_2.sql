-- # Put the your SQL below migration seperator.
-- !UP

grant select on table jobs to e_announcer;
grant select on table organizations to e_announcer;
grant select on table countries to e_announcer;
grant select on table work_industries to e_announcer;
grant select on table work_functions to e_announcer;
grant select on table job_tags to e_announcer;

-- !DOWN

revoke select on table job_tags from e_announcer;
revoke select on table work_functions from e_announcer;
revoke select on table work_industries from e_announcer;
revoke select on table countries from e_announcer;
revoke select on table organizations from e_announcer;
revoke select on table jobs from e_announcer;
