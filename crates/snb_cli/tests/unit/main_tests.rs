use super::*;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("shinobu-cli-test-{}-{}", name, std::process::id()));
        if path.exists() {
            std::fs::remove_dir_all(&path).unwrap();
        }
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).ok();
    }
}

fn make_project_root(path: &Path) {
    std::fs::write(path.join("Cargo.toml"), "[workspace]\n").unwrap();
    std::fs::create_dir_all(path.join("crates").join("snb_cli")).unwrap();
    std::fs::write(
        path.join("crates").join("snb_cli").join("Cargo.toml"),
        "[package]\nname = \"snb_cli\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(path.join("crates").join("snb_core")).unwrap();
    std::fs::write(
        path.join("crates").join("snb_core").join("Cargo.toml"),
        "[package]\nname = \"snb_core\"\n",
    )
    .unwrap();
}

#[test]
fn runtime_root_walks_up_from_plugin_dir_to_project_root() {
    let temp = TempDir::new("plugin-cwd");
    make_project_root(&temp.path);
    let plugin_dir = temp.path.join("plugins").join("snb_adapter_tg");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("Cargo.toml"),
        "[package]\nname = \"plugin\"\n",
    )
    .unwrap();
    let exe_dir = temp.path.join("target").join("debug");

    assert_eq!(resolve_runtime_root(&plugin_dir, &exe_dir), temp.path);
}

#[test]
fn runtime_root_falls_back_to_exe_dir_without_project_root() {
    let temp = TempDir::new("exe-fallback");
    let cwd = temp.path.join("elsewhere");
    let exe_dir = temp.path.join("bin");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&exe_dir).unwrap();

    assert_eq!(resolve_runtime_root(&cwd, &exe_dir), exe_dir);
}
