-- # Put the your SQL below migration seperator.
-- !UP

create table if not exists countries (
  id serial primary key,
  name text,
  code varchar(4) unique,
  idd_code varchar(4),
  currency varchar(4),
  status smallint default 1,
  created_at timestamptz default current_timestamp,
  updated_at timestamptz default current_timestamp
);

alter table countries enable row level security;

create policy "all countries are viewable by everyone." on countries
for select using (true);

insert into countries
  (name, code, idd_code, currency)
values
  ('Australia', 'au', '61', 'aud'),
  ('Austria', 'at', '43', 'eur'),
  ('Belgium', 'be', '32', 'eur'),
  ('Brazil', 'br', '55', 'brl'),
  ('Canada', 'ca', '1', 'cad'),
  ('Chile', 'cl', '56', 'clp'),
  ('China', 'cn', '86', 'cny'),
  ('Czech Republic', 'cz', '420', 'czk'),
  ('Denmark', 'dk', '45', 'dkk'),
  ('Estonia', 'ee', '372', 'eur'),
  ('Finland', 'fi', '358', 'eur'),
  ('France', 'fr', '33', 'eur'),
  ('Germany', 'de', '49', 'eur'),
  ('Greece', 'gr', '30', 'eur'),
  ('Hungary', 'hu', '36', 'huf'),
  ('Iceland', 'is', '354', 'isk'),
  ('India', 'in', '91', 'inr'),
  ('Ireland', 'ie', '353', 'eur'),
  ('Israel', 'il', '972', 'ils'),
  ('Italy', 'it', '39', 'eur'),
  ('Japan', 'jp', '81', 'jpy'),
  ('Latvia', 'lv', '371', 'eur'),
  ('Lithuania', 'lt', '370', 'eur'),
  ('Luxembourg', 'lu', '352', 'eur'),
  ('Malaysia', 'my', '60', 'myr'),
  ('Mexico', 'mx', '52', 'mxn'),
  ('Netherlands', 'nl', '31', 'eur'),
  ('New Zealand', 'nz', '64', 'nzd'),
  ('Norway', 'no', '47', 'nok'),
  ('Philippines', 'ph', '63', 'php'),
  ('Poland', 'pl', '48', 'pln'),
  ('Portugal', 'pt', '351', 'eur'),
  ('Russia', 'ru', '7', 'rub'),
  ('Singapore', 'sg', '65', 'sgd'),
  ('Slovakia', 'sk', '421', 'eur'),
  ('Slovenia', 'si', '386', 'eur'),
  ('South Africa', 'za', '27', 'zar'),
  ('South Korea', 'kr', '82', 'krw'),
  ('Spain', 'es', '34', 'eur'),
  ('Sweden', 'se', '46', 'sek'),
  ('Switzerland', 'ch', '41', 'chf'),
  ('Thailand', 'th', '66', 'thb'),
  ('Turkey', 'tr', '90', 'try'),
  ('United Arab Emirates', 'ae', '971', 'aed'),
  ('United Kingdom', 'uk', '44', 'gbp'),
  ('United States', 'us', '1', 'usd'),
  ('Vietnam', 'vn', '84', 'vnd');

-- !DOWN

drop table if exists countries;
