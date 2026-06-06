use proc_macro2::TokenStream;
use quote::quote;
use syn::{Expr, ExprLit, ItemStruct, Lit, LitStr, parse2};

use crate::common::MetaArgs;

/// `#[plugin]` generates the FFI exports every dynamically loaded plugin needs
/// (`create_plugin` / `destroy_plugin` / `plugin_abi`).
///
/// With metadata args it ALSO generates the whole [`SnbPlugin`] impl, folding in
/// `set_bot` + `register_all`:
///
/// ```ignore
/// #[plugin(name = "stdin", version = "0.1.0", kind = Adapter)]
/// pub struct StdinAdapter;
/// ```
///
/// Bare `#[plugin]` (no args) emits only the FFI — use it when the plugin needs
/// a hand-written `SnbPlugin` impl (custom `on_load` / `on_event`, fields, etc.).
/// The full form requires a unit struct (its generated `new()` returns `Self`).
pub fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    match try_expand(attr, item) {
        Ok(ts) => ts,
        Err(e) => e.to_compile_error(),
    }
}

fn try_expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let input = parse2::<ItemStruct>(item)?;
    let name = input.ident.clone();
    let krate = quote! { ::snb_core::plugin };

    let ffi = quote! {
        #input

        #[unsafe(no_mangle)]
        pub extern "C" fn create_plugin() -> *mut Box<dyn #krate::SnbPlugin> {
            let inner: Box<dyn #krate::SnbPlugin> = Box::new(<#name as #krate::SnbPlugin>::new());
            Box::into_raw(Box::new(inner))
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn plugin_abi() -> *const std::ffi::c_char {
            static ABI: std::sync::OnceLock<std::ffi::CString> = std::sync::OnceLock::new();
            ABI.get_or_init(|| std::ffi::CString::new(#krate::snb_plugin_abi().to_string()).unwrap())
                .as_ptr()
        }
        #[unsafe(no_mangle)]
        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        pub extern "C" fn destroy_plugin(ptr: *mut Box<dyn #krate::SnbPlugin>) {
            if !ptr.is_null() {
                unsafe { drop(Box::from_raw(ptr)); }
            }
        }
    };

    let args = MetaArgs::parse(attr)?;
    if args.get("name").is_none() {
        // Bare form: FFI only; the author writes the `SnbPlugin` impl.
        return Ok(ffi);
    }

    let name_lit = lit_str(&args, "name", &input)?;
    let (major, minor, patch) = parse_version(&args, &input)?;
    let kind = args.require("kind", &input)?;

    let impl_block = quote! {
        impl #krate::SnbPlugin for #name {
            fn new() -> Self {
                Self
            }
            fn name(&self) -> &str {
                #name_lit
            }
            fn version(&self) -> #krate::Version {
                #krate::Version { major: #major, minor: #minor, patch: #patch }
            }
            fn plugin_type(&self) -> #krate::PluginType {
                #krate::PluginType::#kind
            }
            fn on_load(&mut self, ctx: ::std::sync::Arc<dyn ::snb_core::context::BotContext>) {
                ::snb_core::context::set_bot(ctx);
                ::snb_core::context::register_all(#name_lit);
                ::log::info!("v{}.{}.{} loaded!", #major, #minor, #patch);
            }
            fn on_unload(&mut self) {}
        }
    };

    Ok(quote! {
        #ffi
        #impl_block
    })
}

fn lit_str<'a>(args: &'a MetaArgs, key: &str, src: &ItemStruct) -> syn::Result<&'a LitStr> {
    let expr = args.require(key, src)?;
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) => Ok(s),
        _ => Err(syn::Error::new_spanned(
            expr,
            format!("`{key}` must be a string literal"),
        )),
    }
}

fn parse_version(args: &MetaArgs, src: &ItemStruct) -> syn::Result<(u32, u32, u32)> {
    let lit = lit_str(args, "version", src)?;
    let value = lit.value();
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() != 3 {
        return Err(syn::Error::new_spanned(
            lit,
            "`version` must be \"major.minor.patch\"",
        ));
    }
    let parse = |p: &str| {
        p.parse::<u32>()
            .map_err(|e| syn::Error::new_spanned(lit, format!("invalid version component: {e}")))
    };
    Ok((parse(parts[0])?, parse(parts[1])?, parse(parts[2])?))
}
