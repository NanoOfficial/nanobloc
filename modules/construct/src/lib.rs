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
    pub fn path(&self) -> String {
        match self {
            ProtoSources::Nano => get_nano_protobuf_files_path(),
            ProtoSources::Common => get_nano_protobuf_common_files_path(),
            ProtoSources::Crypto => get_nano_protobuf_crypto_files_path(),
            ProtoSources::Merkledb => get_nano_protobuf_merkledb_files_path(),
            ProtoSources::Path(path) => (*path).to_string(),
        }
    }
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

fn get_proto_files<P: AsRef<Path>>(path: &P) -> Vec<ProtobufFile> {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| {
            let entry = e.ok()?;
            if entry.file_type().is_file() && entry.path().extension()?.to_str() == Some("proto") {
                let full_path = entry.path().to_owned();
                let relative_path = full_path.strip_prefix(path).unwrap().to_owned();
                let relative_path = relative_path
                    .to_str()
                    .expect("Cannot convert relative path to string");

                Some(ProtobufFile {
                    full_path,
                    relative_path: canonicalize_protobuf_path(relative_path),
                })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(windows)]
fn canonicalize_protobuf_path(path_str: &str) -> String {
    path_str.replace('\\', "/")
}

#[cfg(not(windows))]
fn canonicalize_protobuf_path(path_str: &str) -> String {
    path_str.to_owned()
}

fn include_proto_files(proto_files: HashSet<&ProtobufFile>, name: &str) -> impl ToTokens {
    let proto_files_len = proto_files.len();

    let proto_files = proto_files.iter().map(|file| {
        let name = &file.relative_path;

        let mut content = String::new();
        File::open(&file.full_path)
            .expect("Unable to open .proto file")
            .read_to_string(&mut content)
            .expect("Unable to read .proto file");

        quote! {
            (#name, #content),
        }
    });

    let name = Ident::new(name, Span::call_site());

    quote! {
        #[allow(dead_code)]
        pub const #name: [(&str, &str); #proto_files_len] = [
            #( #proto_files )*
        ];
    }
}

fn get_mod_files(proto_files: &[ProtobufFile]) -> impl Iterator<Item = TokenStream> + '_ {
    proto_files.iter().map(|file| {
        let mod_name = file
            .full_path
            .file_stem()
            .unwrap()
            .to_str()
            .expect(".proto file name is not convertible to &str");

        let mod_name = Ident::new(mod_name, Span::call_site());
        if mod_name == "tests" {
            quote! {
                #[cfg(test)] pub mod #mod_name;
            }
        } else {
            quote! {
                pub mod #mod_name;
            }
        }
    })
}

fn generate_mod_rs(
    out_dir: impl AsRef<Path>,
    proto_files: &[ProtobufFile],
    includes: &[ProtobufFile],
    mod_file: impl AsRef<Path>,
) {
    let mod_files = get_mod_files(proto_files);

    let includes = includes
        .iter()
        .filter(|file| !proto_files.contains(file))
        .collect();

    let proto_files = include_proto_files(proto_files.iter().collect(), "PROTO_SOURCES");
    let includes = include_proto_files(includes, "INCLUDES");

    let content = quote! {
        #( #mod_files )*
        #proto_files
        #includes
    };

    let dest_path = out_dir.as_ref().join(mod_file);
    let mut file = File::create(dest_path).expect("Unable to create output file");
    file.write_all(content.into_token_stream().to_string().as_bytes())
        .expect("Unable to write data to file");
}

fn generate_mod_rs_without_sources(
    out_dir: impl AsRef<Path>,
    proto_files: &[ProtobufFile],
    mod_file: impl AsRef<Path>,
) {
    let mod_files = get_mod_files(proto_files);
    let content = quote! {
        #( #mod_files )*
    };
    let dest_path = out_dir.as_ref().join(mod_file);
    let mut file = File::create(dest_path).expect("Unable to create output file");
    file.write_all(content.into_token_stream().to_string().as_bytes())
        .expect("Unable to write data to file");
}

#[derive(Debug)]
pub struct ProtobufGenerator<'a> {
    includes: Vec<ProtoSources<'a>>,
    mod_name: &'a str,
    input_dir: &'a str,
    include_sources: bool,
}

