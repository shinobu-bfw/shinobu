---
name: shinobu-plugin-development
description: Use when writing, adapting, porting, or debugging a Shinobu plugin — any adapter, command, hook, message handler, or database driver. Covers the #[plugin]/#[command]/#[hook]/#[message_handler]/#[adapter]/#[database] macros, BotContext, the Event flow, config/data access, plugin-global state, building with cargo xtask, the snb.toml manifest, and dynamic loading.
---

# Shinobu plugin development

A plugin is a `cdylib` that the host loads at runtime over a C-ABI + `dyn`-trait
boundary. You write Rust against `snb_core` + `snb_macros`; the macros generate
the FFI exports and auto-register your components.

**Before writing code, read the closest reference plugin** — they are the
canonical patterns and are kept correct:

- Commands / hooks / message handlers / session / a demo adapter →
  [`plugins/snb_plugin_example/src/lib.rs`](../../../plugins/snb_plugin_example/src/lib.rs)
- A minimal real adapter (incoming + outgoing + `stop`) →
  [`plugins/snb_adapter_stdin/src/lib.rs`](../../../plugins/snb_adapter_stdin/src/lib.rs)
- A database driver →
  [`plugins/snb_database_sqlite/src/lib.rs`](../../../plugins/snb_database_sqlite/src/lib.rs)
- A full platform adapter (config, plugin-global state, incoming `convert`,
  outgoing `send`, command-menu sync) → the `snb_adapter_tg` plugin dir.

## Checklist for a new plugin

1. Create `plugins/<snb_kind_name>/` with `Cargo.toml` (`crate-type =
   ["cdylib"]`, depend on `snb_core` + `snb_macros`). Copy a reference plugin's
   manifest. Crate naming: `snb_adapter_*` / `snb_database_*` / `snb_plugin_*`.
2. Declare the plugin struct with `#[plugin(...)]` (see forms below).
3. Declare components with the attribute macros (`#[command]`, `#[hook]`,
   `#[message_handler]`, `#[adapter]`, `#[database]`). They auto-register — you
   do **not** hand-write `register_*` calls.
4. If you need config or persistent state, set it up in `on_load` (requires the
   bare `#[plugin]` form — see the **Config & data** section).
5. Build with `cargo xtask build-plugin <name>` and run the host with
   `cargo run`. Rebuild + restart to reload.
6. Add tests under `tests/unit/` and wire them with `#[path = ...]` (see the
   **Testing** section).

## The `#[plugin]` macro: two forms

**Full form** (unit struct, no custom lifecycle) — the macro writes the entire
`SnbPlugin` impl, folding in `set_bot` + `register_all`:

```rust
use snb_macros::plugin;

#[plugin(name = "MyPlugin", version = "0.1.0", kind = Plugin)]
struct MyPlugin;
```

`kind` is `Plugin`, `Adapter`, or `DatabaseDriver` (a `PluginType` variant).

**Bare form** (`#[plugin]` with no args) — emits only the FFI exports
(`create_plugin` / `destroy_plugin` / `plugin_abi`); you hand-write the
`SnbPlugin` impl. Use this when you need a custom `on_load` (load config, build
clients), `on_event`, or `on_unload`:

```rust
#[plugin]
struct TGAdapter;

impl SnbPlugin for TGAdapter {
    fn new() -> Self { Self }
    fn name(&self) -> &str { "TGAdapter" }
    fn version(&self) -> Version { Version { major: 0, minor: 0, patch: 1 } }
    fn plugin_type(&self) -> PluginType { PluginType::Adapter }
    fn on_load(&mut self, ctx: Arc<dyn BotContext>) {
        context::set_bot(ctx);            // ALWAYS first — your BOT static is empty
        context::set_plugin(self.name()); // enables context::plugin() config/data helper
        // ... load config, build clients, store in plugin-global state ...
        context::register_all(self.name()); // registers all #[...]-declared components
        log::info!("v{} loaded!", self.version());
    }
    fn on_unload(&mut self) { /* reset plugin-global state for clean reload */ }
    fn on_event(&self, event: &Event) { /* react to PluginLoaded/Unloaded, etc. */ }
}
```

**`on_load` ordering is fixed: `set_bot` → (`set_plugin`, config) →
`register_all`.** `set_bot` installs the log bridge and the context; nothing
context-dependent works before it.

## The component macros

Each macro takes a free function and generates a hidden trait-impl type +
`inventory::submit!`. `register_all` constructs and registers them.

