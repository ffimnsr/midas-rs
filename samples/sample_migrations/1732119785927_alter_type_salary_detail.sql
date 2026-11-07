-- # Put the your SQL below migration seperator.
-- !UP

create type new_salary_detail as (
  upper_limit text,
  lower_limit text,
  currency text,
  timeframe salary_timeframe
);

alter table jobs add column new_salary new_salary_detail;
alter table jobs drop column salary;
alter table jobs rename column new_salary to salary;
drop type salary_detail;
alter type new_salary_detail rename to salary_detail;
alter table jobs alter column salary set default row('5', '10', 'USD', 'hourly')::salary_detail;
update jobs set salary = row('5', '10', 'USD', 'hourly')::salary_detail where salary is null;


-- !DOWN

-- No down migration for this migration