impl<'a> ProtobufGenerator<'a> {
    pub fn with_mod_name(mod_name: &'a str) -> Self {
        assert!(!mod_name.is_empty(), "Mod name is not specified");
        Self {
            includes: Vec::new(),
            input_dir: "",
            mod_name,
            include_sources: true,
        }
    }
    pub fn with_input_dir(mut self, path: &'a str) -> Self {
        assert!(
            self.input_dir.is_empty(),
            "Input directory is already specified"
        );
        self.input_dir = path;
        self.includes.push(ProtoSources::Path(path));
        self
    }

    pub fn add_path(mut self, path: &'a str) -> Self {
        self.includes.push(ProtoSources::Path(path));
        self
    }

    pub fn with_common(mut self) -> Self {
        self.includes.push(ProtoSources::Common);
        self
    }

    pub fn with_crypto(mut self) -> Self {
        self.includes.push(ProtoSources::Crypto);
        self
    }

    pub fn with_merkledb(mut self) -> Self {
        self.includes.push(ProtoSources::Merkledb);
        self
    }

    pub fn with_nano(mut self) -> Self {
        self.includes.push(ProtoSources::Nano);
        self
    }

    pub fn with_includes(mut self, includes: &'a [ProtoSources<'_>]) -> Self {
        self.includes.extend_from_slice(includes);
        self
    }

    pub fn without_sources(mut self) -> Self {
        self.include_sources = false;
        self
    }

    pub fn generate(self) {
        assert!(!self.input_dir.is_empty(), "Input dir is not specified");
        assert!(!self.includes.is_empty(), "Includes are not specified");
        protobuf_generate(
            self.input_dir,
            &self.includes,
            self.mod_name,
            self.include_sources,
        );
    }
}

fn protobuf_generate(
    input_dir: &str,
    includes: &[ProtoSources<'_>],
    mod_file_name: &str,
    include_sources: bool,
) {
    let out_dir = env::var("OUT_DIR")
        .map(PathBuf::from)
        .expect("Unable to get OUT_DIR");

    let includes: Vec<_> = includes.iter().map(ProtoSources::path).collect();
    let mut includes: Vec<&str> = includes.iter().map(String::as_str).collect();
    includes.push(input_dir);

    let proto_files = get_proto_files(&input_dir);

    if include_sources {
        let included_files = get_included_files(&includes);
        generate_mod_rs(&out_dir, &proto_files, &included_files, mod_file_name);
    } else {
        generate_mod_rs_without_sources(&out_dir, &proto_files, mod_file_name);
    }

    protobuf_codegen::Codegen::new()
        .pure()
        .out_dir(out_dir)
        .inputs(proto_files.into_iter().map(|f| f.full_path))
        .includes(&includes)
        .customize(
            Customize::default()
                .generate_accessors(true)
                .gen_mod_rs(true),
        )
        .run_from_script()
}

fn get_included_files(includes: &[&str]) -> Vec<ProtobufFile> {
    includes.iter().flat_map(get_proto_files).collect()
}

fn get_nano_protobuf_files_path() -> String {
    env::var("DEP_NANO_PROTOBUF_PROTOS").expect("Failed to get nano protobuf path")
}

fn get_nano_protobuf_crypto_files_path() -> String {
    env::var("DEP_NANO_PROTOBUF_CRYPTO_PROTOS")
        .expect("Failed to get nano crypto protobuf path")
}

fn get_nano_protobuf_common_files_path() -> String {
    env::var("DEP_NANO_PROTOBUF_COMMON_PROTOS")
        .expect("Failed to get nano common protobuf path")
}

fn get_nano_protobuf_merkledb_files_path() -> String {
    env::var("DEP_NANO_PROTOBUF_MERKLEDB_PROTOS")
        .expect("Failed to get nano merkledb protobuf path")
}