use proc_macro2::{Ident, Span, TokenStream};
use protobuf_codegen::Customize;
use quote::{quote, ToTokens};
use std::{
    collections::HashSet,
    env,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Debug, Copy, Clone)]
pub enum ProtoSources<'a> {
    Nano,
    Crypto,
    Common,
    Merkledb,
    Path(&'a str),
}

impl<'a> ProtoSources<'a> {
    // pub fn path(&self) -> String {
    //     // match self {
    //     //     // match functionalities
    //     // }
    // }
}

impl<'a> From<&'a str> for ProtoSources<'a> {
    fn from(path: &'a str) -> Self {
        ProtoSources::Path(path)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ProtobufFile {
    full_path: PathBuf,
    relative_path: String,
}