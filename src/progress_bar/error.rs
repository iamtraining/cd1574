use anyhow::anyhow;
use hyper::{header, Body, Response, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::error;

fn json_response<T: Serialize>(status: StatusCode, data: T) -> Result<Response<Body>, ApiError> {
    let json = serde_json::to_string(&data).map_err(ApiError::from_err)?;
    let response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json))
        .map_err(ApiError::from_err)?;
    Ok(response)
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("NotFound: {0}")]
    NotFound(String),

    #[error(transparent)]
    InternalServerError(#[from] anyhow::Error),
}

impl ApiError {
    pub fn from_err<E: Into<anyhow::Error>>(err: E) -> Self {
        Self::InternalServerError(anyhow!(err))
    }

    pub fn into_response(self) -> Response<Body> {
        match self {
            ApiError::NotFound(_) => {
                HttpErrorBody::response_from_msg_and_status(self.to_string(), StatusCode::NOT_FOUND)
            }
            ApiError::InternalServerError(err) => HttpErrorBody::response_from_msg_and_status(
                err.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct HttpErrorBody {
    pub msg: String,
}

impl HttpErrorBody {
    pub fn from_msg(msg: String) -> Self {
        HttpErrorBody { msg }
    }

    pub fn response_from_msg_and_status(msg: String, status: StatusCode) -> Response<Body> {
        HttpErrorBody { msg }.to_response(status)
    }

    pub fn to_response(&self, status: StatusCode) -> Response<Body> {
        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_string(self).unwrap()))
            .unwrap()
    }
}

pub async fn handler(err: routerify::RouteError) -> Response<Body> {
    error!("error while processing http request: {:?}", err);
    err.downcast::<ApiError>()
        .expect("pub async fn handler")
        .into_response()
}
