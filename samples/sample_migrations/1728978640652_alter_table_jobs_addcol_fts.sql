-- # Put the your SQL below migration seperator.
-- !UP

alter table jobs
add column fts tsvector
generated always
as (to_tsvector('english', description || ' ' || title)) stored;

create index jobs_fts_idx on jobs using gin (fts);

-- !DOWN

drop index jobs_fts_idx;
alter table jobs drop column fts;
