-- !UP
create table embedded_widgets (
  id integer primary key,
  name text not null
);
-- !DOWN
drop table embedded_widgets;