```rust
// Command. visibility = Public (default) | Admin | Hidden. Admin commands are
// silently ignored for non-admins and shown only to admins in command menus.
#[command(name = "echo", aliases = ["say"], description = "Echo text", visibility = Public)]
fn echo(ctx: &CommandContext) -> anyhow::Result<()> {
    snb_core::context::bot().emit_event(/* reply */);
    Ok(())
}

// Hook. kind = HookType variant. priority lower runs first (default 0).
#[hook(name = "log_hook", kind = HookType::All)]
fn log_hook(event: &mut Event) -> anyhow::Result<()> { Ok(()) }
// Hooks can MUTATE the event (e.g. rewrite command args under BeforeNamedCommand).

// Message handler — runs for every EventType::Message. priority lower first.
#[message_handler(name = "echo_handler")]
fn echo_handler(event: &Event) -> anyhow::Result<()> { Ok(()) }

// Adapter — an async fn driven on a dedicated OS thread. See Adapters below.
#[adapter]
async fn demo_tick(bot: Arc<dyn BotContext>) { /* loop emitting events */ }

// Database driver — a free fn returning any DatabaseDriver; runs after set_bot
// so the body may read context (e.g. data_dir).
#[database]
fn sqlite_driver() -> SqliteDatabase { /* build driver */ }
```

`name`/`aliases`/`hook name`/`handler name` are **globally unique across all
loaded plugins**. A clash makes the host refuse and roll back your entire
plugin — choose distinctive names.

## BotContext — what a plugin can call

Obtain it with `snb_core::context::bot()` (after `set_bot`). Highlights:

- `emit_event(event)` — push an event into the dispatch loop (your main output).
- `logger()` — the bot logger (or just use `log::*` macros).
- `get_me()` / `status()` / `commands()` / `list_plugins()` / `get_plugin(name)`.
- `get_session_manager()` — short-lived in-memory sessions (`SessionKey`,
  `SessionMessage`, `SessionState`); persist via a database driver if you need
  durability.
- `get_database(plugin_name)` → `Arc<dyn DatabaseDriver>`; then use the
  `DatabaseOps` builder API (`.table()`, `.insert()`, `.select()`, …).
- Config/data: `data_dir`, `load_config`, `write_config` (prefer the
  `context::plugin()` helper — see below).

## Config & data

Each plugin gets `configs/<plugin>/` and `data/<plugin>/`, path-confined (no
`..`). Two ways to access:

```rust
// Explicit (needs the plugin name each call):
let text = bot().load_config("MyPlugin", Path::new("config.toml"))?;

// Helper (set context::set_plugin(name) once in on_load, then):
let text = context::plugin().load_config(Path::new("config.toml"))?;
let db   = context::plugin().data_dir().join("state.db");
context::plugin().write_config(Path::new("config.toml"), DEFAULT_CONFIG)?;
```

Pass paths **relative to your namespace** — never include your own plugin name
as a path component. The canonical first-run pattern (see the TG adapter
`on_load`): try `load_config`; on error, `write_config` a `DEFAULT_CONFIG`
template and warn the user to edit it. Parse with whatever format you want
(TOML/JSON/…); the framework returns raw UTF-8.

## Plugin-global state

A plugin is a singleton (one cdylib, one instance) and the component functions
are free functions, so **keep shared state in module-level globals**, mirroring
`snb_core`'s own `set_bot`. Use `RwLock<Option<T>>` (not `OnceLock`) so
`on_unload` can reset it for a clean reload:

```rust
static CONFIG: RwLock<Option<Config>> = RwLock::new(None);
pub(crate) fn set_config(c: Config) { *CONFIG.write().unwrap() = Some(c); }
pub(crate) fn reset() { *CONFIG.write().unwrap() = None; }  // call from on_unload
```

See [`snb_adapter_tg/src/state.rs`].

## Adapters (incoming + outgoing)

An adapter is a long-running source/sink of external events. The `#[adapter]`
macro (or a hand-written `Adapter` impl) gives you three methods:

- **`run(&self, bot)`** — your loop. The macro wraps your `async fn` in
  `run_async`, which builds a **single-threaded tokio runtime inside your own
  cdylib** (do not rely on the host's runtime — plugins carry their own tokio
  statics). Convert each external update into an `Event` and `bot.emit_event` it.
  Tag inbound events with `.with_reply_plugin("<YourName>")` so replies can be
  routed back to you.
- **`stop(&self)`** — trip a stop flag (e.g. `static STOP: AtomicBool`). Called
  on unload before the host waits for your thread. **Must not block.** An
  adapter that never stops is leaked on unload. (Blocking reads like `stdin`
  can only check the flag between reads — acceptable; the host's leak-on-timeout
  fallback covers a truly idle reader.)
