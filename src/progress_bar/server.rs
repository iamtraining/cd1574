use std::sync::Arc;

use hyper::{Body, Request, Response};
use routerify::prelude::*;
use routerify::{Middleware, RequestInfo, Router};
use tracing::info;

use super::bar::Bar;

async fn logger(
    res: Response<Body>,
    info: RequestInfo,
) -> Result<Response<Body>, routerify_json_response::Error> {
    info!("{} {} {}", info.method(), info.uri().path(), res.status(),);
    Ok(res)
}

pub fn router(
    bar: Arc<Bar>,
) -> anyhow::Result<Router<hyper::Body, routerify_json_response::Error>> {
    Ok(Router::builder()
        .middleware(Middleware::post_with_info(logger))
        .get("/status", status_bar)
        .data(bar)
        .build()
        .unwrap())
}

async fn status_bar(req: Request<Body>) -> Result<Response<Body>, routerify_json_response::Error> {
    let bar = req.data::<Arc<Bar>>().unwrap();
    let output = bar.get_status();

    let response = Response::builder()
        .status(200)
        .header("Content-Type", "text/plain")
        .body(Body::from(output.to_string()))
        .unwrap();

    Ok(response)
}
