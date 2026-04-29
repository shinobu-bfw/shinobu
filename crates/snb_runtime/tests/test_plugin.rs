use std::process::Command;
use std::sync::Once;

static BUILD_LIB: Once = Once::new();

fn ensure_lib_built() {
    BUILD_LIB.call_once(|| {
        let status = Command::new("cargo")
            .args(["build", "--lib", "-p", "snb_plugin_example"])
            .status()
            .expect("failed to build dynamic library");
        assert!(status.success(), "building dynamic library failed");
    });
}

#[test]
fn test_plugin_load_and_commands() {
    use snb_core::bot::BotInfo;
    use snb_core::context::BotContext;
    use snb_runtime::bot::Bot;
    use snb_runtime::logger::StdoutLogger;
    use snb_runtime::plugin_manager::PluginLoader;
    use std::sync::Arc;

    let bot = Arc::new(Bot::new(
        BotInfo {
            name: "TestBot".into(),
        },
        Arc::new(StdoutLogger::new(snb_core::logger::LogLevel::Info)),
        std::env::current_dir()
            .unwrap()
            .join("..")
            .join("..")
            .join("configs"),
        std::env::current_dir()
            .unwrap()
            .join("..")
            .join("..")
            .join("data"),
    ));

    snb_core::context::set_bot(bot.clone());

    let loader = PluginLoader::new(bot.clone());
    ensure_lib_built();
    let lib_name = if cfg!(target_os = "windows") {
        "snb_plugin_example.dll"
    } else if cfg!(target_os = "macos") {
        "libsnb_plugin_example.dylib"
    } else {
        "libsnb_plugin_example.so"
    };
    let plugin_path = std::env::current_dir()
        .unwrap()
        .join("..")
        .join("..")
        .join("target")
        .join("debug")
        .join(lib_name);
    loader.load_plugin(plugin_path).unwrap();

    // Plugin is registered
    let plugins = bot.list_plugins();
    assert!(
        plugins.contains(&"MyPlugin".to_string()),
        "plugin not in list: {:?}",
        plugins
    );

    // Plugin is reachable via get_plugin
    let info = bot.get_plugin("MyPlugin").unwrap();
    assert_eq!(info.name, "MyPlugin");
    assert_eq!(
        info.version,
        snb_core::plugin::Version {
            major: 0,
            minor: 1,
            patch: 0
        }
    );

    loader.unload_plugin("MyPlugin").unwrap();
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
struct NestedConfig {
    enabled: bool,
    tags: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
struct ExampleConfig {
    key: String,
    count: i64,
    nested: NestedConfig,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
struct TestConfig {
    example: ExampleConfig,
}

#[test]
fn test_load_config() {
    use snb_core::bot::BotInfo;
    use snb_core::context::BotContext;
    use snb_runtime::bot::Bot;
    use snb_runtime::logger::StdoutLogger;
    use std::path::Path;
    use std::sync::Arc;

    let config_dir = std::env::current_dir()
        .unwrap()
        .join("..")
        .join("..")
        .join("configs");
    let bot = Arc::new(Bot::new(
        BotInfo {
            name: "TestBot".into(),
        },
        Arc::new(StdoutLogger::new(snb_core::logger::LogLevel::Info)),
        config_dir,
        std::env::current_dir()
            .unwrap()
            .join("..")
            .join("..")
            .join("data"),
    ));

    // Deserialize from config text
    let text = bot.load_config(Path::new("test.toml")).unwrap();
    let config: TestConfig = toml::from_str(&text).unwrap();

    assert_eq!(config.example.key, "value");
    assert_eq!(config.example.count, 42);
    assert!(config.example.nested.enabled);
    assert_eq!(config.example.nested.tags, vec!["a", "b", "c"]);

    // Serialize back and roundtrip
    let serialized = toml::to_string(&config).unwrap();
    let roundtripped: TestConfig = toml::from_str(&serialized).unwrap();
    assert_eq!(config, roundtripped);

    // Non-existent file
    let err = bot.load_config(Path::new("no_such_file.toml")).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn test_write_config_ownership() {
    use snb_core::bot::BotInfo;
    use snb_core::context::BotContext;
    use snb_runtime::bot::Bot;
    use snb_runtime::logger::StdoutLogger;
    use std::path::Path;
    use std::sync::Arc;

    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path();

    let data_dir = tmp.path().join("data");
    let bot = Arc::new(Bot::new(
        BotInfo {
            name: "TestBot".into(),
        },
        Arc::new(StdoutLogger::new(snb_core::logger::LogLevel::Info)),
        config_dir.to_path_buf(),
        data_dir,
    ));

    // plugin_a writes its own config — should succeed
    bot.write_config("plugin_a", Path::new("data.toml"), "[config]\nkey = \"a\"")
        .unwrap();
    let content = bot.load_config(Path::new("plugin_a/data.toml")).unwrap();
    assert_eq!(content, "[config]\nkey = \"a\"");

    // plugin_a tries to write into plugin_b's namespace — should be blocked
    let err = bot
        .write_config(
            "plugin_a",
            Path::new("../plugin_b/evil.toml"),
            "[evil]\nhacked = true",
        )
        .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);

    // plugin_b's config should be untouched
    let err = bot
        .load_config(Path::new("plugin_b/evil.toml"))
        .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);

    // Path traversal via .. — should be blocked
    let err = bot
        .write_config("plugin_a", Path::new("../../etc/passwd"), "root:x:0:0")
        .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
}