- **`send(&self, event)`** — deliver an outgoing event to the platform. The host
  calls this when an event's `target_plugin` is your adapter name. Inspect
  `event.event_type` (`Message`/`MessageEdit`/`MessageDelete`) and
  `message.content` items, and deliver what you support. Default impl bails
  ("does not support outgoing"); override for output-capable adapters.

**Incoming conversion** (build `Event` from a platform update): set
`message.chat.id` (the routing/reply target), `sender`, `is_admin`, `content`,
`reply_to`, and `event.source`. Parse a leading command into
`EventType::Command` + `Command{cmd,args}`; otherwise `EventType::Message`. Put
platform extras in `Sender::extra` / `Chat::extra`. Model:
[`snb_adapter_tg/src/convert.rs`].

**Outgoing send** maps `ContentItem`s (`Text` with optional `TextFormat`,
`File`, `Image`, `Other`) to platform calls, honoring `reply_to` and
`delete_after`. If the origin set `reply_plugin` and needs the sent id back,
emit a `MessageSent` event targeted at the origin. Model:
[`snb_adapter_tg/src/outgoing.rs`].

**Command-menu sync:** if your platform has a command menu, implement `on_event`
to call your sync routine on `PluginLoaded`/`PluginUnloaded`, reading
`bot().commands()` and partitioning by `CommandVisibility` (Public→everyone,
Admin→admins only, Hidden→never). Model: [`snb_adapter_tg/src/commands.rs`].

## Building & loading

```sh
cargo xtask build-plugin <name>            # fuzzy: "tg" → snb_adapter_tg
cargo xtask build-plugin <name> --release  # extra cargo args pass through
cargo xtask build-all                      # workspace + every plugin
cargo run                                  # host loads libs from target/<profile> then ./plugins
```

The host loads `libsnb_*`/`snb_*` dynlibs from the executable dir first, then
`./plugins/`. After a debug build, `cargo run` picks them out of `target/debug/`.
**Rebuild the plugin and restart the host to load changes** (or, if the
`snb_plugin_manager` plugin is loaded, use its `/plugin` admin command to
reload at runtime).

### External plugin in its own repo (`snb.toml`)

A plugin can live in its own git repo with its own Cargo workspace (the
`snb_adapter_tg` pattern: an empty `[workspace]` in `Cargo.toml` detaches it from
the root). Such a plugin is dropped into `plugins/` and gitignored here.

For a plugin that doesn't want to maintain a `cdylib` `Cargo.toml` directly, an
`snb.toml` manifest lets `cargo xtask` generate one:

```toml
[build]
source = "src/lib.rs"     # OR  manifest = "path/to/Cargo.toml"
# package = "snb_my_plugin"   # optional crate/library name
# version = "0.1.0"

[dependencies]            # extra deps; snb_core/snb_macros/the plugin crate are injected
serde = { version = "1", features = ["derive"] }
```

`xtask` writes a detached manifest under `target/xtask-snb/<name>/` that mirrors
`[profile.release]` (including `panic = "unwind"`). Do not override the injected
`snb_core` / `snb_macros` / plugin dependencies in `[dependencies]`.

## ABI compatibility

A plugin is rejected at load if its ABI version is incompatible with the host
(major mismatch, minor newer, or — in `0.x` — any minor difference). The ABI
version is baked in from the `snb_core` you compiled against. **If you pull a
new `snb_core`, rebuild your plugin.** If a load fails with an ABI error, the
fix is almost always "rebuild the plugin against the current core." You do not
set the ABI version in your plugin — it comes from `snb_core`.

## Testing

Put tests in `tests/unit/<module>_tests.rs` and include them as inline modules:

```rust
#[cfg(test)]
#[path = "../tests/unit/lib_tests.rs"]
mod lib_tests;
```

Run with `cargo test -p <crate>` (reference plugins) or from the plugin's own
directory (external plugins). Test pure logic (parsing, conversion, command
partitioning) directly — they don't need a running bot.

## Common mistakes

- Forgetting `set_bot` first in a hand-written `on_load` → `bot()` panics.
- Forgetting `register_all` → components silently never register.
- Using `println!` for logs → use `log::*` (stdout is the stdin adapter's reply
  channel).
- A non-stoppable adapter (`stop` blocks or no stop flag) → leaked on unload.
- Reusing a command/hook/handler name already taken → whole plugin rolled back.
- Assuming the host's tokio runtime — adapters must use `run_async` (the macro
  does this) so the runtime is created inside the plugin's own cdylib.
