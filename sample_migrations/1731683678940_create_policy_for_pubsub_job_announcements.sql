-- # Put the your SQL below migration seperator.
-- !UP

create policy "all pub sub job announcements are viewable by everyone." on pubsub_job_announcements
for select using (true);

create policy "everybody can modify from pub sub job announcements." on pubsub_job_announcements
for update using (true);

-- !DOWN

drop policy "everybody can modify from pub sub job announcements." on pubsub_job_announcements;
drop policy "all pub sub job announcements are viewable by everyone." on pubsub_job_announcements;
