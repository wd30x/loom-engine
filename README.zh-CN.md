# loom-engine

[English](README.md) | 简体中文

用 Rust 从零手写的游戏引擎（winit + wgpu + glam），专门设计成让 AI 能直接操作和验证。AI 可以加载场景、按键、一个 tick 一个 tick 地推进模拟、以 JSON 读取完整状态、查看截图，全程不需要窗口；同样的输入永远得到同样的结果。

目标是支持 2D 和 3D 的通用引擎，但功能只在真实的游戏需要时才加。目前有两个游戏在驱动它：一个类魂战斗原型和一个小型 2D roguelike。

> **现状：** 早期、实验性。格式和接口经常变动，目前只在 Windows 上开发和测试。

## 设计原则

- **一切皆文本**：场景、配置、数据、输入录制都用 RON 或 JSON，AI 能直接读、比较和修改。
- **可无头运行**：所有功能都能在 `--headless` 下运行，不创建窗口。
- **确定性**：模拟以固定的 60 Hz tick 推进，不读墙钟时间，随机数都带种子，超越函数用 `libm`。同样的输入产生逐字节相同的输出。
- **可观测**：每个功能都能通过截图、JSON 状态导出、逐 tick 的追踪记录或日志检查。
- **输出给机器读**：`loom` 命令行的 stdout 只输出 JSON，给人看的日志走 stderr。

## 功能

**模拟和输入**
- 固定步长时钟，输入按动作和轴命名
- 输入录制和回放，脚本是 RON 文本
- JSON 状态导出，以及逐 tick 记录事件和状态的 JSONL 追踪

**场景**
- RON 场景文件：节点树，包含变换、网格、精灵、瓦片地图、相机和各游戏自己的属性
- 窗口模式下热重载；场景文件解析失败时保持旧场景继续运行

**渲染**
- wgpu 渲染，永远先画到离屏目标，所以窗口和无头模式的画面一致
- 3D 基本几何体和 glTF 模型，支持骨骼动画：片段、混合、播放速率
- 2D 精灵和瓦片地图，素材来自 Aseprite 导出的 sprite sheet
- 像素字体 HUD 和调试线框（受击框、碰撞体、锁定等）

**AI 工具**
- `loom serve`：无头的远程控制会话，通过 stdin/stdout 收发 JSON-RPC
- `loom_mcp`：基于 `loom serve` 的 MCP 服务器，提供加载和编辑场景、设置输入、推进、读状态、截图等工具
- `loom schema`：输出所有文件和输出格式的 JSON Schema

## 两个游戏

**`games/souls`**：3D 类魂战斗原型。一个骑士，黑魂式的环绕相机；翻滚、后撤步、冲刺和攻击消耗耐力；轻重攻击带预输入；锁定、韧性和硬直；元素瓶、篝火和血迹；还有一个按距离选招、血量降到一半进入更快的二阶段的 Boss。

**`games/rogue`**：小型回合制 2D roguelike，玩法借鉴《Cataclysm: Dark Days Ahead》。在瓦片地图上用小键盘或 vi 键八方向移动，僵尸会追玩家，伤害固定，死后留下尸体，有消息记录。

## 快速开始

需要：

