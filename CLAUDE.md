# Shinobu — Agent Guide

Shinobu is an extensible bot framework in Rust. Plugins are compiled to native
shared libraries (`cdylib`: `.so`/`.dll`/`.dylib`) and **dynamically loaded at
runtime**. The host knows nothing about a plugin at compile time; everything
crosses a C-ABI + `dyn`-trait boundary. That single fact drives most of the
design below — read the section **The dynamic-loading model** below
before changing anything in `crates/`.

## Workspace layout

The root `Cargo.toml` workspace has two kinds of members:

| Crate | Role |
|-------|------|
| `crates/snb_core` | **Plugin-facing API surface.** Traits (`SnbPlugin`, `BotContext`, `Adapter`, `CommandHandler`, `Hook`, `MessageHandler`, `DatabaseDriver`), the `Event` type, FFI infra (`PluginCell`), ABI version, the `inventory` registry, and `register_all`. Statically linked into **every** plugin. |
| `crates/snb_macros` | Proc macros: `#[plugin]`, `#[command]`, `#[hook]`, `#[message_handler]`, `#[adapter]`, `#[database]`. Generate trait impls + `inventory::submit!` registrations + FFI exports. |
| `crates/snb_runtime` | The concrete `Bot` (implements `BotContext`), the dynamic `PluginLoader`, `EnvLogger`, `InMemorySessionManager`. The dispatch loop lives in [`bot.rs`](crates/snb_runtime/src/bot.rs). |
| `crates/snb_status` | Runtime status snapshots (uptime, platform, process memory). Standalone, no plugin deps. |
| `crates/snb_cli` | The host binary (`cargo run`). Loads plugins, starts adapters, handles shutdown. |
| `crates/xtask` | Build tool behind the `cargo xtask` / `cargo ba`/`bp`/… aliases. Builds plugin `cdylib`s and resolves `snb.toml` plugins. |
| `plugins/snb_adapter_stdin`, `plugins/snb_database_sqlite`, `plugins/snb_plugin_example` | **Reference plugins**, tracked in this repo and part of the root workspace. The best worked examples of each component type. |

**External plugins** (`snb_adapter_tg`, `snb_plugin_manager`,
`snb_plugin_payload_extract`) are **separate git repos with their own Cargo
workspaces** (note the empty `[workspace]` in their `Cargo.toml`), dropped into
`plugins/` and gitignored here. They are *not* root-workspace members and are
built/tested from their own directories. Treat them as real-world examples, not
as code you own from this repo.

## Build, run, test

The host runs the bot; `cargo xtask` builds the plugin `cdylib`s. Aliases are in
[.cargo/config.toml](.cargo/config.toml).

```sh
cargo run                       # build + run the host (snb_cli)
cargo xtask build-all   # cargo ba   # build workspace + ALL plugins in plugins/
cargo xtask build-plugins # cargo bps # build only non-example plugins
cargo xtask build-plugin tg # cargo bp tg # one plugin (fuzzy name: tg → snb_adapter_tg)
cargo xtask build-example # cargo be  # the three reference plugins
cargo xtask list-plugins  # cargo lp  # list discovered plugins
cargo xtask build-plugin tg --release  # extra args pass through to cargo
cargo test --workspace          # tests for crates + the 3 reference plugins
```

**Loading:** the host scans for plugin libraries in two places, in priority
order — (1) the executable's directory (Cargo's `target/<profile>/`), then (2)
`./plugins/`. A library in (1) shadows a same-named file in (2). Only files
named `libsnb_*` / `snb_*` with a dynlib extension are considered. So after
`cargo xtask build-all` (debug), `cargo run` picks the freshly built `.dll`/`.so`
out of `target/debug/`. Rebuild a plugin and restart the host to load changes.

Tests live in `tests/unit/<module>_tests.rs` and are pulled into the crate as
inline modules:

```rust
#[cfg(test)]
#[path = "../tests/unit/event_tests.rs"]
mod event_tests;
```

Match this convention when adding tests next to a source module.

## The dynamic-loading model

This is the load-bearing idea. Internalize it.

- **Each plugin is its own `cdylib` and statically links its own copy of
  `snb_core`.** There is no shared `snb_core` between host and plugins.
- **Statics are per-compilation-unit.** `snb_core`'s `BOT` global and the
  `inventory` collections exist *separately* in the host and in each plugin
  `.so`. That is why:
  - every plugin must call `set_bot(ctx)` in `on_load` (its own `BOT` is empty);
  - `register_all` **must live in `snb_core`** (statically linked) so
    `inventory::iter` reads *that plugin's* submissions, not the host's empty set.
- **Types crossing the boundary must have identical layout in host and plugin.**
  `Event`, the trait vtables (`dyn BotContext`, `dyn SnbPlugin`, …), `PluginCell`,
  `Version` — all are compiled into both sides from each side's own `snb_core`.
  A layout mismatch is undefined behavior, not a compile error. The **ABI
  version** is the guardrail (see below).
- **FFI contract.** `#[plugin]` exports `create_plugin` /  `destroy_plugin` /
  `plugin_abi`. The loader ([`Bot::load_plugin`](crates/snb_runtime/src/bot.rs))
  resolves these symbols, checks the ABI, and wraps the plugin in a `PluginCell`
  that keeps the `libloading::Library` alive for the plugin's lifetime.

