-- # Put the your SQL below migration seperator.
-- !UP

grant connect on database startup to e_sofia;
grant usage on schema public to e_sofia;
grant select on table job_applicant_wishlists to e_sofia;
grant update on table job_applicant_wishlists to e_sofia;
grant select on table jobs to e_sofia;
grant select on table organizations to e_sofia;
grant select on table countries to e_sofia;
grant select on table work_industries to e_sofia;
grant select on table work_functions to e_sofia;
grant select on table job_tags to e_sofia;

-- !DOWN

revoke select on table job_tags from e_sofia;
revoke select on table work_functions from e_sofia;
revoke select on table work_industries from e_sofia;
revoke select on table countries from e_sofia;
revoke select on table organizations from e_sofia;
revoke select on table jobs from e_sofia;
revoke update on table job_applicant_wishlists from e_sofia;
revoke select on table job_applicant_wishlists from e_sofia;
revoke usage on schema public from e_sofia;
revoke connect on database startup from e_sofia;
