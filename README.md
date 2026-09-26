# loom-engine

用 Rust 从零手写、专门为 AI 操作设计的游戏引擎，目标是支持 2D 和 3D 的通用引擎，功能按具体需求逐步加入。第一个使用者是类魂战斗原型，之后是一个小型 2D 示例游戏。

- 项目规则：[CLAUDE.md](CLAUDE.md)
- 架构决策：[docs/architecture.md](docs/architecture.md)
- 任务清单和进度：[docs/TODO.md](docs/TODO.md)
- Godot 参考：[docs/godot-reference.md](docs/godot-reference.md)

```powershell
pwsh scripts/verify.ps1
cargo loom run --headless --frames 60 --screenshot out/shot.png
```
