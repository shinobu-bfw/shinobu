use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{FnArg, Ident, Pat};

use crate::common::parse_fn;

/// `#[adapter]` on a free `async fn`:
///
/// ```ignore
/// #[adapter]
/// async fn demo(bot: Arc<dyn BotContext>) { ... }
/// ```
///
/// Generates a hidden unit struct implementing [`Adapter`], whose sync `run`
/// drives the async function via `run_async` (tokio runtime created inside the
/// plugin's own cdylib), and registers it via `inventory` so
/// [`register_all`](snb_core::context::register_all) picks it up.
///
/// Adapters that need state should keep it in module-level globals (mirroring
/// the framework's own `context::set_bot`), so the function stays stateless.
pub fn expand(_attr: TokenStream, item: TokenStream) -> TokenStream {
    match try_expand(item) {
        Ok(ts) => ts,
        Err(e) => e.to_compile_error(),
    }
}

fn try_expand(item: TokenStream) -> syn::Result<TokenStream> {
    let func = parse_fn(item)?;
    if func.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &func.sig,
            "`#[adapter]` requires `async fn ...(bot: Arc<dyn BotContext>)`",
        ));
    }
    let fn_name = &func.sig.ident;
    let (bot_pat, bot_ty) = first_param(&func.sig)?;
    let ty = Ident::new(&format!("__SnbAdapter_{fn_name}"), Span::call_site());

    Ok(quote! {
        #func

        #[doc(hidden)]
        #[derive(Clone, Copy)]
        struct #ty;

        impl ::snb_core::adapter::Adapter for #ty {
            fn run(&self, #bot_pat: #bot_ty) {
                ::snb_core::adapter::run_async(#fn_name(#bot_pat));
            }
        }

        ::snb_core::registry::submit! {
            ::snb_core::registry::AdapterRegistration {
                factory: || ::std::sync::Arc::new(#ty),
            }
        }
    })
}

/// Extract the first non-receiver parameter's `(ident, type)`.
fn first_param(sig: &syn::Signature) -> syn::Result<(Ident, Box<syn::Type>)> {
    let arg = sig
        .inputs
        .iter()
        .find(|a| matches!(a, FnArg::Typed(_)))
        .ok_or_else(|| syn::Error::new_spanned(sig, "adapter must take a `BotContext` argument"))?;
    match arg {
        FnArg::Typed(pt) => match &*pt.pat {
            Pat::Ident(id) => Ok((id.ident.clone(), pt.ty.clone())),
            _ => Err(syn::Error::new_spanned(
                &pt.pat,
                "adapter parameter must be a simple identifier",
            )),
        },
        _ => unreachable!(),
    }
}
