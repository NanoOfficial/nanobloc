use std::{future::Future, marker::PhantomData};
use time::OffsetDateTime;

use crate::{error, EndpointMutability};

pub type Result<I> = std::result::Result<I, error::Error>;

#[derive(Debug)]
pub struct With<Q, I, R, F> {
    pub handler: F,
    pub actuality: Actuality,
    _query_type: PhantomData<Q>,
    _item_type: PhantomData<I>,
    _result_type: PhantomData<R>,
}

#[derive(Debug, Clone)]
pub enum Actuality {
    Actual,
    Deprecated {
        discontinued_on: Option<OffsetDateTime>,
        description: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Deprecated<Q, I, R, F> {
    pub handler: F,
    pub discontinued_on: Option<OffsetDateTime>,
    pub description: Option<String>,
    _query_type: PhantomData<Q>,
    _item_type: PhantomData<I>,
    _result_type: PhantomData<R>,
}

impl<Q, I, R, F> Deprecated<Q, I, R, F> {
    pub fn new(handler: F) -> Self {
        Self {
            handler,
            discontinued_on: None,
            description: None,
            _query_type: PhantomData,
            _item_type: PhantomData,
            _result_type: PhantomData,
        }
    }

    pub fn with_date(self, discontinued_on: OffsetDateTime) -> Self {
        Self {
            discontinued_on: Some(discontinued_on),
            ..self
        }
    }

    pub fn with_description<S: Into<String>>(self, description: S) -> Self {
        Self {
            description: Some(description.into()),
            ..self
        }
    }

    pub fn with_different_handler<F1, R1>(self, handler: F1) -> Deprecated<Q, I, R1, F1>
    where
        F1: Fn(Q) -> R1,
        R1: Future<Output = Result<I>>,
    {
        Deprecated {
            handler,
            discontinued_on: self.discontinued_on,
            description: self.description,

            _query_type: PhantomData,
            _item_type: PhantomData,
            _result_type: PhantomData,
        }
    }
}

impl<Q, I, R, F> From<F> for Deprecated<Q, I, R, F>
where
    F: Fn(Q) -> R,
    R: Future<Output = Result<I>>,
{
    fn from(handler: F) -> Self {
        Self::new(handler)
    }
}

impl<Q, I, R, F> From<Deprecated<Q, I, R, F>> for With<Q, I, R, F> {
    fn from(deprecated: Deprecated<Q, I, R, F>) -> Self {
        Self {
            handler: deprecated.handler,
            actuality: Actuality::Deprecated {
                discontinued_on: deprecated.discontinued_on,
                description: deprecated.description,
            },
            _query_type: PhantomData,
            _item_type: PhantomData,
            _result_type: PhantomData,
        }
    }
}

#[derive(Debug)]
pub struct NamedWith<Q, I, R, F> {
    pub name: String,
    pub inner: With<Q, I, R, F>,
    pub mutability: EndpointMutability,
}

impl<Q, I, R, F> NamedWith<Q, I, R, F> {
    pub fn new<S, W>(name: S, inner: W, mutability: EndpointMutability) -> Self
    where
        S: Into<String>,
        W: Into<With<Q, I, R, F>>,
    {
        Self {
            name: name.into(),
            inner: inner.into(),
            mutability,
        }
    }

    pub fn mutable<S, W>(name: S, inner: W) -> Self
    where
        S: Into<String>,
        W: Into<With<Q, I, R, F>>,
    {
        Self {
            name: name.into(),
            inner: inner.into(),
            mutability: EndpointMutability::Mutable,
        }
    }

    pub fn immutable<S, W>(name: S, inner: W) -> Self
    where
        S: Into<String>,
        W: Into<With<Q, I, R, F>>,
    {
        Self {
            name: name.into(),
            inner: inner.into(),
            mutability: EndpointMutability::Immutable,
        }
    }
}

impl<Q, I, R, F> From<F> for With<Q, I, R, F>
where
    F: Fn(Q) -> R,
    R: Future<Output = Result<I>>,
{
    fn from(handler: F) -> Self {
        Self {
            handler,
            actuality: Actuality::Actual,
            _query_type: PhantomData,
            _item_type: PhantomData,
            _result_type: PhantomData,
        }
    }
}
