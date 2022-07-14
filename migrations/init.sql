CREATE TABLE IF NOT EXISTS nomenclatures
(
    nm_id int8 PRIMARY KEY,
    old_pics_count int2 NOT NULL,
    new_pics_count int2 NOT NULL,
    good_links TEXT NOT NULL,
    in_process BOOLEAN NOT NULL,
    is_finished BOOLEAN NOT NULL,
    retries int8 NOT NULL
    -- date        timestamp NOT NULL DEFAULT NOW()
);

--DELETE FROM public.nomenclatures;
--SELECT * FROM public.nomenclatures;

CREATE TABLE IF NOT EXISTS shard_10
(
    nm_id int8 PRIMARY KEY,
    old_pics_count int2 NOT NULL
);

-- 

alter table shard_1
add column new_pics_count int2 NULL DEFAULT NULL,
add column in_process boolean NOT NULL DEFAULT false,
add column is_finished boolean NOT NULL DEFAULT false,
add column retries int8 NOT NULL DEFAULT 0,
add column good_links text NOT NULL DEFAULT ''

-- 

update public.shard_10 as n set
    in_process = false,
    is_finished = false,
    retries = 0,
    good_links = ''
from (
    select * from public.shard_10
) as c(nm_id)
where c.nm_id = n.nm_id and in_process = true;

--

select count(*) from shard_10 s 
where 
s.is_finished = true or
(s.retries = 3 and s.error = '')

--

with f as (
	select count(*) as "done" from public.shard_2 s 
	where s.is_finished = true or s.retries = 3
)
, t as (
	select count(*) as "total" from public.shard_2
)
select f."done", t."total" from f
cross join t

--

update public.shard_1 as n set
    in_process = false,
    is_finished = false,
    retries = 0,
    good_links = '',
    new_pics_count = NULL
from (
    select * from public.shard_1
) as c(nm_id)
where c.nm_id = n.nm_id and c.new_pics_count = 0;