- Rust（版本固定在 `rust-toolchain.toml`，`rustup` 会自动安装）
- [Git LFS](https://git-lfs.com/)，克隆前装好，用于二进制素材
- [PowerShell 7](https://github.com/PowerShell/PowerShell)（`pwsh`），用于 `scripts/` 下的脚本

```powershell
git clone https://github.com/wd30x/loom-engine.git
cd loom-engine

# 在窗口里玩 Boss 战
cargo loom run --scene assets/scenes/boss.ron

# 在窗口里玩 roguelike
cargo loom run --scene assets/scenes/rogue.ron

# 无头跑 2 秒，保存截图和完整状态
cargo loom run --headless --frames 120 --scene assets/scenes/boss.ron --screenshot out/boss.png --dump-state out/boss.json

# 格式、lint 和测试：每次提交前必跑
pwsh scripts/verify.ps1
```

`cargo loom` 是 `loom` 程序的 cargo 别名（见 `.cargo/config.toml`）。截图、导出、录制等运行时产出放在 `out/`，已被 git 忽略。

## 命令行

| 命令 | 作用 |
|---|---|
| `loom run` | 在窗口或无头模式下运行场景。可以截图（`--screenshot`）、导出状态（`--dump-state`）、回放输入（`--input`）、录制输入（`--record`）、写追踪（`--trace`）、画调试线框（`--debug-draw`）、设分辨率（`--size`） |
| `loom view` | 不模拟，在一个或多个 tick 上渲染场景并截图 |
| `loom serve` | 启动无头的远程控制会话：stdin 收 JSON-RPC 请求，stdout 每行一个回复 |
| `loom schema` | 输出场景文件、属性、状态导出、追踪行和事件的 JSON Schema |
| `loom model` | 按导入规则检查 glTF 二进制模型（`.glb`）并描述它 |
| `loom clip` | 把 `.glb` 模型里的一段动画转成 loom 的片段文件（RON） |
| `loom info` | 不渲染，只输出 GPU 适配器信息 |

每个命令的全部选项见 `cargo loom <命令> --help`。

## 让 AI 驱动引擎

仓库里带了 `.mcp.json`，读取它的 MCP 客户端（比如在仓库里打开的 Claude Code）会通过 `scripts/mcp.ps1` 启动 `loom` MCP 服务器。它的工具覆盖了完整的闭环：

- `load_scene`、`reset`、`scene`、`node`、`edit_node`、`add_node`、`remove_node`、`edit_scene`、`save_scene`：加载、查看和编辑场景
- `input`、`step`：按住动作、设置轴，再推进 N 个 tick，拿到这期间发生的事件
- `state`、`screenshot`、`record`、`trace`：读状态、看画面、保存输入脚本或追踪
- `describe`、`schema`：查询动作名和所有格式

不用 MCP 的话，`loom serve` 直接说同一套协议：

```console
$ echo '{"jsonrpc":"2.0","id":1,"method":"describe"}' | loom serve
{"jsonrpc":"2.0","id":1,"result":{"tick_hz":60,"tick":0,"game":"souls","scene":null,"edited":false,"actions":["attack","drink","heavy","interact","lock","roll"],"axes":["look_x","look_y","move_x","move_y"]}}
```

## 目录结构

| crate | 职责 |
|---|---|
| `crates/loom_core` | 固定步长时钟、tick、输入状态 |
| `crates/loom_platform` | winit 事件循环和窗口，把系统事件翻译成 core 的输入 |
| `crates/loom_render` | wgpu 渲染，永远先画到离屏目标 |
| `crates/loom_scene` | 场景文件解析和节点树 |
| `crates/loom_engine` | 门面 crate 和 `loom` 命令行，按场景指定的游戏组装 |
| `crates/loom_mcp` | MCP 适配程序，只通过 `loom serve` 和引擎通信 |
| `games/souls`、`games/rogue` | 游戏逻辑，不许依赖渲染、平台层，也不许互相依赖 |

素材在 `assets/`。每个素材目录都有一份 README，说明文件是怎么做出来的；大部分能用 Aseprite 的 Lua 脚本或 Blender 的 Python 脚本重新生成。

## 文档

- [CLAUDE.md](CLAUDE.md)：项目规则和工作流程
- [docs/architecture.md](docs/architecture.md)：架构决策，以及所有格式和系统的规格
- [docs/godot-reference.md](docs/godot-reference.md)：loom 的概念和 Godot 的对照

## 参与贡献

见 [CONTRIBUTING.md](CONTRIBUTING.md)（英文）。

## 许可证

可以任选以下一种许可证使用：

- Apache License 2.0（[LICENSE-APACHE](LICENSE-APACHE)）
- MIT 许可证（[LICENSE-MIT](LICENSE-MIT)）

除非你另有声明，你有意提交并被收录进本项目的贡献（按 Apache-2.0 的定义），都按上述双许可发布，不附加其他条款或条件。
