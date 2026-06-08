use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, Once};

use snb_core::adapter::Adapter;
use snb_core::context::BotContext;
use snb_core::event::{ContentItem, Event, FileSource};

static BUILD_LIB: Once = Once::new();
static INIT_LOGGER: Once = Once::new();

fn ensure_lib_built() {
    BUILD_LIB.call_once(|| {
        let status = Command::new("cargo")
            .args(["build", "--lib", "-p", "snb_plugin_example"])
            .status()
            .expect("failed to build dynamic library");
        assert!(status.success(), "building dynamic library failed");
    });
}

fn init_test_logger() {
    INIT_LOGGER.call_once(|| {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .is_test(true)
            .try_init();
    });
}

struct TestFileCleanup(PathBuf);

impl Drop for TestFileCleanup {
    fn drop(&mut self) {
        if self.0.is_dir() {
            let _ = std::fs::remove_dir_all(&self.0);
        } else {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

#[test]
fn test_plugin_load_and_commands() {
    use snb_core::bot::BotInfo;
    use snb_core::context::BotContext;
    use snb_runtime::bot::Bot;
    use snb_runtime::logger::EnvLogger;
    use snb_runtime::plugin_manager::PluginLoader;
    use std::sync::Arc;

    init_test_logger();

    let bot = Arc::new(Bot::new(
        BotInfo {
            name: "TestBot".into(),
        },
        Arc::new(EnvLogger::new()),
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
    use snb_runtime::logger::EnvLogger;
    use std::path::Path;
    use std::sync::Arc;

    init_test_logger();

    let config_dir = std::env::current_dir()
        .unwrap()
        .join("..")
        .join("..")
        .join("configs");

    // Create test.toml temporarily
    let test_config_path = config_dir.join("test.toml");
    let test_config_content = r#"[example]
key = "value"
count = 42

[example.nested]
enabled = true
tags = ["a", "b", "c"]
"#;
    std::fs::write(&test_config_path, test_config_content).unwrap();

    // Ensure cleanup on test exit
    let _cleanup = TestFileCleanup(test_config_path.clone());

    let bot = Arc::new(Bot::new(
        BotInfo {
            name: "TestBot".into(),
        },
        Arc::new(EnvLogger::new()),
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
    use snb_runtime::logger::EnvLogger;
    use std::path::Path;
    use std::sync::Arc;

    init_test_logger();

    let tmp = std::env::temp_dir().join(format!("snb_test_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let _cleanup = TestFileCleanup(tmp.clone());

    let config_dir = tmp.clone();
    let data_dir = tmp.join("data");
    let bot = Arc::new(Bot::new(
        BotInfo {
            name: "TestBot".into(),
        },
        Arc::new(EnvLogger::new()),
        config_dir,
        data_dir,
    ));

    // plugin_a writes its own config; should succeed
    bot.write_config("plugin_a", Path::new("data.toml"), "[config]\nkey = \"a\"")
        .unwrap();
    let content = bot.load_config(Path::new("plugin_a/data.toml")).unwrap();
    assert_eq!(content, "[config]\nkey = \"a\"");

    // plugin_a tries to write into plugin_b's namespace; should be blocked
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

    // Path traversal via ..; should be blocked
    let err = bot
        .write_config("plugin_a", Path::new("../../etc/passwd"), "root:x:0:0")
        .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
}

#[derive(Default)]
struct CaptureAdapter {
    sent: Mutex<Vec<Event>>,
}

impl Adapter for CaptureAdapter {
    fn run(&self, _bot: Arc<dyn BotContext>) {}

    fn send(&self, event: &Event) -> anyhow::Result<()> {
        self.sent.lock().unwrap().push(event.clone());
        Ok(())
    }
}

#[test]
fn test_send_file_to_adapter() {
    use snb_core::bot::BotInfo;
    use snb_runtime::bot::Bot;
    use snb_runtime::logger::EnvLogger;

    init_test_logger();

    let adapter = Arc::new(CaptureAdapter::default());
    let bot = Bot::new(
        BotInfo {
            name: "TestBot".into(),
        },
        Arc::new(EnvLogger::new()),
        std::env::current_dir().unwrap().join("configs"),
        std::env::current_dir().unwrap().join("data"),
    );

    bot.register_adapter("capture", adapter.clone());
    bot.emit_event(
        Event::file_message(
            "test",
            FileSource::Path("report.txt".to_string()),
            Some("report.txt".to_string()),
        )
        .with_receiver("capture"),
    );

    let sent = adapter.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    let message = sent[0].message.as_ref().unwrap();
    assert!(matches!(
        &message.content[0],
        ContentItem::File {
            source: FileSource::Path(path),
            file_name: Some(name),
            ..
        } if path == "report.txt" && name == "report.txt"
    ));
}
