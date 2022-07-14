use futures::future;
use hyper::{Body, Error, Request, Response};
use tower::retry::Policy;
use tracing::{trace, warn};

#[derive(Clone, Debug)]
pub struct RetryLimit {
    remaining_tries: usize,
}

type Requestttt = Request<http_body::Full<hyper::body::Bytes>>;

impl Policy<Requestttt, Response<Body>, Error> for RetryLimit {
    type Future = future::Ready<Self>;

    fn retry(
        &self,
        _req: &Requestttt,
        result: Result<&Response<Body>, &Error>,
    ) -> Option<Self::Future> {
        warn!(?result);
        match result {
            Ok(resp) => {
                if resp.status() == 200 {
                    trace!("resp.status() == 200");
                    None
                } else {
                    trace!("RETRY");
                    self.should_retry()
                }
            }
            Err(_) => {
                trace!("RETRY FROM ERR");
                self.should_retry()
            }
        }
    }

    //  Request<http_body::Full<hyper::body::Bytes>>

    fn clone_request(&self, req: &Requestttt) -> Option<Requestttt> {
        let mut builder = Request::builder()
            .method(req.method())
            .uri(req.uri())
            .version(req.version());

        *builder.headers_mut()? = req.headers().clone();
        builder.body(req.body().clone()).ok()
    }
}

impl RetryLimit {
    pub fn new(remaining_tries: usize) -> Self {
        Self { remaining_tries }
    }

    fn should_retry(&self) -> Option<future::Ready<Self>> {
        trace!("should_retry");
        if self.remaining_tries > 0 {
            let remaining_tries = self.remaining_tries - 1;
            Some(future::ready(RetryLimit { remaining_tries }))
        } else {
            None
        }
    }
}
