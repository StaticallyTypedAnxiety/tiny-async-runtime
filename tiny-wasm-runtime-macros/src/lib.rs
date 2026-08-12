use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse::Parser,
    punctuated::Punctuated,
    spanned::Spanned,
    Block, Error, ItemFn, Lit, Meta, MetaNameValue, Path, ReturnType, Token,
};

#[proc_macro_attribute]
pub fn main(args: TokenStream, input: TokenStream) -> TokenStream {
    expand_main(args, input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// Exports the async `wasi:cli/run` entrypoint from a raw statement body,
/// with no implicit `block_on` wrapper -- unlike `#[tiny_wasm_runtime::main]`
/// on `fn main`, which always wraps the whole body in one. Use this instead
/// when the body needs to call `WasmRuntimeAsyncEngine::block_on` itself one
/// or more times (an outer wrapper would nest a second, reentrant call on
/// the same thread and deadlock).
#[proc_macro]
pub fn async_command(input: TokenStream) -> TokenStream {
    expand_async_command(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn expand_async_command(input: TokenStream) -> syn::Result<proc_macro2::TokenStream> {
    let crate_path: Path = syn::parse_quote!(::tiny_wasm_runtime);
    let stmts = Block::parse_within.parse(input)?;

    Ok(quote! {
        struct __TinyWasmRuntimeAsyncCommand;

        impl #crate_path::bindings::exports::wasi::cli::run::Guest for __TinyWasmRuntimeAsyncCommand {
            async fn run() -> ::core::result::Result<(), ()> {
                #(#stmts)*
                ::core::result::Result::Ok(())
            }
        }

        #crate_path::bindings::export_command!(__TinyWasmRuntimeAsyncCommand);
    })
}

fn expand_main(args: TokenStream, input: TokenStream) -> syn::Result<proc_macro2::TokenStream> {
    let crate_path = parse_crate_path(args)?;
    let function = syn::parse::<ItemFn>(input)?;
    build_main(crate_path, function)
}

fn parse_crate_path(args: TokenStream) -> syn::Result<Path> {
    let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
    let metas = parser.parse2(args.into())?;
    let mut crate_path = syn::parse_quote!(::tiny_wasm_runtime);

    for meta in metas {
        match meta {
            Meta::NameValue(MetaNameValue { path, value, .. }) if path.is_ident("crate") => {
                let expr = match value {
                    syn::Expr::Lit(expr_lit) => expr_lit,
                    other => {
                        return Err(Error::new(
                            other.span(),
                            "expected `crate = \"path::to::runtime\"`",
                        ))
                    }
                };

                let Lit::Str(path_literal) = expr.lit else {
                    return Err(Error::new(
                        expr.lit.span(),
                        "expected `crate` value to be a string literal path",
                    ));
                };

                crate_path = path_literal.parse()?;
            }
            other => {
                return Err(Error::new(
                    other.span(),
                    "unsupported attribute arguments; expected optional `crate = \"...\"`",
                ))
            }
        }
    }

    Ok(crate_path)
}

fn build_main(crate_path: Path, mut function: ItemFn) -> syn::Result<proc_macro2::TokenStream> {
    if function.sig.asyncness.is_none() {
        return Err(Error::new(
            function.sig.span(),
            "`#[tiny_wasm_runtime::main]` requires an async function",
        ));
    }

    if function.sig.ident == "main" {
        return build_command_main(crate_path, function);
    }

    function.sig.asyncness = None;

    let attrs = function.attrs;
    let vis = function.vis;
    let sig = function.sig;
    let block = function.block;

    Ok(quote! {
        #(#attrs)*
        #vis #sig {
            #crate_path::WasmRuntimeAsyncEngine::block_on(async move #block)
        }
    })
}

/// `fn main` is treated as the component's entrypoint: WASI 0.3's
/// `wasi:cli/command` world declares `run` as `async func`, and there's no
/// `wasm32-wasip3` rustc target yet to emit that from a plain `fn main()`,
/// so this generates the `Guest` impl + `export_command!` call by hand
/// instead of wrapping `main` in `block_on` like any other function. The
/// crate root still needs `#![no_main]` itself -- an attribute macro on one
/// item can't inject a crate-level inner attribute elsewhere in the file.
fn build_command_main(crate_path: Path, function: ItemFn) -> syn::Result<proc_macro2::TokenStream> {
    if !function.sig.inputs.is_empty() {
        return Err(Error::new(
            function.sig.inputs.span(),
            "`#[tiny_wasm_runtime::main]` on `fn main` does not support arguments",
        ));
    }

    let returns_unit = match &function.sig.output {
        ReturnType::Default => true,
        ReturnType::Type(_, ty) => matches!(&**ty, syn::Type::Tuple(tuple) if tuple.elems.is_empty()),
    };
    if !returns_unit {
        return Err(Error::new(
            function.sig.output.span(),
            "`#[tiny_wasm_runtime::main]` on `fn main` requires no return value",
        ));
    }

    let attrs = function.attrs;
    let block = function.block;

    Ok(quote! {
        #(#attrs)*
        struct __TinyWasmRuntimeMain;

        impl #crate_path::bindings::exports::wasi::cli::run::Guest for __TinyWasmRuntimeMain {
            async fn run() -> ::core::result::Result<(), ()> {
                #crate_path::WasmRuntimeAsyncEngine::block_on(async move #block);
                ::core::result::Result::Ok(())
            }
        }

        #crate_path::bindings::export_command!(__TinyWasmRuntimeMain);
    })
}
