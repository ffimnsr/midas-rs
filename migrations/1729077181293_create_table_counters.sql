-- # Put the your SQL below migration seperator.
-- !UP

create table if not exists counters (
    name text primary key,
    total_count bigint not null
);

alter table counters enable row level security;

create policy "all counters are viewable by everyone." on counters
for select using (true);

create policy "everybody can insert data into counters." on counters
for insert with check (true);

create policy "everybody can update data in counters." on counters
for update using (true);

create or replace function fn_inc_job_count()
returns trigger
language plpgsql
as $$
begin
    insert into counters (name, total_count)
    values ('job_count', 1)
    on conflict(name)
    do update set total_count = counters.total_count + 1;
    return new;
end;
$$;

create or replace function fn_dec_job_count()
returns trigger
language plpgsql
as $$
begin
    insert into counters (name, total_count)
    values ('job_count', 0)
    on conflict(name)
    do update set total_count = counters.total_count - 1;
    return old;
end;
$$;

create trigger tr_inc_job_count
after insert on jobs
for each row
execute function fn_inc_job_count();

create trigger tr_dec_job_count
after delete on jobs
for each row
execute function fn_dec_job_count();

-- !DOWN

drop trigger if exists tr_dec_job_count on jobs;
drop trigger if exists tr_inc_job_count on jobs;
drop function if exists fn_dec_job_count();
drop function if exists fn_inc_job_count();
drop table if exists counters;
