use super::*;

#[test]
fn split_build_plugin_args_keeps_named_plugin() {
    let args = vec![
        OsString::from("snb_plugin_example"),
        OsString::from("--release"),
    ];
    let (plugin, cargo_args) = split_build_plugin_args(&args);

    assert_eq!(plugin, Some(&args[0]));
    assert_eq!(cargo_args, &args[1..]);
}

#[test]
fn split_build_plugin_args_treats_leading_flag_as_cargo_args() {
    let args = vec![OsString::from("--release")];
    let (plugin, cargo_args) = split_build_plugin_args(&args);

    assert_eq!(plugin, None);
    assert_eq!(cargo_args, &args[..]);
}

#[test]
fn resolve_plugin_dot_relative_to_current_plugin_dir() {
    let root = workspace_root();
    let current_dir = root.join("plugins").join("snb_plugin_example");

    let plugin = resolve_plugin(&root, &current_dir, &OsString::from(".")).unwrap();

    assert_eq!(plugin.name, "snb_plugin_example");
}

#[test]
fn resolve_plugin_uses_root_relative_path_as_fallback() {
    let root = workspace_root();
    let current_dir = root.join("crates").join("xtask");

    let plugin = resolve_plugin(
        &root,
        &current_dir,
        &OsString::from("plugins/snb_plugin_example"),
    )
    .unwrap();

    assert_eq!(plugin.name, "snb_plugin_example");
}

#[test]
fn plugin_matches_snb_source_package_name() {
    let root = workspace_root();
    let plugin = PluginBuild {
        name: "custom_plugin_dir".to_string(),
        manifest: generated_snb_manifest(&root, "custom_plugin_dir"),
        kind: PluginBuildKind::SnbSource(SnbSourceBuild {
            plugin_dir: root.join("plugins").join("custom_plugin_dir"),
            source: root
                .join("plugins")
                .join("custom_plugin_dir")
                .join("src")
                .join("compat.rs"),
            package_name: "snb_custom_plugin".to_string(),
            package_version: "0.1.0".to_string(),
            dependency_name: "custom_plugin".to_string(),
            dependencies: toml::Table::new(),
        }),
    };

    assert!(plugin_matches(&plugin, "snb_custom_plugin"));
}

#[test]
fn find_current_plugin_dir_walks_up_from_plugin_subdir() {
    let root = workspace_root();
    let current_dir = root.join("plugins").join("snb_plugin_example").join("src");

    let plugin_dir = find_current_plugin_dir(&root, &current_dir).unwrap();

    assert_eq!(plugin_dir, root.join("plugins").join("snb_plugin_example"));
}
