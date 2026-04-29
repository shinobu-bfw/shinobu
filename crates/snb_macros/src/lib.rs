//! Proc macros for the Shinobu plugin framework.
//!
//! Provides the `#[plugin]` attribute macro that generates the FFI boilerplate
//! required for dynamically loaded plugins (`create_plugin`, `destroy_plugin`,
//! `plugin_abi` exports).

mod plugin;

use proc_macro::TokenStream;

/// Generate FFI exports for a Shinobu plugin struct.
///
/// Apply this to a struct that implements `SnbPlugin`.
/// It generates three `extern "C"` functions:
///
/// - `create_plugin` — allocates the plugin and returns a raw pointer.
/// - `destroy_plugin` — deallocates the plugin from a raw pointer.
/// - `plugin_abi` — returns the ABI version string from `Cargo.toml`.
#[proc_macro_attribute]
pub fn plugin(_attr: TokenStream, input: TokenStream) -> TokenStream {
    plugin::new_plugin(input)
}
