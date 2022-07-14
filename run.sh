# export POSTGRES_DB=content
# export POSTGRES_USER=content
# export POSTGRES_PASSWORD=1231
# export POSTGRES_HOST=localhost
# export POSTGRES_PORT=5433

export POSTGRES_DB=content
export POSTGRES_USER=content
export POSTGRES_PASSWORD=1231
export POSTGRES_HOST=images-2.miningfarm.vm.prod-ocp.cloud.3data
export POSTGRES_PORT=5433


export SHARD=shard_1
export DEBUG=false 
export RENDER_ADDR=http://render-go-ingress-controller.render-go.svc.k8s.dataline
export RENDER_TOKEN=35a3da62-f4a4-4bdf-9e85-8d0f1ad2c9a2

cargo run --bin process