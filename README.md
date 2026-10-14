# loom-engine

English | [简体中文](README.zh-CN.md)

A game engine written from scratch in Rust (winit + wgpu + glam), designed to be driven and verified by AI agents. An agent can load a scene, press buttons, step the simulation tick by tick, read the full state as JSON and look at screenshots, all without a window, and the same inputs always produce the same result.

It aims to become a general-purpose 2D and 3D engine, but features are added only when a real game needs them. Two games currently drive it: a Souls-like combat prototype and a small 2D roguelike.

> **Status:** early and experimental. Formats and APIs change often, and the engine is developed and tested on Windows only.

## Design principles

- **Everything is text.** Scenes, config, data and input recordings are RON or JSON, so an agent can read, diff and edit them.
- **Headless.** Every feature runs with `--headless`, without creating a window.
- **Deterministic.** The simulation advances in fixed 60 Hz ticks, never reads the wall clock, seeds every random number and takes transcendental functions from `libm`. The same input produces byte-identical output.
- **Observable.** Every feature can be checked through a screenshot, a JSON state dump, a per-tick trace or a log.
- **Machine-readable output.** The `loom` CLI writes only JSON to stdout; human-readable logs go to stderr.

## Features

**Simulation and input**
- Fixed-step clock with named input actions and axes
- Input recording and replay as RON scripts
- JSON state dumps and per-tick JSONL traces of events and state

**Scenes**
- RON scene files: a node tree with transforms, meshes, sprites, tilemaps, cameras and per-game properties
- Hot reload in window mode; a scene that fails to parse keeps the previous one running

**Rendering**
- wgpu renderer that always draws to an offscreen target first, so window and headless output match
- 3D primitives and glTF models with skeletal animation: clips, blending and playback rate
- 2D sprites and tilemaps from Aseprite sprite sheets
- Pixel-font HUD and debug wireframes (hurtboxes, colliders, lock-on, ...)

**AI tooling**
- `loom serve`: a headless remote-control session over JSON-RPC on stdin and stdout
- `loom_mcp`: an MCP server on top of `loom serve`, with tools to load and edit scenes, set input, step, read state and take screenshots
- `loom schema`: JSON Schema for every file and output format

## The games

**`games/souls`**: a Souls-like combat prototype in 3D. A knight with a Dark Souls style orbit camera, stamina spent on rolls, backsteps, sprinting and attacks, light and heavy attacks with input buffering, lock-on, poise and stagger, healing flasks, bonfires and bloodstains, and a boss that picks its moves by distance and enters a faster second phase at half health.

**`games/rogue`**: a small turn-based 2D roguelike inspired by Cataclysm: Dark Days Ahead. Eight-way movement on a tile map with numpad or vi keys, zombies that chase the player, fixed damage, corpses and a message log.

## Quick start

Requirements:

- Rust (the toolchain is pinned in `rust-toolchain.toml` and installed by `rustup` automatically)
- [Git LFS](https://git-lfs.com/), installed before cloning, for the binary assets
- [PowerShell 7](https://github.com/PowerShell/PowerShell) (`pwsh`) for the scripts in `scripts/`

```powershell
git clone https://github.com/wd30x/loom-engine.git
cd loom-engine

# Play the boss fight in a window
cargo loom run --scene assets/scenes/boss.ron

# Play the roguelike in a window
cargo loom run --scene assets/scenes/rogue.ron

# Run 2 seconds headless, then save a screenshot and the full state
cargo loom run --headless --frames 120 --scene assets/scenes/boss.ron --screenshot out/boss.png --dump-state out/boss.json

# Formatting, lints and tests: run before every commit
pwsh scripts/verify.ps1
```

`cargo loom` is a cargo alias for the `loom` binary (see `.cargo/config.toml`). Runtime output such as screenshots, dumps and recordings goes to `out/`, which is ignored by git.

## Command line

| Command | What it does |
|---|---|
| `loom run` | Run a scene in a window or headless. Options for screenshots (`--screenshot`), state dumps (`--dump-state`), input replay (`--input`) and recording (`--record`), traces (`--trace`), debug wireframes (`--debug-draw`) and resolution (`--size`) |
| `loom view` | Render a scene once without simulating it, at one or more ticks, and save a screenshot |
| `loom serve` | Start a headless remote-control session: JSON-RPC requests on stdin, one reply per line on stdout |
| `loom schema` | Print the JSON Schema of scene files, props, state dumps, trace lines and events |
| `loom model` | Check a glTF binary model (`.glb`) against the import rules and describe it |
| `loom clip` | Convert one animation of a `.glb` model into a loom clip file (RON) |
| `loom info` | Print GPU adapter information without rendering |

Run `cargo loom <command> --help` for every option.

## Driving the engine from an AI agent

The repository ships an `.mcp.json`, so an MCP client that reads it (for example Claude Code opened in the repository) starts the `loom` MCP server through `scripts/mcp.ps1`. Its tools cover the whole loop:

- `load_scene`, `reset`, `scene`, `node`, `edit_node`, `add_node`, `remove_node`, `edit_scene`, `save_scene`: load, inspect and edit scenes
- `input`, `step`: hold actions and axes, then advance N ticks and get the events that happened
- `state`, `screenshot`, `record`, `trace`: read the state, see the frame, save an input script or a trace
- `describe`, `schema`: discover the action names and every format

Without MCP, `loom serve` speaks the same protocol directly:

```console
$ echo '{"jsonrpc":"2.0","id":1,"method":"describe"}' | loom serve
{"jsonrpc":"2.0","id":1,"result":{"tick_hz":60,"tick":0,"game":"souls","scene":null,"edited":false,"actions":["attack","drink","heavy","interact","lock","roll"],"axes":["look_x","look_y","move_x","move_y"]}}
```

## Workspace layout

| Crate | Role |
|---|---|
| `crates/loom_core` | Fixed-step clock, ticks, input state |
| `crates/loom_platform` | winit event loop and window; translates OS events into core input |
| `crates/loom_render` | wgpu rendering, always to an offscreen target first |
| `crates/loom_scene` | Scene file parsing and the node tree |
| `crates/loom_engine` | Facade crate and the `loom` CLI; picks the game named by the scene |
| `crates/loom_mcp` | MCP adapter; talks to the engine only through `loom serve` |
| `games/souls`, `games/rogue` | Game logic; may not depend on the renderer, the platform layer or each other |

Assets live in `assets/`. Each asset directory has a README explaining how its files are made; most of them are rebuilt from Aseprite Lua or Blender Python scripts.

## Documentation

The project documentation is written in Chinese.

- [CLAUDE.md](CLAUDE.md): project rules and workflow
- [docs/architecture.md](docs/architecture.md): architecture decisions and the specification of every format and system
- [docs/godot-reference.md](docs/godot-reference.md): how loom's concepts map to Godot's

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
