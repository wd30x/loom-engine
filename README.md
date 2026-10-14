# loom-engine

A game engine written from scratch in Rust (winit + wgpu + glam), designed to be driven and verified by AI agents. It aims to become a general-purpose 2D and 3D engine, but features are added only when a real game needs them. The current users are a Souls-like combat prototype (`games/souls`) and a small 2D roguelike (`games/rogue`).

Design principles:

- **Everything is text**: scenes, config, data and input recordings are RON or JSON.
- **Headless**: every feature runs with `--headless`, without a window.
- **Deterministic**: the simulation advances in fixed-step ticks; the same input produces byte-identical output.
- **Observable**: features can be checked through screenshots, JSON state dumps or logs. The `loom` CLI writes only JSON to stdout, and `loom_mcp` exposes it as an MCP server.

The project documentation is written in Chinese.

用 Rust 从零手写、专门为 AI 操作设计的游戏引擎，目标是支持 2D 和 3D 的通用引擎，功能按具体需求逐步加入。第一个使用者是类魂战斗原型，之后是一个小型 2D 示例游戏。

- 项目规则：[CLAUDE.md](CLAUDE.md)
- 架构决策：[docs/architecture.md](docs/architecture.md)
- Godot 参考：[docs/godot-reference.md](docs/godot-reference.md)

```powershell
pwsh scripts/verify.ps1
cargo loom run --headless --frames 60 --screenshot out/shot.png
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
