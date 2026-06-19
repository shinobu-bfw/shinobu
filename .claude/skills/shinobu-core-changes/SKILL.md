---
name: shinobu-core-changes
description: Use when modifying snb_core, snb_macros, or snb_runtime — the plugin-facing ABI surface of the Shinobu framework. Covers what counts as an ABI break, when and how to bump abi_version, the cdylib/static-linking model, FFI safety invariants, the unload drain protocol, adding a new component type, and keeping the macros + register_all + the loader in sync so already-built plugins stay memory-safe.
---

# Changing Shinobu core

The host and every plugin each compile their **own** copy of `snb_core` and meet
only across a C-ABI + `dyn`-trait boundary. A change that shifts a shared layout
is **undefined behavior at load time, not a compile error**. This skill is about
making such changes safely.

**The one rule:** if your change alters anything a plugin's compiled-in
`snb_core` assumes about a host-provided type or vtable, it is an **ABI break** —
bump `abi_version` and rebuild all plugins. When unsure, treat it as a break.

## Step 1 — Decide: is this an ABI break?

It IS an ABI break (bump required) if you change any of:

- **Trait method sets/signatures/order** on anything crossing the boundary:
  `SnbPlugin`, `BotContext`, `Adapter`, `CommandHandler`, `Hook`,
  `MessageHandler`, `DatabaseDriver`, `Logger`, `SessionManager`. These are
  called through vtables built on one side and invoked on the other; adding,
  removing, reordering, or re-signing a method shifts the vtable. *Adding a
  method with a default still shifts the vtable* — it's a break.
- **Layout of types passed across the boundary:** `Event`, `EventType`,
  `Message`, `Command`, `Sender`, `Chat`, `ChatType`, `ContentItem`,
  `ImageSource`/`FileSource`, `TextFormat`, `CommandSpec`, `CommandVisibility`,
  `PluginInfo`, `PluginType`, `Version`, `PluginCell`, `BotInfo`, `BotStatus`
  and friends. Adding/removing/reordering a field or enum variant is a break.
- **The FFI exports** (`create_plugin` / `destroy_plugin` / `plugin_abi`) — their
  signatures or the `*mut Box<dyn SnbPlugin>` contract.

It is NOT an ABI break (no bump) if it is purely internal and invisible to a
plugin's `snb_core`:

- Anything inside `snb_runtime` that isn't part of a boundary type/trait (the
  dispatch loop, `Bot`'s private fields, the loader internals, logging format).
- New free functions / methods *on concrete runtime types* that plugins don't
  call through a `dyn` boundary.
- Doc comments, tests, internal refactors with identical public layout.

> Pre-1.0 caution: in `0.x` the loader treats **any** minor difference as
> incompatible (an older-minor plugin is rejected, not just warned). So a bump
> is a hard wall — every plugin must be rebuilt. Don't bump frivolously, but
> never skip a bump on a real break: a missed bump is silent UB.

## Step 2 — Bump the ABI version

Edit the root [`Cargo.toml`](../../../Cargo.toml):

```toml
[workspace.metadata.snb]
abi_version = "0.x.y"   # read the real value from the file; while pre-1.0, bump the MINOR on a breaking change (e.g. 0.5.0 → 0.6.0)
```

How it propagates: [`crates/snb_core/build.rs`](../../../crates/snb_core/build.rs)
reads `[workspace.metadata.snb].abi_version` and emits
`cargo:rustc-env=SNB_ABI_VERSION`; `snb_plugin_abi()` parses it; `#[plugin]`'s
generated `plugin_abi` export returns it; the loader compares plugin vs host in
[`Bot::load_plugin`](../../../crates/snb_runtime/src/bot.rs). The build script
already `rerun-if-changed`s the workspace `Cargo.toml`, so a bump takes effect on
the next build. You do **not** touch `build.rs` or `snb_plugin_abi()`.

Semantics enforced by the loader (keep this contract in mind if you ever edit it):
major mismatch → reject; minor newer → reject; minor older while major 0 →
reject; minor older while major ≥ 1 → warn; patch differs → warn.

## Step 3 — Keep the macro layer in sync

`snb_macros` generates the code plugins rely on. If your core change touches a
boundary, the matching macro usually must change too:

- New/changed **trait method** → update the generator in
  [`crates/snb_macros/src/`](../../../crates/snb_macros/src/) (`command.rs`,
  `hook.rs`, `handler.rs`, `adapter.rs`, `database.rs`, `plugin.rs`) so the
  generated impl still satisfies the trait. The macros reference core via
  `::snb_core::...` paths — keep those paths valid.
- Changed **FFI export** signature → update `plugin.rs` (which emits
  `create_plugin`/`destroy_plugin`/`plugin_abi`) **and** the symbol resolution +
  call in `Bot::load_plugin`. These two must agree exactly.
