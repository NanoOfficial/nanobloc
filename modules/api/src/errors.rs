//@filename: errors.rs
//@license: Apache-2.0 License

pub use actix_web::http::{
    header::{self, HeaderMap, HeaderName},
    StatusCode as HttpStatusCode,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
pub struct Error {
    pub http_code: HttpStatusCode,
    pub body: ErrorBody,
    pub headers: HeaderMap,
}

impl Default for Error {
    fn default() -> Self {
        Self {
            http_code: HttpStatusCode::default(),
            body: ErrorBody::default(),
            headers: HeaderMap::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub struct ErrorBody {
    #[serde(rename = "type", default, skip_serializing_if = "String::is_empty")]
    pub docs_uri: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.body.title, self.body.detail)
    }
}

impl Error {
    pub fn new(http_code: HttpStatusCode) -> Self {
        Self {
            http_code,
            body: ErrorBody::default(),
            headers: HeaderMap::new(),
        }
    }

    pub fn bad_request() -> Self {
        Error::new(HttpStatusCode::BAD_REQUEST)
    }

    pub fn forbidden() -> Self {
        Error::new(HttpStatusCode::FORBIDDEN)
    }

    pub fn not_found() -> Self {
        Error::new(HttpStatusCode::NOT_FOUND)
    }
}

#[derive(Debug)]
pub struct MovedPermanentlyError {
    location: String,
    query_part: Option<String>,
}

impl MovedPermanentlyError {
    pub fn new(location: String) -> Self {
        Self {
            location,
            query_part: None,
        }
    }
}
