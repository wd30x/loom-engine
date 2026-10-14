# loom-engine

English | [简体中文](README.zh-CN.md)

A game engine written from scratch in Rust (winit + wgpu + glam), designed to be driven and verified by AI agents. It aims to become a general-purpose 2D and 3D engine, but features are added only when a real game needs them. The current users are a Souls-like combat prototype (`games/souls`) and a small 2D roguelike (`games/rogue`).

Design principles:

- **Everything is text**: scenes, config, data and input recordings are RON or JSON.
- **Headless**: every feature runs with `--headless`, without a window.
- **Deterministic**: the simulation advances in fixed-step ticks; the same input produces byte-identical output.
- **Observable**: features can be checked through screenshots, JSON state dumps or logs. The `loom` CLI writes only JSON to stdout, and `loom_mcp` exposes it as an MCP server.

## Documentation

The project documentation is written in Chinese.

- Project rules: [CLAUDE.md](CLAUDE.md)
- Architecture decisions: [docs/architecture.md](docs/architecture.md)
- Godot reference: [docs/godot-reference.md](docs/godot-reference.md)

## Quick start

```powershell
pwsh scripts/verify.ps1
cargo loom run --headless --frames 60 --screenshot out/shot.png
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
