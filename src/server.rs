use http_body_util::{BodyExt, Full};
use hyper::{
    body::{Bytes, Incoming},
    header::HeaderValue,
    Request,
};

use crate::{BoxBody, SERVER_API_KEY};

pub(crate) fn full<T: Into<Bytes>>(chunk: T) -> BoxBody {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

pub(crate) fn api_key_auth(req: &Request<Incoming>) -> bool {
    if let Some(key) = req.headers().get("Authorization") {
        return sha256_string(key)
            == *SERVER_API_KEY
                .get()
                .expect("Error getting server API key from OnceLock");
    }

    false
}

#[inline]
fn sha256_string(data: &HeaderValue) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    let result = hasher.finalize();
    let string_result = format!("{:x}", result);

    string_result
}
