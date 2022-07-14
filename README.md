## docker-compose file
### up
`docker-compose --project-name="catalog" up -d`

### down
`docker-compose --project-name="catalog" down --remove-orphans`

## test
[http://localhost:3000](http://localhost:3000)

[http://localhost:5050](http://localhost:5050)

## scaling a worker replica set
`docker-compose --project-name="catalog" up --no-recreate --scale shard-1=1 --scale shard-2=1 --scale shard-3=1 --scale shard-4=1 --scale shard-5=1 --scale shard-6=1 --scale shard-7=1 --scale shard-8=1 --scale shard-9=1 --scale shard-10=1 -d`

`docker-compose --project-name="catalog" up --no-recreate --scale shard-1=1 --scale shard-2=0 --scale shard-3=0 --scale shard-4=0 --scale shard-5=0 --scale shard-6=0 --scale shard-7=0 --scale shard-8=0 --scale shard-9=0 --scale shard-10=0 -d`