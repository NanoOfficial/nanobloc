pub use actix_cors::Cors;
pub use actix_web::{
    body::EitherBody,
    dev::JsonBody,
    http::{Method as HttpMethod, StatusCode as HttpStatusCode},
    web::{Bytes, Payload},
    HttpRequest, HttpResponse,
};

use actix_web::{
    body::{BodySize, BoxBody, MessageBody},
    dev::ServiceResponse,
    error::ResponseError,
    http::header,
    middleware::{ErrorHandlerResponse, ErrorHandlers},
    web::{self, scope, Json, Query},
    FromRequest,
};
use futures::{
    future::{Future, LocalBoxFuture},
    prelude::*,
};
use serde::{de::DeserializeOwned, Serialize};

use std::{fmt, sync::Arc};

use crate::{
    Actuality, AllowOrigin, ApiBackend, ApiScope, EndpointMutability, Error as ApiError,
    ExtendApiBackend, NamedWith,
};

pub type RawHandler = dyn Fn(HttpRequest, Payload) -> LocalBoxFuture<'static, Result<HttpResponse, actix_web::Error>>
    + 'static
    + Send
    + Sync;

#[derive(Clone)]
pub struct RequestHandler {
    pub name: String,
    pub method: actix_web::http::Method,
    pub inner: Arc<RawHandler>,
}

impl fmt::Debug for RequestHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestHandler")
            .field("name", &self.name)
            .field("method", &self.method)
            .finish()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ApiBuilder {
    handlers: Vec<RequestHandler>,
}

impl ApiBuilder {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ApiBackend for ApiBuilder {
    type Handler = RequestHandler;
    type Backend = actix_web::Scope;

    fn raw_handler(&mut self, handler: Self::Handler) -> &mut Self {
        self.handlers.push(handler);
        self
    }

