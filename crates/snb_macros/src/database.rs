use proc_macro2::TokenStream;
use quote::quote;

use crate::common::parse_fn;

/// `#[database]` on a free function that builds a driver:
///
/// ```ignore
/// #[database]
/// fn sqlite() -> SqliteDatabase {
///     let path = context::bot().data_dir("sqlite").join("data.db");
///     SqliteDatabase::new("sqlite", path)
/// }
/// ```
///
/// The function returns any `DatabaseDriver`; the macro registers it via
/// `inventory` so [`register_all`](snb_core::context::register_all)
/// constructs and registers it (after `set_bot`, so the body may read context).
///
/// Unlike the behaviour macros, no metadata is needed: the driver supplies its
/// own [`name`](snb_core::database::DatabaseDriver::name), and the function body
/// owns the runtime construction (paths, connections).
pub fn expand(_attr: TokenStream, item: TokenStream) -> TokenStream {
    match try_expand(item) {
        Ok(ts) => ts,
        Err(e) => e.to_compile_error(),
    }
}

fn try_expand(item: TokenStream) -> syn::Result<TokenStream> {
    let func = parse_fn(item)?;
    let fn_name = &func.sig.ident;

    Ok(quote! {
        #func

        ::snb_core::registry::submit! {
            ::snb_core::registry::DatabaseRegistration {
                factory: || ::std::sync::Arc::new(#fn_name()),
            }
        }
    })
}
