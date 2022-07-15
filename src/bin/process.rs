use std::{env::var, time::Duration};

use flume::SendError;
use futures::stream::FuturesUnordered;

use {
    anyhow::anyhow,
    futures::prelude::*,
    reqwest::{Request, Response},
    tokio::{runtime::Builder, time::sleep},
    tower::{service_fn, BoxError, Service, ServiceBuilder},
    tracing::{error, info, trace, Level},
};

use {
    cd1574 as q,
    q::store::{models::Nomenclature, sqlx_queue, sqlx_queue::Queue},
};

const JPG: &str = "jpg";
static SIZES: &[(&str, bool)] = &[
    // ("big", true),
    // ("large", false),
    // ("c516x688", true),
    // ("square", true),
    // ("c324x432", false),
    // ("c252x336", false),
    // ("c246x328", true),
    // ("small", false),
    // ("tm", false),
    ("big", false),
];

const WORKERS: usize = 20;
const LOG_LEVEL: Level = Level::TRACE;

const POSTGRES_DB: &str = "POSTGRES_DB";
const POSTGRES_USER: &str = "POSTGRES_USER";
const POSTGRES_PASSWORD: &str = "POSTGRES_PASSWORD";
const POSTGRES_HOST: &str = "POSTGRES_HOST";
const POSTGRES_PORT: &str = "POSTGRES_PORT";

const DEBUG: &str = "DEBUG";

const SHARD: &str = "SHARD";

static mut DEBUG_FLAG: bool = true;

fn main() -> anyhow::Result<()> {
    let rt = Builder::new_multi_thread().enable_all().build()?;
    let _guard = rt.enter();
    init_tracing();
    info!("process.rs started");

    let q_user = var(POSTGRES_USER).unwrap_or_else(|_| String::from("content"));
    let q_password = var(POSTGRES_PASSWORD).unwrap_or_else(|_| String::from("1231"));
    let q_host = var(POSTGRES_HOST).unwrap_or_else(|_| String::from("localhost"));
    let q_port = var(POSTGRES_PORT).unwrap_or_else(|_| String::from("5433"));
    let q_database = var(POSTGRES_DB).unwrap_or_else(|_| String::from("content"));

    let shard = var(SHARD).unwrap_or_else(|_| String::from("shard_1"));

    unsafe {
        DEBUG_FLAG = var(DEBUG)
            .unwrap_or_else(|_| String::from("true"))
            .parse::<bool>()?;
    }

    info!("user=[{q_user}]");
    info!("password=[{q_password}]");
    info!("host=[{q_host}]");
    info!("port=[{q_port}]");
    info!("database=[{q_database}]");
    info!("==============");
    info!("shard=[{shard}]");
    unsafe {
        info!("DEBUG_FLAG=[{DEBUG_FLAG}]");
    }

    let queue = rt.block_on(async {
        sqlx_queue::SqlxPool::new(q_host, q_port, q_user, q_password, q_database).await
    })?;

    let cli = {
        let svc = reqwest::Client::builder().build().unwrap();
        ServiceBuilder::new()
            .timeout(Duration::from_secs(30))
            .service(service_fn(move |req| svc.execute(req)))
    };

    rt.spawn({
        async move {
            process(shard, queue, cli).await;
        }
    });

    // для теста секционирования
    // rt.block_on(async { sleep(Duration::from_secs(180)).await });

    rt.block_on(async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for event");
    });

    info!("received ctrl-c event");

    Ok(())
}

async fn process<C>(shard: String, queue: impl Queue, client: C)
where
    C: Service<Request, Response = Response, Error = BoxError> + Clone + Send + 'static,
    C::Future: Send,
{
    loop {
        let nms = match queue.pull(&shard, WORKERS as i64).await {
            Ok(nms) => nms,
            Err(err) => {
                error!("work>queue.pull: {err}");
                sleep(Duration::from_millis(500)).await;
                Vec::new()
            }
        };

        println!("{:?}", nms);

        if nms.is_empty() {
            trace!("[nms] is empty");
            sleep(Duration::from_millis(500)).await;
            continue;
        }

        trace!("{} had been fetched", nms.len());

        let (finished_tx, finished_rx) = flume::bounded(3000);
        let (failed_tx, failed_rx) = flume::bounded(3000);

        stream::iter(nms)
            .for_each_concurrent(WORKERS, |nm| async {
                // println!("here");
                let client = client.clone();
                let (nm, result) = worker_fn(nm, client).await;
                match result {
                    Ok(res) => {
                        if let Err(SendError(id)) = finished_tx.send_async(res).await {
                            error!("sending error {:?}", id);
                        };
                    }
                    Err(err) => {
                        error!("run_worker>handle_nm[{nm}]: {err}");
                        if let Err(SendError(nm)) = failed_tx.send_async(nm).await {
                            error!("sending error {}", nm);
                        };
                    }
                };
            })
            .await;
        drop(finished_tx);
        drop(failed_tx);

        let finished_nms = finished_rx.into_iter().collect::<Vec<(i64, i16, String)>>();
        if !finished_nms.is_empty() {
            info!("{} nomenclatures were finished", finished_nms.len());
            if let Err(err) = queue.batch_finish_jobs(&shard, finished_nms).await {
                error!("batch_finish_jobs {}", err);
            }
        };

        let failed_nms = failed_rx.into_iter().collect::<Vec<i64>>();
        if !failed_nms.is_empty() {
            info!("{} nomenclatures were failed", failed_nms.len());
            if let Err(err) = queue.batch_fail_jobs(&shard, failed_nms).await {
                error!("batch_fail_jobs {}", err);
            }
        };

        sleep(Duration::from_millis(100)).await;
    }
}

