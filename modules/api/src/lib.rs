// @filename: errors.rs
// @author: Krisna Pranav
// @license: Apache-2.0 License

mod errors;
mod withs;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum EndpointMutability {
    Mutable,
    Immutable,
}
