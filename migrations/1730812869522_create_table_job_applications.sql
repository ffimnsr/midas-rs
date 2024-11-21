-- # Put the your SQL below migration seperator.
-- !UP

create table if not exists job_applications (
  id varchar(24) primary key,
  job_id varchar(24) not null references jobs(id) on delete cascade,
  name varchar(300) not null,
  email varchar(100) not null,
  linkedin_url text not null,
  cv_url text not null,
  telegram_username varchar(100) not null,
  telegram_photo_url text not null,
  telegram_language_code varchar(10) not null,
  telegram_first_name varchar(100) not null,
  telegram_last_name varchar(100) not null,
  status smallint default 1,
  created_at timestamptz default current_timestamp,
  updated_at timestamptz default current_timestamp
);

alter table job_applications enable row level security;

create policy "all job applications are viewable by everyone." on job_applications
for select using (true);

create policy "everybody can insert data into job applications." on job_applications
for insert with check (true);

create trigger tr_mod_updated_at
before update on job_applications
for each row
execute function moddatetime(updated_at);

-- !DOWN

drop trigger if exists tr_mod_updated_at on job_applications;
drop table if exists job_applications;
