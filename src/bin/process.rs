use std::{env::var, time::Duration};

use {
    anyhow::anyhow,
    futures::prelude::*,
    hyper::{Body, Request, Response},
    tokio::{runtime::Builder, time::sleep},
    tower::{BoxError, Service, ServiceBuilder},
    tower_http::follow_redirect::FollowRedirectLayer,
    tracing::{error, info, trace, Level},
};

use {
    cd1574 as q,
    q::store::{models::Nomenclature, sqlx_queue, sqlx_queue::Queue},
    q::worker::pool::spawn,
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
        let conn = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .build();

        ServiceBuilder::new()
            .timeout(Duration::from_secs(30))
            .layer(FollowRedirectLayer::new())
            .service(
                hyper::Client::builder()
                    .retry_canceled_requests(true)
                    .build(conn),
            )
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
    C: Service<
            Request<http_body::Full<hyper::body::Bytes>>,
            Response = Response<Body>,
            Error = BoxError,
        > + Clone
        + Send
        + Sync
        + 'static,
    C::Future: Send,
{
    loop {
        let mut nms = match queue.pull(&shard, WORKERS as i64).await {
            Ok(nms) => nms,
            Err(err) => {
                error!("work>queue.pull: {err}");
                sleep(Duration::from_millis(500)).await;
                Vec::new()
            }
        };

        if nms.is_empty() {
            trace!("[nms] is empty");
            sleep(Duration::from_millis(500)).await;
            continue;
        }

        trace!("{} had been fetched", nms.len());

        unsafe {
            if DEBUG_FLAG {
                nms.iter_mut()
                    .for_each(|nm| nm.good_links = "rawrxd".to_string());
                info!("debug mode");
                let finished: Vec<_> = nms
                    .into_iter()
                    .map(|nm| (nm.nm_id, 3, nm.good_links))
                    .collect();
                if !finished.is_empty() {
                    info!("{} nomenclatures were finished", finished.len());
                    if let Err(err) = queue.batch_finish_jobs(&shard, finished).await {
                        error!("batch_fail_jobs {}", err);
                    }
                };
                continue;
            }
        }

        let (tx, rx) = spawn(WORKERS, worker_fn, client.clone());

        tokio::spawn(async move {
            for nm in nms {
                if let Err(err) = tx.send_async(nm).await {
                    error!("error occured while trying to send in spawned pool channel {err}");
                };
            }
        });

        let mut finished = Vec::new();
        let mut errors = Vec::new();

        let mut stream = rx.into_stream();
        while let Some((nm_id, result)) = stream.next().await {
            match result {
                Ok(res) => finished.push(res),
                Err(e) => {
                    error!("process error: {e}");
                    errors.push(nm_id);
                }
            }
        }

        if !finished.is_empty() {
            info!("{} nomenclatures were finished", finished.len());
            if let Err(err) = queue.batch_finish_jobs(&shard, finished).await {
                error!("batch_finish_jobs {}", err);
            }
        };

        if !errors.is_empty() {
            info!("{} nomenclatures were failed", errors.len());
            if let Err(err) = queue.batch_fail_jobs(&shard, errors).await {
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
    C: Service<
            Request<http_body::Full<hyper::body::Bytes>>,
            Response = Response<Body>,
            Error = BoxError,
        > + Clone
        + Send
        + 'static,
    C::Future: Send,
{
    let links = generate_urls_by_nm_id(&nm.nm_id, &nm.old_pics_count);
    trace!("generated links: {:?}", links);

    let n = links.len();
    let nm_id = nm.nm_id;
    let statuses = stream::iter(links)
        .enumerate()
        .map(|(idx, link)| {
            let client = cli.clone();
            tokio::spawn(async move { (idx, do_request(client, &link).await) })
        })
        .buffer_unordered(n);

    let rawr = statuses
        .filter_map(|r| async move {
            match r {
                Ok(x) => Some(x),
                Err(_) => None,
            }
        })
        .collect::<Vec<(usize, Result<bool, anyhow::Error>)>>()
        .await;

    let mut result: Vec<String> = Vec::new();
    let mut indexes = Vec::new();

    for (image_idx, res) in rawr {
        match res {
            Ok(true) => {
                let bucket = nm_id / 10000 * 10000;
                result.push(format_url(
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
                return (nm.nm_id, Err(e));
            }
        };
    }

    trace!("nm {} indexes {:?}", nm.nm_id, indexes);

    let new_pics_count = get_pics_count(indexes);
    trace!("nm {} new_pics_count {:?}", nm.nm_id, new_pics_count);
    let good_links = result.join(";");
    trace!("nm {} good_links {:?}", nm.nm_id, good_links);

    (nm.nm_id, Ok((nm.nm_id, new_pics_count, good_links)))
}

async fn do_request<C>(mut client: C, url: &str) -> anyhow::Result<bool>
where
    C: Service<
        Request<http_body::Full<hyper::body::Bytes>>,
        Response = Response<Body>,
        Error = BoxError,
    >,
{
    trace!("do_request > {url}");
    let response = client
        .call(Request::head(url).body(http_body::Full::new(hyper::body::Bytes::new()))?)
        .await
        .map_err(|err| anyhow!("{err}: {url}"))?;

    match response.status() {
        hyper::StatusCode::OK => {
            trace!("{url} status_code::ok");
            Ok(true)
        }
        hyper::StatusCode::NOT_FOUND => {
            trace!("{url} status_code::not_found");
            Ok(false)
        }
        _ => {
            trace!("{url} status_code::отличается от ожидаемого");
            return Err(anyhow!("{url} response.status {}", response.status()));
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
        let conn = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .build();

        ServiceBuilder::new()
            .timeout(Duration::from_secs(5))
            .layer(FollowRedirectLayer::new())
            .service(
                hyper::Client::builder()
                    .retry_canceled_requests(true)
                    .build(conn),
            )
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
