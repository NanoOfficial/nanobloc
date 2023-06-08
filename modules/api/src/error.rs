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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<u8>,
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

    pub fn internal(cause: impl fmt::Display) -> Self {
        Error::new(HttpStatusCode::INTERNAL_SERVER_ERROR).detail(cause.to_string())
    }

    pub fn docs_uri(mut self, docs_uri: impl Into<String>) -> Self {
        self.body.docs_uri = docs_uri.into();
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.body.title = title.into();
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.body.detail = detail.into();
        self
    }

    #[doc(hidden)]
    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.body.source = source.into();
        self
    }

    pub fn error_code(mut self, error_code: u8) -> Self {
        self.body.error_code = Some(error_code);
        self
    }

    pub(crate) fn header(mut self, key: HeaderName, value: &str) -> Self {
        self.headers.insert(key, value.parse().unwrap());
        self
    }

    pub fn parse(
        http_code: HttpStatusCode,
        body: &str,
    ) -> std::result::Result<Self, serde_json::Error> {
        let body = if !body.is_empty() {
            serde_json::from_str(body)?
        } else {
            ErrorBody::default()
        };

        Ok(Self {
            http_code,
            body,
            headers: HeaderMap::new(),
        })
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
    pub fn with_query<Q: Serialize>(self, query: Q) -> Self {
        let serialized_query =
            serde_urlencoded::to_string(query).expect("Unable to serialize query.");
        Self {
            query_part: Some(serialized_query),
            ..self
        }
    }
}

impl From<MovedPermanentlyError> for Error {
    fn from(e: MovedPermanentlyError) -> Self {
        let full_location = match e.query_part {
            Some(query) => format!("{}?{}", e.location, query),
            None => e.location,
        };

        Error::new(HttpStatusCode::MOVED_PERMANENTLY).header(header::LOCATION, &full_location)
    }
}
