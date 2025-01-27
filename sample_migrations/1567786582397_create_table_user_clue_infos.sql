-- # Put the your SQL below migration seperator.
-- !UP

create table if not exists user_clue_infos (
  id bigint generated by default as identity primary key,
  user_id bigint references users(id) on delete cascade,
  gender smallint,
  birth_date date,
  tax_identification_no varchar(60),
  created_at timestamptz default current_timestamp,
  updated_at timestamptz default current_timestamp
);

alter table user_clue_infos enable row level security;

create policy "all user infos are viewable by everyone." on user_clue_infos
for select using (true);

create policy "users can insert their own profile info." on user_clue_infos
for insert with check (public.get_current_user_id() = user_id);

create policy "users can update own profile info." on user_clue_infos
for update using (public.get_current_user_id() = user_id);

create trigger tr_mod_updated_at
before update on user_clue_infos
for each row
execute function moddatetime(updated_at);

-- !DOWN

drop trigger if exists tr_mod_updated_at on user_clue_infos;
drop table if exists user_clue_infos;