### ABI versioning

The ABI version is `[workspace.metadata.snb].abi_version` in the root
`Cargo.toml` (that file is the source of truth — don't restate the number
here). [`snb_core/build.rs`](crates/snb_core/build.rs)
reads it into the `SNB_ABI_VERSION` env at compile time; `snb_plugin_abi()`
exposes it. The loader enforces (see `plugin.rs` doc comment and `load_plugin`):

- **major** mismatch → reject.
- **minor** newer than the host → reject.
- **minor** older than the host, **while major == 0** → reject (in `0.x` the
  minor *is* the breaking position; an older minor is an incompatible vtable).
- **minor** older, major ≥ 1 → load with a warning (additive change).
- **patch** difference → warn only.

**Bump the abi_version minor (while we are pre-1.0) on any change to the
plugin-facing ABI**, then rebuild every plugin. See the
`shinobu-core-changes` skill for the exact checklist.

## Event flow & dispatch

`Event` (in [`event.rs`](crates/snb_core/src/event.rs)) is the unit of
communication. Adapters emit events in; plugins emit events out, all through
`BotContext::emit_event`. Key fields:

- `event_type: EventType` (`Command`, `Message`, `MessageSent/Edit/Delete`,
  `PluginLoaded/Unloaded`, `Other`).
- `command` / `message` — the structured payload (`Some` per the type).
- `source` — the adapter/origin name (free text).
- `reply_plugin` — origin plugin that expects a response routed back to it.
- `target_plugin` — directed routing; `None` broadcasts.
- `Message` carries structured `sender: Option<Sender>`, `chat: Chat`,
  `is_admin`, `content: Vec<ContentItem>`, `reply_to`, `delete_after`. Read
  ids via `msg.sender_id()` / `msg.chat_id()`; platform-specific fields go in
  `Sender::extra` / `Chat::extra` (e.g. `chat.extra["raw_kind"]`), not new typed
  fields.

`emit_event` (in `bot.rs`) does, in order:
1. If `target_plugin` names an adapter that accepts it via `send`, deliver and
   return (this is how outgoing messages reach an adapter).
2. Run **Main-phase hooks** exactly once.
3. Dispatch by type: `Command` → `dispatch_command` (admin gate → BeforeCommand
   hooks → handler → AfterCommand hooks); `Message` → all message handlers
   (priority-sorted).
4. Broadcast `on_event` to plugin cells (or just `target_plugin`), each wrapped
   in `catch_unwind`.

**Admin gating:** a command whose `visibility()` is `Admin` is silently ignored
unless `event.message.is_admin` is true. Adapters set `is_admin`.

## Invariants & gotchas (don't break these)

- **`panic = "unwind"` is mandatory** (`[profile.release]` and the generated
  `snb.toml` manifests). Adapter bodies (`run_async`) and `on_event` use
  `catch_unwind` to contain a plugin panic instead of aborting the host. A
  foreign-frame unwind across the cdylib→host boundary would abort the process,
  so panics are always caught *inside* the plugin's own dylib.
- **Unload is a drain protocol, not a free.** `unregister_plugin` removes
  registry entries, calls `Adapter::stop()`, then waits (≤1s) until it is the
  sole owner of every component `Arc` and the `PluginCell` before running
  `on_unload` and `dlclose`-ing. If a holder never releases (e.g. an adapter
  blocked on I/O), it **leaks the library** rather than unmap code a live thread
  is running. New long-running work a plugin spawns must be stoppable via
  `Adapter::stop`.
- **Component names are globally unique and conflicts roll back the whole
  plugin.** Registration *refuses* (never overwrites) a duplicate command /
  alias / hook / message-handler name; the loader then unloads the offending
  plugin. Pick distinctive names.
- **Config/data are namespaced and path-confined.** `data_dir`,
  `load_config`, `write_config` resolve under `data/<plugin>/` and
  `configs/<plugin>/`; `..` traversal is rejected. Plugins pass paths *below*
  their namespace and never their own name. Config writes are atomic (tmp +
  rename).
- **Route logs through the framework.** Use `log::info!`/`debug!`/… (bridged to
  the bot logger once `set_bot` runs) or `ctx.logger()`. Never `println!`
  for diagnostics (stdout is reserved — the stdin adapter prints replies there).

## Conventions

- Edition and MSRV (Rust version) are set in the workspace `Cargo.toml` — read
  them there rather than relying on a copy here. Errors are `anyhow::Result` at
  trait boundaries; `snb_core::error::PluginError` for lifecycle failures.
- Doc comments are dense and explain *why* (especially the unsafe/FFI/unload
  code). Preserve and extend that style — a non-obvious safety or ordering
  reason belongs in a comment.
- Plugin crate naming: `snb_adapter_*`, `snb_database_*`, `snb_plugin_*`.
- `configs/` (except `bot.toml`), `data/`, `target/`, and non-reference
  `plugins/*` are gitignored.

## When working in this repo

- **Writing or adapting a plugin** (a new adapter/command/hook/handler/driver,
  or porting an existing one) → use the **`shinobu-plugin-development`** skill.
- **Changing `snb_core`, `snb_macros`, or `snb_runtime`** — anything that could
  shift the plugin-facing ABI → use the **`shinobu-core-changes`** skill. It has
  the ABI-bump and rebuild checklist that keeps already-built plugins safe.
