-- # Put the your SQL below migration seperator.
-- !UP

create type pubsub_message_status as enum (
  'pending',
  'delivered'
);

create table if not exists pubsub_job_announcements (
    job_id varchar(24) primary key references jobs(id) on delete cascade,
    message_id int null,
    message_status pubsub_message_status not null default 'pending',
    scheduled_delivery_at timestamptz default current_timestamp,
    delivery_attemps int default 0,
    delivered_at timestamptz null,
    created_at timestamptz default current_timestamp,
    updated_at timestamptz default current_timestamp
);

alter table pubsub_job_announcements enable row level security;

create trigger tr_mod_updated_at
before update on pubsub_job_announcements
for each row
execute function moddatetime(updated_at);

create or replace function fn_notify_job_announcer()
returns trigger
language plpgsql
security definer
as $$
begin
  insert into pubsub_job_announcements (job_id)
  values (new.id);

  perform pg_notify('job_announcements', new.id::text);

  return new;
end;
$$;

create trigger tr_notify_job_announcer
after insert on jobs
for each row
execute function fn_notify_job_announcer();

-- !DOWN

drop trigger if exists tr_notify_job_announcer on jobs;
drop function if exists fn_notify_job_announcer();
drop trigger if exists tr_mod_updated_at on jobs;
drop table if exists pubsub_job_announcements;
drop type if exists pubsub_message_status;