- New attribute argument (e.g. another command field like `visibility` was) →
  parse it in the macro and thread it into the generated impl, mirroring the
  existing `MetaArgs`-based handling.

## Step 4 — Adding a new component type (worked path)

If you add, say, a "scheduler" component, all of these must change together —
miss one and registration or unload silently breaks:

1. **Trait** in `snb_core` (e.g. `scheduler.rs`), object-safe (`Send + Sync`).
2. **Registry** in [`registry.rs`](../../../crates/snb_core/src/registry.rs): a
   `SchedulerRegistration { factory: fn() -> Arc<dyn Scheduler> }` +
   `inventory::collect!`.
3. **`register_all`** in [`context.rs`](../../../crates/snb_core/src/context.rs):
   iterate the new collection and call a new `BotContext::register_scheduler`.
4. **`BotContext`** trait: add `register_scheduler` (this is itself an ABI break).
5. **`Bot`** in `snb_runtime`: store it, implement `register_scheduler`, and —
   critically — **drop it in `remove_plugin_components`** and add it to the
   `DrainArc` enum so unload waits out its `Arc` like every other component.
6. **Macro** in `snb_macros` (`#[scheduler]`) emitting the impl + `submit!`.
7. Bump `abi_version`, rebuild all, add tests.

Use the existing five component types as a template — grep for `Database` across
`registry.rs`, `context.rs`, and `bot.rs` to see every site one type touches.

## FFI & memory-safety invariants (do not regress)

The runtime goes to real lengths to stay memory-safe across `dlclose`. If you
edit [`bot.rs`](../../../crates/snb_runtime/src/bot.rs) or
[`plugin.rs`](../../../crates/snb_core/src/plugin.rs), preserve these:

- **`panic = "unwind"` must stay** in `[profile.release]` and in the generated
  `snb.toml` manifest (xtask). `run_async` and `on_event` rely on `catch_unwind`
  to contain a plugin panic; a panic crossing the cdylib→host frame would abort
  the whole process. Keep panics caught *inside* the plugin's own dylib.
- **`PluginCell` keeps the `Library` alive** (`_keep_alive`) and drops in order:
  `destroy_plugin` runs before the `Library` unmaps. Don't reorder its `Drop`.
- **The unload drain protocol** in `unregister_plugin`: remove registry entries
  → `Adapter::stop()` → spin/sleep until `Arc::get_mut` succeeds on the cell and
  every `DrainArc` (≤ `UNLOAD_DRAIN_TIMEOUT`) → then `on_unload` + drop
  (`dlclose`); otherwise `std::mem::forget` the cell and **leak** rather than
  unmap code a live thread runs. Any new per-plugin `Arc` you introduce must be
  added to the drain set or it becomes a use-after-free window. `stop()` must
  never block.
- **Locks are dropped before re-entrant calls.** Dispatch snapshots handler/cell
  `Arc`s and releases the registry lock before invoking them, because a handler
  may re-enter the bot (emit, register, unregister). Preserve "snapshot, drop
  lock, then call".
- **`register_all` must stay in `snb_core`.** It's statically linked per plugin
  so `inventory::iter` reads that plugin's submissions. Moving it behind a `dyn
  BotContext` call would iterate the host's empty registry. Don't relocate it.
- **Path confinement.** `safe_path_under` rejects `..`/non-normal components and
  verifies the canonicalized path stays under the plugin root. Keep
  `load_config`/`write_config`/`data_dir` going through it; writes stay atomic
  (tmp + rename).

## Step 5 — Verify

```sh
cargo build                 # host + default members
cargo test --workspace      # crates + the 3 reference plugins
cargo xtask build-all       # rebuild EVERY plugin against the new core
cargo run                   # confirm plugins load without ABI errors
```

After an ABI bump, a plugin built against the old core **must** be rejected at
load with an ABI message — that's the guardrail working, not a bug. Rebuild it.
Remember the external plugins (`snb_adapter_tg`, `snb_plugin_manager`,
`snb_plugin_payload_extract`) live in their own repos/workspaces — they need a
separate rebuild from their own directories; `cargo xtask build-all` only
rebuilds what's present in `plugins/`.

## Pitfalls

- Bumping `abi_version` but forgetting to rebuild plugins → confusing load
  rejections (working as designed; rebuild).
- Changing a boundary type but **not** bumping `abi_version` → silent UB; the
  worst failure mode. When in doubt, bump.
- Editing a `BotContext`/`SnbPlugin` method without updating the macro that
  generates the impl → reference plugins fail to compile (catch it with
  `cargo test --workspace`).
- Adding a component `Arc` to `Bot` without adding it to `remove_plugin_components`
  / `DrainArc` → unload can `dlclose` while it's live.
- Routing diagnostics through `println!` in the runtime → collides with the
  stdin adapter's stdout reply channel; use the logger.
