pub use self::{
    cors::AllowOrigin,
    error::{Error, ErrorBody, HttpStatusCode, MovedPermanentlyError},
    manager::{ApiManager, ApiManagerConfig, UpdateEndpoints, WebServerConfig},
    withs::{Actuality, Deprecated, NamedWith, Result, With},
};

mod cors;
mod end;
mod error;
mod manager;
mod withs;
use serde::{de::DeserializeOwned, Serialize};
use std::{collections::BTreeMap, fmt, future::Future};

use crate::end::actix;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub enum EndpointMutability {
    Mutable,
    Immutable,
}

pub trait ApiBackend: Sized {
    type Handler;
    type Backend;

    fn endpoint<Q, I, R, F, E>(&mut self, name: &str, endpoint: E) -> &mut Self
    where
        Q: DeserializeOwned + 'static,
        I: Serialize + 'static,
        F: Fn(Q) -> R + 'static + Clone,
        E: Into<With<Q, I, R, F>>,
        Self::Handler: From<NamedWith<Q, I, R, F>>,
    {
        let named_with = NamedWith::immutable(name, endpoint);
        self.raw_handler(Self::Handler::from(named_with))
    }

    fn endpoint_mut<Q, I, R, F, E>(&mut self, name: &str, endpoint: E) -> &mut Self
    where
        Q: DeserializeOwned + 'static,
        I: Serialize + 'static,
        F: Fn(Q) -> R + 'static + Clone,
        E: Into<With<Q, I, R, F>>,
        Self::Handler: From<NamedWith<Q, I, R, F>>,
    {
        let named_with = NamedWith::mutable(name, endpoint);
        self.raw_handler(Self::Handler::from(named_with))
    }

    fn raw_handler(&mut self, handler: Self::Handler) -> &mut Self;

    fn wire(&self, output: Self::Backend) -> Self::Backend;
}

#[derive(Debug, Clone, Default)]
pub struct ApiScope {
    pub(crate) actix_backend: actix::ApiBuilder,
}

impl ApiScope {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn endpoint<Q, I, R, F, E>(&mut self, name: &str, endpoint: E) -> &mut Self
    where
        Q: DeserializeOwned + 'static,
        I: Serialize + 'static,
        F: Fn(Q) -> R + 'static + Clone + Send + Sync,
        E: Into<With<Q, I, R, F>>,
        R: Future<Output = crate::Result<I>>,
    {
        self.actix_backend.endpoint(name, endpoint);
        self
    }

    pub fn endpoint_mut<Q, I, R, F, E>(&mut self, name: &str, endpoint: E) -> &mut Self
    where
        Q: DeserializeOwned + 'static,
        I: Serialize + 'static,
        F: Fn(Q) -> R + 'static + Clone + Send + Sync,
        E: Into<With<Q, I, R, F>>,
        R: Future<Output = crate::Result<I>>,
    {
        self.actix_backend.endpoint_mut(name, endpoint);
        self
    }

    pub fn web_backend(&mut self) -> &mut actix::ApiBuilder {
        &mut self.actix_backend
    }
}

#[derive(Debug, Clone, Default)]
pub struct ApiBuilder {
    pub public_scope: ApiScope,
    pub private_scope: ApiScope,
}

impl ApiBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn public_scope(&mut self) -> &mut ApiScope {
        &mut self.public_scope
    }

    pub fn private_scope(&mut self) -> &mut ApiScope {
        &mut self.private_scope
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ApiAccess {
    Public,
    Private,
}

impl fmt::Display for ApiAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ApiAccess::Public => f.write_str("public"),
            ApiAccess::Private => f.write_str("private"),
        }
    }
}

pub trait ExtendApiBackend {
    fn extend<'a, I>(self, items: I) -> Self
    where
        I: IntoIterator<Item = (&'a str, &'a ApiScope)>;
}

#[derive(Debug, Clone, Default)]
pub struct ApiAggregator {
    endpoints: BTreeMap<String, ApiBuilder>,
}

impl ApiAggregator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: &str, api: ApiBuilder) {
        self.endpoints.insert(name.to_owned(), api);
    }

    pub fn extend(&mut self, endpoints: impl IntoIterator<Item = (String, ApiBuilder)>) {
        self.endpoints.extend(endpoints);
    }

    #[doc(hidden)]
    pub fn extend_backend<B: ExtendApiBackend>(&self, access: ApiAccess, backend: B) -> B {
        let endpoints = self.endpoints.iter();
        match access {
            ApiAccess::Public => backend
                .extend(endpoints.map(|(name, builder)| (name.as_str(), &builder.public_scope))),
            ApiAccess::Private => backend
                .extend(endpoints.map(|(name, builder)| (name.as_str(), &builder.private_scope))),
        }
    }
}
