-- # Put the your SQL below migration seperator.
-- !UP

create table if not exists user_leaderboard_ranks (
  id serial primary key,
  name text unique,
  status smallint default 1,
  created_at timestamptz default current_timestamp,
  updated_at timestamptz default current_timestamp
);

alter table user_leaderboard_ranks enable row level security;

create policy "all user leaderboard ranks are viewable by everyone." on user_leaderboard_ranks
for select using (true);

insert into user_leaderboard_ranks
  (name, status, created_at, updated_at)
values
  ('Warrior', 1, now(), now()),
  ('Elite', 1, now(), now()),
  ('Master', 1, now(), now()),
  ('Grandmaster', 1, now(), now()),
  ('Epic', 1, now(), now()),
  ('Legend', 1, now(), now()),
  ('Mythic', 1, now(), now()),
  ('Mythical Glory', 1, now(), now()),
  ('Immortal', 1, now(), now());

-- !DOWN

drop table if exists user_leaderboard_ranks;
