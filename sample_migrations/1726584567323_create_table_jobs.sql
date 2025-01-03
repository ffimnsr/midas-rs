-- # Put the your SQL below migration seperator.
-- !UP

create type work_experience_level as enum (
  'intern',
  'entry_level',
  'mid_level',
  'senior_level',
  'executive'
);

create type work_contract_type as enum (
  'full_time',
  'part_time',
  'freelance_contract',
  'fixed_term_contract',
  'zero_hour_contract',
  'internship'
);

create type salary_timeframe as enum (
  'hourly',
  'daily',
  'weekly',
  'semi_monthly',
  'monthly',
  'quarterly',
  'annually'
);

create type salary_detail as (
  upper_limit text,
  lower_limit text,
  currency varchar(10),
  timeframe salary_timeframe
);

create table if not exists jobs (
  id varchar(24) primary key,
  title varchar(300) not null,
  description text not null,
  industry_id int references work_industries(id) default 1000,
  country_id int references countries(id) default 1,
  organization_id bigint references organizations(id) on delete set default default 1,
  experience_level work_experience_level default 'intern'::work_experience_level,
  contract_type work_contract_type default 'full_time'::work_contract_type,
  salary salary_detail default ('10', '5', 'USD', 'hourly')::salary_detail,
  has_timetracker bool default false,
  is_remote bool default true,
  is_featured bool default false,
  status smallint default 1,
  created_at timestamptz default current_timestamp,
  updated_at timestamptz default current_timestamp
);

alter table jobs enable row level security;

create policy "all jobs are viewable by everyone." on jobs
for select using (true);

create policy "organization members can insert data into jobs." on jobs
for insert with check (public.is_current_user_member_of_organization(organization_id));

create policy "organization members can update data in jobs." on jobs
for update using (public.is_current_user_member_of_organization(organization_id));

create policy "organization members can delete data in jobs." on jobs
for delete using (public.is_current_user_member_of_organization(organization_id));

create or replace function is_job_under_current_user_organization(job_id varchar)
returns boolean
language plpgsql
as $$
declare
    org_id bigint;
begin
    select organization_id into org_id from jobs where id = job_id;
    return is_current_user_member_of_organization(org_id);
end;
$$;

create trigger tr_mod_updated_at
before update on jobs
for each row
execute function moddatetime(updated_at);

-- !DOWN

drop trigger if exists tr_mod_updated_at on jobs;
drop table if exists jobs;
drop type if exists salary_detail;
drop type if exists salary_timeframe;
drop type if exists work_contract_type;
drop type if exists work_experience_level;
