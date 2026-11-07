-- # Put the your SQL below migration seperator.
-- !UP

create table if not exists work_functions (
  id serial primary key,
  name text unique,
  status smallint default 1,
  created_at timestamptz default current_timestamp,
  updated_at timestamptz default current_timestamp
);

alter table work_functions enable row level security;

create policy "all work functions are viewable by everyone." on work_functions
for select using (true);

insert into work_functions
  (id, name, status, created_at, updated_at)
values
  (1000, 'Engineering', 1, now(), now()),
  (1001, 'Devops', 1, now(), now()),
  (1002, 'Accounting', 1, now(), now()),
  (1003, 'Legal', 1, now(), now()),
  (1004, 'Marketing', 1, now(), now()),
  (1005, 'Operations', 1, now(), now()),
  (1006, 'Designer', 1, now(), now()),
  (1007, 'Research', 1, now(), now()),
  (1008, 'Sales', 1, now(), now()),
  (1009, 'Support', 1, now(), now()),
  (1010, 'Virtual assistant', 1, now(), now());

-- !DOWN

drop table if exists work_functions;
