# loom-engine

用 Rust 从零手写的游戏引擎（winit + wgpu + glam，不用任何现成引擎），**专门设计成让 AI 能直接操作和验证**。目标是支持 2D 和 3D 的通用引擎（用户决定），但**功能由具体需求驱动**：有真实的使用者需要才加，不为假想的需求提前做。目前的使用者是类魂（黑魂3风格）战斗原型 `games/souls` 和 2D roguelike 示例 `games/rogue`。**引擎和游戏分开**：引擎 crate 里不许出现具体游戏的逻辑。外部工具（Aseprite、Blender 等）只通过导出的开放格式对接，见 `docs/architecture.md` 第 12 节。

## 工作流程

一个会话负责规划、实现、review 和提交（用户决定）。不写 PLAN、REVIEW 文件（用户决定，2026-10-16）。

- 每个切片依次：读 `docs/architecture.md` → 在对话里列出范围 → 用户看过后实现代码和测试 → 自己 review → 逐条处理 → 提交。一个切片一次提交。
- **开工前的范围说明**写在对话里，几行就够：做什么、「不在本切片范围」、验收标准（验收命令、哪些输出必须不变、要汇报的数据）。
- 不做范围外的事，「不在本切片范围」里列出的尤其不要碰。
- **进行中**：TODO 里这一项标 `[~]`，下面随时记进度和偏离范围的地方，中断后从那里接着做。
- review 时逐项对照范围说明检查 diff，不能只看测试是否通过。结论写进收尾汇报和提交说明。
- 收尾时向用户汇报：验收命令及其输出、要汇报的数据、偏离范围的地方、需要用户决定的问题。需要长期保留的结论写进 TODO 或架构文档。

进度以 `docs/TODO.md` 为准，下一个任务就是按阶段顺序的第一个 `[ ]` 或 `[~]`。

## AI 优先原则（硬性规则）

1. **一切皆文本**：场景、配置、数据、输入录制都用 RON/JSON 这类文本格式，不用二进制格式。
2. **可无头运行**：所有功能都必须能在 `--headless` 下运行，不创建窗口，也不依赖 winit。
3. **确定性**：模拟只在固定步长 tick 里推进。模拟代码不读墙钟时间，不依赖 `HashMap` 的迭代顺序，随机数必须带种子。同样的输入必须产生逐字节相同的输出（范围见 `docs/architecture.md` 第 4 节）。
4. **可观测**：新功能要能通过截图、JSON 状态导出或日志被检查到。做不到的功能，视为没做完。
5. **stdout 给机器，stderr 给人**：`loom` 的 stdout 只输出 JSON，日志一律走 `log` 宏写到 stderr。
6. **自己验证**：改了画面相关的代码，就用无头模式截图，**打开 PNG 亲眼看一下**再说完成。

## 常用命令

```powershell
pwsh scripts/verify.ps1                                   # 提交前必跑：fmt + clippy(-D warnings) + test
pwsh scripts/smoke.ps1                                    # review 时跑：GPU 和窗口路径，需要桌面会话（-HeadlessOnly 只跑无头部分）
cargo loom run                                            # 窗口模式（cargo alias，见 .cargo/config.toml）
cargo loom run --headless --frames 60 --screenshot out/shot.png
$env:RUST_LOG = "debug"                                   # 打开详细日志
```

运行时产出（截图、导出、录制）一律放在 `out/`（已 gitignore）。

## 结构

| crate | 职责 | 不许依赖 |
|---|---|---|
| `loom_core` | 固定步长时钟、tick、输入状态 | winit、wgpu |
| `loom_platform` | winit 事件循环和窗口，把 OS 事件翻译成 core 的输入 | wgpu、loom_render |
| `loom_render` | wgpu 渲染，永远先画到离屏目标 | winit、loom_platform |
| `loom_engine` | 门面 crate 和 `loom` 命令行程序，负责把上面几个组装起来，按场景的 `game` 选游戏 | — |
| `loom_scene` | 场景文件解析和节点树 | winit、wgpu |
| `loom_mcp` | MCP 适配程序，只通过 `loom serve` 的 stdio 和引擎说话 | 任何引擎 crate |
| `games/souls`、`games/rogue` | 游戏逻辑（类魂原型、2D roguelike），各有 `clippy.toml` 禁 std 超越函数 | 引擎的 render、platform、engine，以及彼此 |

详见 `docs/architecture.md`。

## 代码约定

- 库 crate 的错误用 `thiserror` 定义枚举，只有 `loom_engine` 用 `anyhow`。
- 引擎代码里不用 `unwrap()`。确实不可能失败的地方用 `expect("为什么不会失败")`。
- 往 stdout 输出 JSON 的那一个函数，用 `#[allow(clippy::print_stdout)]` 局部放行，别的地方不许放行。
- 不写 `unsafe`（workspace 层面是 deny）。确实需要时先在范围说明里写明理由，由用户决定。
- **语言约定**：代码注释和标识符用英文；文档用中文；**提交说明一律用英文**（标题和正文都是）。
- 坐标系、单位、颜色空间等约定见 `docs/architecture.md`，不要自己另定。

## 测试约定

- 单元测试和被测代码放在一起（`#[cfg(test)] mod tests`）；`tests/` 目录只放跨 crate 的集成测试。
- **测试名用完整句子描述保证的行为**，例如 `long_frame_is_capped_and_excess_dropped`。
- 测试名不要互相包含（`cargo test` 按子串过滤，`tick` 会选中 `tick_rate`）。
- 和时序有关的测试要**连跑 10 次都通过**才算数，不许靠调大超时掩盖不稳定。
- 每条行为要说明由什么判定对错：解析解（如「1 秒转 1 弧度」）、确定性自洽（跑两次结果相同）、状态导出、截图目测。都覆盖不到的，写进 TODO 末尾的「测试覆盖的已知薄弱处」。

## 参考源码

`../godot/`（和本仓库同级）是 Godot 4.7.2-stable 的只读浅克隆，在仓库之外，概念对照表见 `docs/godot-reference.md`。

- 借鉴设计思路随便用。
- 移植具体代码时，要在 `THIRD_PARTY_NOTICES.md` 里保留 Godot 的 MIT 版权声明。
- 不要从 `../godot/thirdparty/` 里拷代码，那里的许可证各不相同。

## Git

- 自己 review 并逐条处理完后提交，一个切片一次提交，每次提交前 `verify.ps1` 必须通过。
- 提交信息格式：`<crate或范围>: <做了什么>`，例如 `loom_render: add offscreen target and readback`。
- 大的二进制资源（`assets/` 下的图片、模型、音频）走 Git LFS，见 `.gitattributes`。
