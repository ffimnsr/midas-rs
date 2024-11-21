-- # Put the your SQL below migration seperator.
-- !UP

create table if not exists work_industries (
  id serial primary key,
  name text unique,
  status smallint default 1,
  created_at timestamptz default current_timestamp,
  updated_at timestamptz default current_timestamp
);

alter table work_industries enable row level security;

create policy "all work industries are viewable by everyone." on work_industries
for select using (true);

insert into work_industries
  (id, name, status, created_at, updated_at)
values
  (1000, 'Administration and support', 1, now(), now()),
  (1001, 'Architecture and engineering', 1, now(), now()),
  (1002, 'Art and design', 1, now(), now()),
  (1003, 'Business and finance operations', 1, now(), now()),
  (1004, 'Community and social services', 1, now(), now()),
  (1005, 'Computer and technology', 1, now(), now()),
  (1006, 'Education', 1, now(), now()),
  (1007, 'Legal', 1, now(), now()),
  (1008, 'Marketing', 1, now(), now()),
  (1009, 'Sales', 1, now(), now()),
  (1010, 'Other professional services', 1, now(), now());

-- !DOWN

drop table if exists work_industries;
