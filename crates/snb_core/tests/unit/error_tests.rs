use super::*;
use std::error::Error;

#[test]
fn plugin_error_display() {
    assert_eq!(PluginError::LoadError.to_string(), "failed to load plugin");
    assert_eq!(
        PluginError::UnloadError.to_string(),
        "failed to unload plugin"
    );
    assert_eq!(PluginError::InvalidPlugin.to_string(), "invalid plugin");
    assert_eq!(PluginError::BrokenAbi.to_string(), "broken plugin ABI");
    assert_eq!(
        PluginError::UnsupportedAbi.to_string(),
        "unsupported plugin ABI"
    );
    assert_eq!(
        PluginError::DuplicatePlugin.to_string(),
        "a plugin with this name is already loaded"
    );
    assert_eq!(
        PluginError::ComponentConflict.to_string(),
        "plugin component name conflict"
    );
}

#[test]
fn plugin_error_is_error() {
    let err: Box<dyn Error> = Box::new(PluginError::LoadError);
    assert_eq!(err.to_string(), "failed to load plugin");
}