fn init_tracing() {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{filter, fmt, registry};

    let targets = filter::Targets::new().with_target("process", LOG_LEVEL);
    let fmt = fmt::layer()
        .with_target(false)
        .with_file(true)
        .with_line_number(true);

    if atty::is(atty::Stream::Stdout) {
        registry().with(fmt).with(targets).init();
    } else {
        registry().with(fmt.json()).with(targets).init()
    }
}

async fn worker_fn<C>(nm: Nomenclature, cli: C) -> (i64, anyhow::Result<(i64, i16, String)>)
where
    C: Service<Request, Response = Response, Error = BoxError> + Clone + Send + 'static,
    C::Future: Send,
{
    let links = generate_urls_by_nm_id(&nm.nm_id, &nm.old_pics_count);
    // trace!("generated links: {:?}", links);

    let n = links.len();
    let nm_id = nm.nm_id;

    let mut futures: FuturesUnordered<_> = (0..n)
        .map(|idx| {
            let client = cli.clone();
            let future = do_request(idx, client, links[idx].clone());
            tokio::spawn(future)
        })
        .collect();

    let mut res: Vec<String> = Vec::new();
    let mut indexes = Vec::new();

    while let Some(join_result) = futures.next().await {
        let (image_idx, result) = match join_result {
            Ok(r) => r,
            Err(e) => {
                error!("cant join to handle {e}");
                continue;
            }
        };
        match result {
            Ok(true) => {
                let bucket = nm_id / 10000 * 10000;
                res.push(format_url(
                    SIZES[0].0,
                    bucket,
                    &nm_id,
                    (image_idx + 1) as i16,
                    JPG,
                ));
                indexes.push((image_idx + 1) as i16);
            }
            Ok(false) => (),
            Err(e) => {
                error!("rawr error: {e}");
                return (nm.nm_id, Err(anyhow!(e.to_string())));
            }
        };
    }

    // println!("worker_fn 1");

    // trace!("nm {} indexes {:?}", nm.nm_id, indexes);

    let new_pics_count = get_pics_count(indexes);
    // trace!("nm {} new_pics_count {:?}", nm.nm_id, new_pics_count);
    let good_links = res.join(";");
    // trace!("nm {} good_links {:?}", nm.nm_id, good_links);

    (nm.nm_id, Ok((nm.nm_id, new_pics_count, good_links)))
}

async fn do_request<C>(pic_idx: usize, mut client: C, url: String) -> (usize, anyhow::Result<bool>)
where
    C: Service<Request, Response = Response, Error = BoxError>,
{
    // trace!("do_request started {url}");
    let request = match reqwest::Client::new().get(&url).build() {
        Ok(r) => r,
        Err(e) => return (pic_idx, Err(anyhow::anyhow!("{e}"))),
    };

    let resp = match client.call(request).await {
        Ok(r) => r,
        Err(e) => return (pic_idx, Err(anyhow::anyhow!("url={url} - err={e}"))),
    };

    match resp.status() {
        hyper::StatusCode::OK => {
            // trace!("do_request {url} status_code::ok");
            (pic_idx, Ok(true))
        }
        hyper::StatusCode::NOT_FOUND => {
            // trace!("do_request {url} status_code::not_found");
            (pic_idx, Ok(false))
        }
        _ => {
            // trace!("do_request {url} status_code::отличается от ожидаемого");
            return (pic_idx, Err(anyhow!("{url} response.status")));
        }
    }
}

fn generate_urls_by_nm_id(nm_id: &i64, pics_count: &i16) -> Vec<String> {
    let bucket = nm_id / 10000 * 10000;
    let mut image_urls = Vec::new();
    for (t, avif) in SIZES {
        for i in 1..=*pics_count {
            image_urls.push(format_url(t, bucket, nm_id, i, JPG));
        }
    }
    image_urls
}

fn format_url(t: &str, bucket: i64, nm_id: &i64, i: i16, ext: &str) -> String {
    format!(
        "https://images.wbstatic.net/{}/new/{}/{}-{}.{}",
        t, bucket, nm_id, i, ext
    )
}

fn get_pics_count(mut data: Vec<i16>) -> i16 {
    if data.is_empty() {
        return 0;
    }
    data.sort_unstable();
    let mut latest = -1;
    data.into_iter().enumerate().fold(0, |mut accum, (i, v)| {
        if i as i32 - 1 == latest && v == accum + 1 {
            latest = i as i32;
            accum = v;
        }
        accum
    })
}

#[test]
fn test_pics_count() {
    let data = vec![1, 2, 3, 4, 5, 6];
    let pics_count = get_pics_count(data);
    assert_eq!(6, pics_count);

    let data = vec![4, 5, 1, 3];
    let pics_count = get_pics_count(data);
    assert_eq!(1, pics_count);

    let data = vec![7];
    let pics_count = get_pics_count(data);
    assert_eq!(0, pics_count);
}

#[tokio::test]
async fn worker_test() {
    let cli = {
        let svc = reqwest::Client::builder().build().unwrap();
        ServiceBuilder::new()
            .timeout(Duration::from_secs(30))
            .service(service_fn(move |req| svc.execute(req)))
    };

    let nm = Nomenclature {
        nm_id: 91249210,
        old_pics_count: 10,
        new_pics_count: None,
        good_links: "".to_string(),
        in_process: false,
        is_finished: false,
        retries: 0,
    };

    let res = worker_fn(nm, cli.clone()).await;
    println!("{:?}", res);
}

// 64023641  3
// https://images.wbstatic.net/big/new/64020000/64023641-1.jpg
// https://images.wbstatic.net/big/new/64020000/64023641-2.jpg
// https://images.wbstatic.net/big/new/64020000/64023641-3.jpg
