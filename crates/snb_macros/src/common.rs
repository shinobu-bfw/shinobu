use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{Expr, ItemFn, Lit, Token, parse2};

/// Parsed `key = value` attribute arguments (e.g. `name = "echo", priority = 5`).
pub struct MetaArgs {
    pairs: Vec<(String, Expr)>,
}

impl MetaArgs {
    pub fn parse(attr: TokenStream) -> syn::Result<Self> {
        if attr.is_empty() {
            return Ok(Self { pairs: Vec::new() });
        }
        let parser = Punctuated::<syn::MetaNameValue, Token![,]>::parse_terminated;
        let parsed = parser.parse2(attr)?;
        let pairs = parsed
            .into_iter()
            .map(|nv| {
                let key = nv
                    .path
                    .get_ident()
                    .map(std::string::ToString::to_string)
                    .unwrap_or_default();
                (key, nv.value)
            })
            .collect();
        Ok(Self { pairs })
    }

    pub fn get(&self, key: &str) -> Option<&Expr> {
        self.pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn require<T: ToTokens>(&self, key: &str, span_src: &T) -> syn::Result<&Expr> {
        self.get(key).ok_or_else(|| {
            syn::Error::new_spanned(span_src, format!("missing required `{key} = ...` argument"))
        })
    }
}

/// Parse the annotated item as a free function.
pub fn parse_fn(item: TokenStream) -> syn::Result<ItemFn> {
    parse2::<ItemFn>(item)
}

/// Build a `fn name(&self) -> &str` body from a `name = "..."` arg.
pub fn name_method<T: ToTokens>(args: &MetaArgs, src: &T) -> syn::Result<TokenStream> {
    let value = args.require("name", src)?;
    let lit = match value {
        Expr::Lit(syn::ExprLit {
            lit: Lit::Str(s), ..
        }) => s,
        _ => {
            return Err(syn::Error::new_spanned(
                value,
                "`name` must be a string literal",
            ));
        }
    };
    Ok(quote! {
        fn name(&self) -> &str { #lit }
    })
}

/// Build a `fn priority(&self) -> u32` body from an optional `priority = N` arg.
pub fn priority_method(args: &MetaArgs) -> TokenStream {
    if let Some(expr) = args.get("priority") { quote! { fn priority(&self) -> u32 { #expr } } } else { quote! { fn priority(&self) -> u32 { 0 } } }
}
