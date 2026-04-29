use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemStruct, parse_macro_input};

pub fn new_plugin(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    let name = &input.ident;

    let krate = quote! { ::snb_core::plugin };

    let struct_ori = &input;

    let expanded = quote! {
        #struct_ori
        #[unsafe(no_mangle)]
        pub extern "C" fn create_plugin() -> *mut Box<dyn #krate::SnbPlugin> {
            let inner: Box<dyn #krate::SnbPlugin> = Box::new(<#name>::new());
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

    TokenStream::from(expanded)
}