    fn wire(&self, mut output: Self::Backend) -> Self::Backend {
        for handler in &self.handlers {
            let inner = handler.inner.clone();
            output = output.route(
                &handler.name,
                web::method(handler.method.clone())
                    .to(move |request, payload| inner(request, payload)),
            );
        }
        output
    }
}

impl ExtendApiBackend for actix_web::Scope {
    fn extend<'a, I>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = (&'a str, &'a ApiScope)>,
    {
        for item in items {
            self = self.service(item.1.actix_backend.wire(scope(item.0)))
        }
        self
    }
}

impl ResponseError for ApiError {
    fn error_response(&self) -> HttpResponse {
        let body = serde_json::to_value(&self.body).unwrap();
        let body = if body == serde_json::json!({}) {
            Bytes::new()
        } else {
            serde_json::to_string(&self.body).unwrap().into()
        };

        let mut response = HttpResponse::build(self.http_code)
            .append_header((header::CONTENT_TYPE, "application/problem+json"))
            .body(body);

        for (key, value) in self.headers.iter() {
            response.headers_mut().append(key.clone(), value.clone());
        }

        response
    }
}

fn json_response<T: Serialize>(actuality: Actuality, json_value: T) -> HttpResponse {
    let mut response = HttpResponse::Ok();

    if let Actuality::Deprecated {
        ref discontinued_on,
        ref description,
    } = actuality
    {
        let expiration_note = match discontinued_on {
            Some(date) => {
                let date_format = time::format_description::parse(
                    "[weekday repr:short], [day] [month repr:short] [year] [hour]:[minute]:[second] GMT",
                )
                .unwrap();
                format!(
                    "The old API is maintained until {}.",
                    date.format(&date_format).unwrap_or_default()
                )
            }
            None => "Currently there is no specific date for disabling this endpoint.".into(),
        };

        let mut warning_text = format!(
            "Deprecated API: This endpoint is deprecated, \
             see the service documentation to find an alternative. \
             {}",
            expiration_note
        );

        if let Some(description) = description {
            warning_text = format!("{} Additional information: {}.", warning_text, description);
        }

        let warning_string = create_warning_header(&warning_text);

        response.append_header((header::WARNING, warning_string));
    }

    response.json(json_value)
}

fn create_warning_header(warning_text: &str) -> String {
    format!("299 - \"{}\"", warning_text)
}

impl From<EndpointMutability> for actix_web::http::Method {
    fn from(mutability: EndpointMutability) -> Self {
        match mutability {
            EndpointMutability::Immutable => actix_web::http::Method::GET,
            EndpointMutability::Mutable => actix_web::http::Method::POST,
        }
    }
}

async fn extract_query<Q>(
    request: HttpRequest,
    payload: Payload,
    mutability: EndpointMutability,
) -> Result<Q, ApiError>
where
    Q: DeserializeOwned + 'static,
{
    match mutability {
        EndpointMutability::Immutable => Query::extract(&request)
            .await
            .map(Query::into_inner)
            .map_err(|e| {
                ApiError::bad_request()
                    .title("Query parse error")
                    .detail(e.to_string())
            }),

        EndpointMutability::Mutable => Json::from_request(&request, &mut payload.into_inner())
            .await
            .map(Json::into_inner)
            .map_err(|e| {
                ApiError::bad_request()
                    .title("JSON body parse error")
                    .detail(e.to_string())
            }),
    }
}

impl<Q, I, F, R> From<NamedWith<Q, I, R, F>> for RequestHandler
where
    F: Fn(Q) -> R + 'static + Clone + Send + Sync,
    Q: DeserializeOwned + 'static,
    I: Serialize + 'static,
    R: Future<Output = Result<I, crate::Error>>,
{
    fn from(f: NamedWith<Q, I, R, F>) -> Self {
        let handler = f.inner.handler;
        let actuality = f.inner.actuality;
        let mutability = f.mutability;
        let index = move |request: HttpRequest, payload: Payload| {
            let handler = handler.clone();
            let actuality = actuality.clone();

            async move {
                let query = extract_query(request, payload, mutability).await?;
                let response = handler(query).await?;
                Ok(json_response(actuality, response))
            }
            .boxed_local()
        };

        Self {
            name: f.name,
            method: f.mutability.into(),
            inner: Arc::from(index) as Arc<RawHandler>,
        }
    }
}

impl From<&AllowOrigin> for Cors {
    fn from(origin: &AllowOrigin) -> Self {
        match *origin {
            AllowOrigin::Any => Cors::default(),
            AllowOrigin::Whitelist(ref hosts) => {
                let mut cors = Cors::default();
                for host in hosts {
                    cors = cors.allowed_origin(host);
                }

                cors
            }
        }
    }
}

impl From<AllowOrigin> for Cors {
    fn from(origin: AllowOrigin) -> Self {
        Self::from(&origin)
    }
}

trait ErrorHandlersEx {
    fn default_api_error<F: Fn(&ServiceResponse<EitherBody<BoxBody>>) -> ApiError + 'static>(
        self,
        status: HttpStatusCode,
        handler: F,
    ) -> Self;
}

impl ErrorHandlersEx for ErrorHandlers<EitherBody<BoxBody>> {
    fn default_api_error<F: Fn(&ServiceResponse<EitherBody<BoxBody>>) -> ApiError + 'static>(
        self,
        status: HttpStatusCode,
        handler: F,
    ) -> Self {
        self.handler(status, move |res| {
            let res = match res.response().body().size() {
                BodySize::None | BodySize::Sized(0) | BodySize::Stream => {
                    let error: actix_web::Error = handler(&res).into();
                    res.into_response(error.as_response_error().error_response())
                        .map_into_left_body()
                }
                _ => res,
            };

            Ok(ErrorHandlerResponse::Response(res.map_into_left_body()))
        })
    }
}

pub(crate) fn error_handlers() -> ErrorHandlers<EitherBody<BoxBody>> {
    ErrorHandlers::new()
        .default_api_error(HttpStatusCode::NOT_FOUND, |res| {
            ApiError::not_found()
                .title("Method not found")
                .detail(format!(
                    "API endpoint `{}` doesn't exist",
                    res.request().uri().path()
                ))
        })
        .default_api_error(HttpStatusCode::BAD_REQUEST, |_res| {
            ApiError::bad_request().title("Bad request")
        })
}
