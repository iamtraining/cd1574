use futures::Future;
use hyper::{Body, Request, Response};
use tower::{BoxError, Service};

pub fn spawn<G, T, F, R, C>(
    cap: usize,
    generator: G,
    render: C,
) -> (flume::Sender<T>, flume::Receiver<R>)
where
    G: Fn(T, C) -> F + Clone + Send + Sync + 'static,
    F: Future<Output = R> + Send + 'static,
    T: Send + 'static,
    R: Send + 'static,
    C: Service<
            Request<http_body::Full<hyper::body::Bytes>>,
            Response = Response<Body>,
            Error = BoxError,
        > + Clone
        + Send
        + 'static,
    C::Future: Send,
{
    // println!("spawn");
    let (work_tx, work_rx) = flume::bounded(cap + 1);
    let (res_tx, res_rx) = flume::bounded(cap + 1);
    for _ in 1..cap {
        tokio::spawn(worker(
            generator.clone(),
            work_rx.clone(),
            res_tx.clone(),
            render.clone(),
        ));
    }
    tokio::spawn(worker(generator, work_rx, res_tx, render));

    (work_tx, res_rx)
}

async fn worker<G, T, F, R, C>(generator: G, rx: flume::Receiver<T>, tx: flume::Sender<R>, cli: C)
where
    G: Fn(T, C) -> F + Sync,
    F: Future<Output = R>,
    T: Send + 'static,
    R: Send + 'static,
    C: Service<
            Request<http_body::Full<hyper::body::Bytes>>,
            Response = Response<Body>,
            Error = BoxError,
        > + Clone
        + Send
        + 'static,
    C::Future: Send,
{
    // println!("worker");
    while let Ok(item) = rx.recv_async().await {
        // println!("worker1");
        let render = cli.clone();
        // println!("worker2");
        if tx.send_async(generator(item, render).await).await.is_err() {
            // println!("worker3");
            break;
        }
        // println!("worker4");
    }
}
