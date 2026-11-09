-- !UP
create table widgets (
  id bigint generated always as identity primary key,
  name text not null
);

-- !DOWN
drop table widgets;
