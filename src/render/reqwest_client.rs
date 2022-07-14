use {
    anyhow::anyhow,
    reqwest::{Request, Response},
    tower::{BoxError, Service},
    tracing::{error, info, trace, Level},
};

#[async_trait::async_trait]
pub trait RenderClient {
    async fn avif_delete(&self, nm_id: i64, pics_count: i16) -> anyhow::Result<bool>;
}

pub struct Render<Cli> {
    addr: String,
    token: String,
    client: Cli,
}

#[async_trait::async_trait]
impl<Cli> RenderClient for Render<Cli>
where
    Cli: Service<Request, Response = Response, Error = BoxError> + Clone + Send + Sync + 'static,
    Cli::Future: Send,
{
    async fn avif_delete(&self, nm_id: i64, pics_count: i16) -> anyhow::Result<bool> {
        let multipart = reqwest::multipart::Form::new()
            .text("action", "avif_delete")
            .text("nmid", nm_id.to_string())
            .text("picsCount", pics_count.to_string());

        let req = reqwest::Client::new()
            .request(reqwest::Method::POST, &self.addr)
            .header("Authorization", &self.token)
            .multipart(multipart)
            .build()?;

        self.client
            .clone()
            .call(req)
            .await
            .map_err(|err| anyhow!("{err}"))?;

        Ok(true)
    }
}

impl<Cli> Render<Cli> {
    pub fn new(addr: String, token: String, client: Cli) -> Self {
        Self {
            addr,
            token,
            client,
        }
    }
}

pub async fn avif_delete<C>(
    mut client: C,
    nm_id: i64,
    pics_count: i16,
    addr: &str,
    token: &str,
) -> anyhow::Result<(i64, bool)>
where
    C: Service<Request, Response = Response, Error = BoxError> + Clone + Send + 'static,
    C::Future: Send,
{
    let multipart = reqwest::multipart::Form::new()
        .text("action", "avif_delete")
        .text("nmid", nm_id.to_string())
        .text("picsCount", pics_count.to_string());

    let req = reqwest::Client::new()
        .request(reqwest::Method::POST, addr)
        .header("Authorization", token)
        .multipart(multipart)
        .build()?;

    client.call(req).await.map_err(|err| anyhow!("{err}"))?;

    Ok((nm_id, true))
}

// func randomBoundary() string {
// 	var buf [30]byte
// 	_, err := io.ReadFull(rand.Reader, buf[:])
// 	if err != nil {
// 		panic(err)
// 	}
// 	return fmt.Sprintf("%x", buf[:])
// }
