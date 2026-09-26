# loom-engine

用 Rust 从零手写的游戏引擎（winit + wgpu + glam，不用任何现成引擎），**专门设计成让 AI 能直接操作和验证**。引擎跟着一个类魂（黑魂3风格）战斗原型一起长：游戏需要什么才加什么，不做通用引擎。

## 角色分工

- **架构师**：写 `docs/architecture.md`、`docs/roadmap.md` 和 `docs/tasks/` 下的任务文档，审查实现，**负责提交**。
- **实现者**：按任务文档实现代码。
  - 开始前完整读一遍对应任务文档、`docs/architecture.md` 和 `FEEDBACK.md`（里面可能有待处理的审查意见）。
  - 规格有歧义或有错时，**不要自己重新设计**。选最保守的做法继续，并把问题写进 `FEEDBACK.md`。
  - 不要改 `docs/architecture.md` 里的决策。有异议写进 `FEEDBACK.md`，由架构师决定。
  - 不要做任务范围外的功能，「不做」列表里的内容尤其不要碰。
  - 验收结果和实现说明写在任务文档末尾的「实现记录」里。
  - **不要提交**。做完后保持改动未提交，由架构师审查后提交。

`FEEDBACK.md` 是双方的沟通渠道：实现者写问题和建议，架构师在每条下面回复并改状态；架构师的审查意见也写在这里，编号带 `R`（如 `E0-R1`）。

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
| `loom_engine` | 门面 crate 和 `loom` 命令行程序，负责把上面几个组装起来 | — |

详见 `docs/architecture.md`。

## 代码约定

- 库 crate 的错误用 `thiserror` 定义枚举，只有 `loom_engine` 用 `anyhow`。
- 引擎代码里不用 `unwrap()`。确实不可能失败的地方用 `expect("为什么不会失败")`。
- 往 stdout 输出 JSON 的那一个函数，用 `#[allow(clippy::print_stdout)]` 局部放行，别的地方不许放行。
- 不写 `unsafe`（workspace 层面是 deny）。确实需要时先在「实现记录」里说明理由。
- 注释和标识符用英文，文档用中文。
- 坐标系、单位、颜色空间等约定见 `docs/architecture.md`，不要自己另定。

## 参考源码

`ref/godot/` 是 Godot 4.7.2-stable 的只读浅克隆（已 gitignore），概念对照表见 `docs/godot-reference.md`。

- 借鉴设计思路随便用。
- 移植具体代码时，要在 `THIRD_PARTY_NOTICES.md` 里保留 Godot 的 MIT 版权声明。
- 不要从 `ref/godot/thirdparty/` 里拷代码，那里的许可证各不相同。

## Git

- 架构师审查通过后提交，一个任务一个或几个提交，每次提交前 `verify.ps1` 必须通过。
- 提交信息格式：`<crate或范围>: <做了什么>`，例如 `loom_render: add offscreen target and readback`。
- 大的二进制资源（`assets/` 下的图片、模型、音频）走 Git LFS，见 `.gitattributes`。
