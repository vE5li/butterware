#![feature(proc_macro_tracked_env)]

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::{fs, io};

use colored::Colorize;
use convert_case::{Case, Casing};
use derive_syn_parse::Parse;
use proc_macro::TokenStream;
use quote::quote;
use syn::Token;

#[derive(Parse)]
struct Input {
    _as: Token![as],
    ident: syn::Ident,
}

#[proc_macro]
pub fn alias_used_keyboard(input: TokenStream) -> TokenStream {
    let Input { ident, .. } = syn::parse_macro_input!(input as Input);

    colored::control::set_override(true);

    let name = proc_macro::tracked_env::var("KEYBOARD").unwrap_or("meboard".to_owned());

    println!("[{}] Building for {}", "Build system".red().bold(), name.yellow());

    let module = syn::Ident::new(&name.to_case(Case::Snake), ident.span());
    let keyboard = syn::Ident::new(&name.to_case(Case::Pascal), ident.span());

    quote! { use crate::keyboards::#module::#keyboard as #ident; }.into()
}

#[derive(Debug)]
enum Error {
    Io(io::Error),
    Utf8(OsString),
    Empty,
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

fn source_file_names<P: AsRef<Path>>(dir: P) -> Result<Vec<String>, Error> {
    let mut names = Vec::new();
    let mut failures = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }

        let file_name = entry.file_name();
        if file_name == "mod.rs" || file_name == "lib.rs" || file_name == "main.rs" {
            continue;
        }

        let path = Path::new(&file_name);
        if path.extension() == Some(OsStr::new("rs")) {
            match file_name.into_string() {
                Ok(mut utf8) => {
                    utf8.truncate(utf8.len() - ".rs".len());
                    names.push(utf8);
                }
                Err(non_utf8) => {
                    failures.push(non_utf8);
                }
            }
        }
    }

    failures.sort();
    if let Some(failure) = failures.into_iter().next() {
        return Err(Error::Utf8(failure));
    }

    if names.is_empty() {
        return Err(Error::Empty);
    }

    names.sort();
    Ok(names)
}

#[proc_macro]
pub fn import_keyboards(input: TokenStream) -> TokenStream {
    let directory = syn::parse_macro_input!(input as syn::LitStr);
    let cargo_conform_directory = format!("src/{}", directory.value());
    let entries = source_file_names(&cargo_conform_directory).expect("Failed to get keyboards.");

    let (path, module) = entries
        .iter()
        .map(|name| {
            (
                syn::LitStr::new(&format!("{}.rs", name), directory.span()),
                syn::Ident::new(name, directory.span()),
            )
        })
        .unzip::<_, _, Vec<_>, Vec<_>>();

    quote! {
        #[path = #directory]
        mod keyboards {
            #(
                #[path = #path]
                pub mod #module;
            )*
        }
    }
    .into()
}
