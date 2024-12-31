use std::pin::Pin;

use bytes::Bytes;
use futures::Future;
use http_body_util::combinators::BoxBody;

use crate::error::Error;

pub type Response = hyper::Response<BoxBody<Bytes, Error>>;
pub type ServiceFuture = Pin<Box<dyn Future<Output = std::result::Result<Response, Error>> + Send>>;
