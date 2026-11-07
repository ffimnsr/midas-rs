-- # Put the your SQL below migration seperator.
-- !UP

create table if not exists job_applicant_wishlists (
  telegram_id text not null,
  job_id varchar(24) not null references jobs(id) on delete cascade,
  status smallint not null default 1,
  created_at timestamp with time zone not null default now(),
  updated_at timestamp with time zone not null default now(),
  primary key (telegram_id, job_id)
);

alter table job_applicant_wishlists enable row level security;

create policy "everyone can view job applicant wishlists." on job_applicant_wishlists
for select using (true);

create policy "anyone can insert on job applicant wishlists." on job_applicant_wishlists
for insert with check (true);

create policy "anyone can update on job applicant wishlists." on job_applicant_wishlists
for update using (true);

create trigger tr_mod_updated_at
before update on job_applicant_wishlists
for each row
execute function moddatetime(updated_at);

-- !DOWN

drop trigger if exists tr_mod_updated_at on job_applicant_wishlists;
drop policy if exists "anyone can update on job applicant wishlists." on job_applicant_wishlists;
drop policy if exists "anyone can insert on job applicant wishlists." on job_applicant_wishlists;
drop policy if exists "everyone can view job applicant wishlists." on job_applicant_wishlists;
drop table if exists job_applicant_wishlists;
