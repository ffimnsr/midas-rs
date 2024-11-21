-- # Put the your SQL below migration seperator.
-- !UP

create table if not exists map_jobs_to_job_tags (
  job_id varchar(24) not null references jobs(id) on delete cascade,
  job_tag_id bigint not null references job_tags(id) on delete cascade,
  primary key (job_id, job_tag_id)
);

alter table map_jobs_to_job_tags enable row level security;

create policy "all jobs to job tags mapping are viewable by everyone." on map_jobs_to_job_tags
for select using (true);

create policy "organization members can insert data into jobs to job tags mapping." on map_jobs_to_job_tags
for insert with check (public.is_job_under_current_user_organization(job_id));

create policy "organization members can update data from jobs to job tags mapping." on map_jobs_to_job_tags
for update using (public.is_job_under_current_user_organization(job_id));

create policy "organization members can delete data from jobs to job tags mapping." on map_jobs_to_job_tags
for delete using (public.is_job_under_current_user_organization(job_id));

-- !DOWN

drop table if exists map_jobs_to_job_tags;
