use std::error::Error;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result;

/// Errors that can occur during plugin lifecycle operations.
#[derive(Debug)]
pub enum PluginError {
    /// Failed to load the plugin shared library.
    LoadError,
    /// Failed to unload the plugin.
    UnloadError,
    /// The loaded library is not a valid Shinobu plugin.
    InvalidPlugin,
    /// The plugin's internal ABI version does not match the FFI-exported version.
    BrokenAbi,
    /// The plugin's ABI major version is incompatible with the host.
    UnsupportedAbi,
    /// A plugin with the same name is already loaded.
    DuplicatePlugin,
    /// The plugin registered a component (command, hook, …) whose name is
    /// already taken by another plugin.
    ComponentConflict,
}

impl Display for PluginError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Self::LoadError => write!(f, "failed to load plugin"),
            Self::UnloadError => write!(f, "failed to unload plugin"),
            Self::InvalidPlugin => write!(f, "invalid plugin"),
            Self::BrokenAbi => write!(f, "broken plugin ABI"),
            Self::UnsupportedAbi => write!(f, "unsupported plugin ABI"),
            Self::DuplicatePlugin => write!(f, "a plugin with this name is already loaded"),
            Self::ComponentConflict => write!(f, "plugin component name conflict"),
        }
    }
}

impl Error for PluginError {}

#[cfg(test)]
#[path = "../tests/unit/error_tests.rs"]
mod error_tests;
