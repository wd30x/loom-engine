# loom-engine 架构

本文档记录**已经定下来的决策**。实现者不要改这里的内容，有异议写进 `docs/FEEDBACK.md`，由规划者决定后更新这里。

## 1. crate 分层

```
                  loom_engine  (bin: loom)
           /          |            |         \
loom_platform    loom_render   loom_scene     (彼此不知道对方)      游戏：games/souls、games/rogue
       |                                             （只依赖 loom_core、loom_scene）
   loom_core
```

- `loom_scene`：场景文件的解析和校验，以及运行时的节点树（第 10 节）。只依赖 glam、serde、ron、thiserror，不知道渲染和平台。
- **游戏 crate 放在 `games/` 目录**（E4.3 起），和引擎 crate（`crates/`）从目录上就分开：
  - `souls`：类魂原型的角色、相机和碰撞（第 11 节）。G1.0 到 E4.2 叫 `loom_sim`，放在 `crates/loom_sim`，E4.3 改名搬家。依赖 `loom_core`、`loom_scene`、glam、libm。
  - `rogue`：2D roguelike 示例（第 15 节）。依赖 `loom_core`、`loom_scene`、glam。
  - 每个游戏 crate 都有**自己的 `clippy.toml`**，用 `disallowed-methods` 禁掉 std 的超越函数（第 4 节）。注意 crate 级的 `clippy.toml` 会整个替换 workspace 的那份，所以要把 workspace 里的设置也抄过来。
  - 游戏 crate 之间互不依赖，也不依赖 `loom_engine`、`loom_render`、`loom_platform`。

- `loom_core`：纯逻辑。固定步长时钟、tick 计数、输入（按键、动作映射、采样、脚本、录制，见第 9 节）。不依赖 winit 和 wgpu，所以可以在任何环境下做单元测试。
- `loom_platform`：只有它能碰 winit。它把 OS 事件翻译成 `loom_core` 的输入类型，并把窗口句柄交给调用方。
- `loom_render`：只有它能碰 wgpu。**它不知道 winit 的存在**：创建 surface 时接收的是任何实现了 `Into<wgpu::SurfaceTarget<'static>>` 的东西（比如 `Arc<winit::window::Window>`），由 `loom_engine` 传进来。
- `loom_engine`：组装层。解析命令行，按窗口模式或无头模式把上面三个 crate 接起来。
- `loom_mcp`（E3.2 起）：MCP 适配程序 `loom-mcp`，只通过 `loom serve` 的 stdio 协议和引擎说话，**不依赖任何引擎 crate**（第 13 节）。tokio 和 rmcp 只在这里。

以后新增的 crate（场景、远程控制等）也照这个思路来：**各自只依赖自己必须用到的东西**，由 `loom_engine` 统一组装。

**引擎和游戏分开**（2026-09-27，用户决定：做支持 2D 和 3D 的通用引擎，但功能仍由具体需求驱动）：
- 引擎 crate（`loom_core`、`loom_platform`、`loom_render`、`loom_scene`，以及以后的远程控制等）里**不许出现具体游戏的概念**。例如 `loom_render` 只认网格、材质、相机、光，不认「角色」；`loom_scene` 的组件要是通用的，不能是某个游戏专用的。
- 游戏逻辑放在游戏自己的 crate 里（`games/souls`、`games/rogue`）。
- **选择游戏**（E4.3 起，用户决定 2026-09-27）：场景文件顶层写 `game: "souls"` 或 `game: "rogue"`（第 10 节），`loom run`、`loom serve` 和 serve 的 `load_scene` 都按它选游戏。AI 只要给一个场景路径就行，不用另外说是哪个游戏。`loom` 程序把所有游戏都编译进去。
- **怎么接进引擎**（E4.3 规划者决定）：`loom_engine` 里用一个枚举 `AnyGame { Souls(souls::Game), Rogue(rogue::Game) }` 按变体分派，**不定 trait**。只有两个游戏，而且 `loom_engine` 本来就是唯一同时认识所有 crate 的组装层，枚举最简单，加游戏时编译器会指出每个要补的 `match`。等出现第三个游戏，或者游戏要从引擎外部加载时，再考虑 trait。每个游戏 crate 约定提供同样形状的接口：
  - `Game::new(Scene) -> Result<Game, GameError>`：检查场景是否满足这个游戏的要求；
  - `tick(&mut self, &ActionState)`、`clock() -> SimClock`、`scene() -> &Scene`；
  - `state() -> GameState`：状态导出里 `game` 字段的内容，可以序列化；
  - `bindings() -> ActionMap`：这个游戏的键位表，嵌在 crate 里。
- 判断一个功能该不该进引擎：至少要有一个真实的使用者需要它；放进引擎的那一部分，另一个游戏也用得上。

## 2. 两种运行模式

| | 窗口模式 | 无头模式 `--headless` |
|---|---|---|
| winit | 用 | **完全不创建** event loop 和窗口 |
| wgpu surface | 有 | 没有（`compatible_surface: None`） |
| 帧推进 | 墙钟时间驱动累加器 | 每帧恰好推进 1 个 tick，不读墙钟 |
| 输出 | 画面显示到窗口 | 画面读回 CPU，可存成 PNG |
| GPU | 总是创建 | **只在需要输出画面时创建**（目前是指定了 `--screenshot`），且只渲染最后一帧 |

两种模式**共用同一套模拟代码和绘制代码**，只有帧推进方式和最后一步（呈现还是读回）不一样。

**无头模式为什么只渲染需要的帧**：无头模式下中间帧谁也看不到；渲染器又是模拟状态的纯函数（每帧清屏、没有跨帧的渲染状态），所以只渲染最后一帧，得到的图和逐帧渲染逐字节相同。这样做还有一个更重要的好处：不截图的无头运行完全不碰 GPU，没有 GPU 的机器（比如 CI）也能跑模拟和集成测试。

窗口模式下的补充语义（E0-3）：
- `--size` 只决定窗口创建时的 inner size（物理像素）。之后离屏目标跟着窗口尺寸变，所以画面不会被拉伸，截图也是当前窗口的尺寸。
- 最小化时（尺寸为 0）跳过渲染，但 tick 仍然按墙钟推进。
- 报告里的 `frames` 只统计实际渲染了的帧，`size` 是最后一个实际渲染的帧的尺寸。

## 3. 渲染：永远先画到离屏目标

每一帧都画到渲染器自己拥有的离屏颜色目标上（`Rgba8UnormSrgb`，并带深度缓冲），然后：
- 窗口模式：用一个全屏三角形的 blit pass 把离屏目标画到 surface 上。surface 格式可能是 `Bgra8UnormSrgb` 之类，blit 负责处理格式差异。
- 无头模式或需要截图时：把离屏目标拷到 buffer，再映射读回。

**为什么**：截图和无头渲染走的是和平时显示完全相同的代码路径，窗口模式下也能随时截图。代价是每帧多一次全屏 blit，可以忽略。

读回时的已知坑：`copy_texture_to_buffer` 要求每行字节数按 `wgpu::COPY_BYTES_PER_ROW_ALIGNMENT`（256）对齐，读回后要去掉每行末尾的填充。

**图形后端**（G1.5 起，用户决定）：
- Windows 上只用 **Dx12**，窗口模式、无头模式和 `loom info` 都一样。其他平台用 wgpu 的默认后端。
- 环境变量 `WGPU_BACKEND` 可以覆盖默认后端（例如 `WGPU_BACKEND=vulkan`），用于排查问题。报告里的 `adapter.backend` 是实际用上的后端。
- 为什么不用 Vulkan：在本机（NVIDIA，远程桌面会话），Vulkan 下渲染器的创建和销毁偶尔会卡住约 2 分钟，或者以退出码 2173 崩溃。Dx12 在同样的测量里没出过问题，初始化也更快。测量数据见 TODO 的 G1.4。
- 同一块 GPU 上，Dx12 和 Vulkan 的截图逐字节相同，所以换后端不改截图基准。

## 4. 时间与确定性

参考 Godot 的 `MainTimerSync` 和 `--fixed-fps`（见 `docs/godot-reference.md`）。

- 模拟频率固定为 **60 Hz**，`tick_dt = 1/60 s`。
- 模拟时间**由 tick 计数算出**：`sim_time = tick as f64 / 60.0`。不要每帧把浮点 dt 累加起来，那样会累积误差，结果也不可复现。
- 窗口模式用累加器：墙钟 dt 加进累加器，每攒够一个 `tick_dt` 就跑一个 tick。每帧最多跑 `MAX_TICKS_PER_FRAME = 8` 个，超出的时间直接丢弃，防止卡顿后越追越慢（spiral of death）。
- 无头模式每帧恰好跑 1 个 tick，所以第 N 帧的状态完全确定。
- 渲染在 E0 阶段直接用最新的 tick 状态，不做插值。以后需要时再加，会另写决策。

模拟代码里的禁止事项：读墙钟时间（`Instant::now()` 等）、依赖 `HashMap`/`HashSet` 的迭代顺序、使用不带种子的随机数。

**模拟用的数学**：
- 计算模拟状态时，**不许用 std 的超越函数**（`sin`、`cos`、`tan`、`sin_cos`、`atan2`、`exp`、`ln`、`powf` 等）。std 把它们交给平台的数学库，不同操作系统和 C 运行时的结果可能差最后一位。改用 `libm` crate（纯 Rust 实现，在哪儿算结果都一样）。用 glam 的旋转之类的函数时，要打开 glam 的 `libm` 特性。
- `+ - * /`、`sqrt`、`%`（取余）是 IEEE 754 规定精确舍入的，可以随便用。
- 不许打开 `fast-math` 之类会改变浮点语义的选项。
- 渲染端不受这条限制，因为图像本来就只保证本机一致。
- **glam 从 E2 起打开 `libm` 特性**（workspace 依赖里）：场景的旋转是模拟状态，四元数要用 sin/cos。已查过 glam 0.33 源码（`src/f32/math.rs`）：打开 `libm` 后即使同时开着 `std`，也走 libm。这对渲染端同样生效，所以截图的哈希会随之变化一次。
- 执行方式：等出现专门的模拟 crate（G1）时，给它加一个 crate 级别的 `clippy.toml`，用 `disallowed-methods` 禁掉这些函数（见 TODO G1.0）。

**确定性的范围**（E0-4）：
- **图像**：只保证同一台机器、同一驱动下逐字节相同。实测同一块 GPU 上 Vulkan 和 Dx12 的截图也逐字节相同，但换 GPU 或换驱动不作保证。PNG 只用于本机回归和人（AI）眼检查。
- **模拟状态**：必须在任何机器上都逐位相同。跨机器的回归测试一律比较模拟状态的导出（E1 起），不比较图片。

## 5. 约定

| 项目 | 约定 |
|---|---|
| 坐标系 | 右手系，**+Y 向上**，**-Z 向前**，+X 向右（与 glTF 一致） |
| 单位 | 长度用米，角度用弧度，时间用秒。**例外**：场景文件里给人和 AI 手写的角度（欧拉角、视场角）用度 |
| 欧拉角 | 场景文件的 `rotation: (x, y, z)`（度）按 **YXZ** 顺序组合，即 `Quat::from_euler(EulerRot::YXZ, y, x, z)`，和 Godot 默认一致：先偏航，再俯仰，再翻滚 |
| 投影 | `glam::camera::rh::proj::directx::perspective` + `glam::camera::rh::view::look_at_mat4`（右手系、深度 0..1，与 wgpu 一致；即已弃用的 `Mat4::perspective_rh` / `look_at_rh`） |
| 颜色 | 离屏目标是 sRGB 格式，shader 里按线性空间计算；PNG 保存的就是 sRGB 编码后的字节 |
| 数学库 | 只用 `glam`，不引入别的数学库 |
| 换行 | 引擎输出的所有文本文件（状态导出、录制等）一律用 `\n`，不随平台变。注意 ron 的 `PrettyConfig` 在 Windows 上默认是 `\r\n`，必须显式设 `new_line("\n")` |

## 6. 命令行输出协议

`loom` 是给 AI 和脚本调用的，所以输出格式是固定的：

- **stdout**：每次运行**恰好输出一个 JSON 对象**，最后带一个换行。成功时 `"status": "ok"`，失败时是 `"status": "error"` 加 `"error"` 字段。
- **stderr**：给人看的日志（`env_logger`，用 `RUST_LOG` 控制级别）。
- **退出码**：`0` 成功，`1` 运行时出错，`2` 命令行参数错误。
- 参数错误时（E0-2）：clap 的报错照常写 stderr，同时 stdout 输出 `{"status":"error","error":"<clap 报错文本>"}`，退出码 2。
- **唯一的例外**是 `--help` 和 `--version`：输出给人看的纯文本，退出码 0，不输出 JSON。
- `loom serve`（E3 起）是长时间运行的会话，stdout 是一行一条的 JSON-RPC 回复，不是单个 JSON 对象，见第 13 节。启动失败时仍按这里的规则输出一个错误 JSON。

**格式稳定性**（用户决定，2026-09-26）：在出现**第一个外部使用者**之前（预计是 E3 的远程控制 / MCP），stdout、状态导出、输入脚本、场景文件的格式**可以改**，但每次都必须：写进本文档、在同一个切片里同步改测试。出现外部使用者之后，改回严格规则：字段只能加，不能删、不能改名、不能改含义。
- 2026-09-27 用户决定：E3 的 MCP（`loom-mcp`）**不算**外部使用者。它的使用者是 AI 会话，工具说明是每次连接时现读的；录制、状态基准都在仓库里，和格式同一个切片更新。所以**继续用宽松规则**（serve 协议、MCP 工具的参数和结果也一样）。等仓库外面真有东西依赖这些格式时再收紧，比如 aseprite-rs 那边写了调用 loom 的脚本。

### 命令

```
loom run [OPTIONS]
    --headless             不创建窗口；每帧恰好 1 个 tick
    --frames <N>           跑 N 帧（N ≥ 1）后退出。无头模式必须提供
    --screenshot <PATH>    退出前把最后一帧存成 PNG，会自动创建父目录
    --dump-state <PATH>    退出前把模拟状态写成 JSON（见下文「状态导出」），会自动创建父目录
    --input <PATH>         回放输入脚本（第 9 节）；这时键盘不再驱动动作，Esc 仍然可以退出
    --record <PATH>        退出前把这次运行每个 tick 的动作变化写成输入脚本（第 9 节），会自动创建父目录
    --scene <PATH>         加载场景文件（第 10 节），按它顶层的 game 选游戏；不给时用 souls 内置的演示场景。窗口模式下文件变化会热重载
    --no-capture           窗口模式下不抓鼠标（不锁定、不隐藏指针），也不读鼠标移动（轴只来自按键）。
                           smoke 测试用它：不占用桌面鼠标，有人动鼠标也不会污染录制。和 --headless 同时给是参数错误
    --size <WxH>           渲染分辨率，默认 1280x720；窗口模式下只决定初始窗口尺寸（第 2 节）

loom info                  输出 GPU adapter 信息，不渲染

loom view [OPTIONS]        只加载场景、画一帧，不跑任何游戏逻辑（E4.2 起，第 14 节）。只有无头
    --scene <PATH>         必填（E4.3 起）。内置演示场景的相机要靠游戏逻辑摆位，不跑游戏画出来没有意义
    --screenshot <PATH>    必填
    --size <WxH>           默认 1280x720

loom serve [OPTIONS]       远程控制会话（第 13 节），只有无头
    --scene <PATH>         同 run；不给时用内置演示场景
    --size <WxH>           截图的默认尺寸，默认 1280x720
```

无头模式下，第 k 帧（从 1 开始数）跑第 k 个 tick；截图和状态导出取的都是第 N 个 tick 跑完之后的状态，所以 `--frames 60` 时 `sim_time = 1.0`。

### 输出格式

`loom run` 成功：
```json
{"status":"ok","mode":"headless","frames":60,"ticks":60,"sim_time":1.0,"size":[1280,720],"screenshot":"out/e0.png","state":null,"record":null,"input":null,"adapter":{"name":"...","backend":"Dx12","driver":"..."}}
```
- `mode`：`"headless"` 或 `"window"`
- `screenshot`：没有截图时为 `null`
- `state`：没有导出状态时为 `null`，否则是 `--dump-state` 给的路径
- `record`：没有录制时为 `null`，否则是 `--record` 给的路径
- `input`：没有回放时为 `null`，否则是 `{"path":…,"events":脚本里的事件数,"applied":实际生效的事件数}`。脚本比运行长时 `applied < events`
- `adapter`：没有创建 GPU 设备时为 `null`（无头模式且没有 `--screenshot`）
- `renderer_init_ms`（G1.4 起）：从进程启动到渲染器创建完成（设备、管线都建好，窗口模式还包括 surface）的毫秒数，整数；没有创建渲染器时为 `null`。用来区分「启动慢」和「运行中出错」。它是墙钟时间，只用于诊断，不进状态导出
- `scene`：用内置演示场景时为 `null`，否则是 `--scene` 给的路径
- `game`（E4.3 起，紧跟在 `scene` 后面）：跑的是哪个游戏，`"souls"` 或 `"rogue"`
- `reloads`：窗口模式下成功热重载的次数；`reload_errors`：热重载时场景文件解析或校验失败的次数（失败时保持旧场景继续跑）。无头模式两者都是 0
- 窗口模式提前退出（Esc、关窗）时，`frames` 和 `ticks` 是实际跑到的数

`loom info` 成功：
```json
{"status":"ok","adapter":{"name":"...","backend":"Dx12","driver":"...","device_type":"DiscreteGpu"}}
```

失败：
```json
{"status":"error","error":"人能读懂的错误描述"}
```

### 状态导出

`--dump-state <PATH>` 在退出前把模拟状态写成 JSON 文件。它是**跨机器回归的依据**（第 4 节）：只要输入相同，这个文件在任何机器上都必须逐字节相同。

格式：
- 用 `serde_json::to_string_pretty` 输出（2 空格缩进），**末尾补一个换行**。
- 顶层字段属于引擎：`tick`、`sim_time`。以后加的引擎级状态（比如当前按下的动作）也放在顶层。
- 游戏状态整个放在**一个**字段下：G1.2 起是 `game`（第 11 节），替换了 E0–G1.1 的旋转立方体 `demo`。E4.3 起 `game` 的内容由场景的游戏决定：`souls` 见第 11 节（和 E4.2 逐字节相同），`rogue` 见第 15 节。状态导出里不另写是哪个游戏，场景文件里已经有了。
- 格式稳定性规则和 stdout 一样（见上文「格式稳定性」）。

结构（省略了数组的展开）：

```json
{
  "tick": 60,
  "sim_time": 1.0,
  "actions": [],
  "nodes": {
    "player": { "position": [0.0, 0.0, 0.0], "rotation": [x, y, z, w], "scale": [1.0, 1.0, 1.0] },
    "ground": { ... }
  },
  "game": { "player": { "position": [x, y, z], "facing": 0.0 }, "camera": { "yaw": 0.0, "pitch": -0.2617994 } }
}
```

- `actions` 是最后一个 tick 生效的动作集合（按名字排序），见第 9 节。
- `nodes`：场景里**每个节点**的**局部**变换，键是节点路径（第 10 节），按路径排序。旋转是四元数 `[x, y, z, w]`。
- `game` 的字段见第 11 节（`souls`）和第 15 节（`rogue`）。角度都是弧度。

## 7. 错误处理

- 库 crate：用 `thiserror` 定义各自的错误枚举。
- `loom_engine`：用 `anyhow` 汇总错误，在 `main` 里转成错误 JSON 和退出码 1。
- 不因为可恢复的情况 panic，比如窗口最小化后尺寸为 0，或者 surface 丢失（Lost/Outdated）。

## 8. 已知限制

有意暂不处理的问题记在 `docs/TODO.md` 的「延后项」（`[>]`）里，每条都有理由和启用条件。

## 9. 输入

参考 Godot 的 `InputMap` 和 `Input`（见 `docs/godot-reference.md`）。

### 分层

```
OS 按键事件 --(loom_platform 翻译)--> Key --(InputCollector 按 tick 采样 + ActionMap)--> 动作集合 --> ActionState --> 模拟
                                                                   或：输入脚本回放 ------^
```

- **模拟只读 `ActionState`**，永远不直接看按键。这样键盘、脚本回放、以后的手柄走的是同一条路，模拟代码不用区分。
- 全部类型都在 `loom_core`，与平台无关，可以在无头模式下测试。`loom_platform` 只负责把 winit 的按键码翻译成 `Key`。

### 按键 `Key`

- `loom_core` 自己定义的枚举，变体名用 W3C `KeyboardEvent.code` 的名字（`KeyW`、`Digit1`、`ArrowUp`、`ShiftLeft`……），和 winit 的 `KeyCode` 同名。按的是**物理位置**，不受键盘布局影响。
- 目前的范围：`KeyA`–`KeyZ`、`Digit0`–`Digit9`、四个方向键、`Space`、`Enter`、`Tab`、`Backspace`、左右 `Shift`/`Control`/`Alt`，以及 E4.3 为 `rogue` 加的小键盘 `Numpad1`–`Numpad9` 和 `Period`（句号键）。其他按键平台层直接忽略。
- 小键盘按的也是物理位置：NumLock 关着时，Windows 把小键盘 8 当成「上」，但它的扫描码和方向键不同，winit 应当仍报 `Numpad8`（E4.3 要实测）。
- **`Escape` 不是 `Key`**：它保留给引擎用来退出，不能绑定。
- 鼠标的移动量作为轴的来源，见下文「模拟量（轴）」；鼠标按键和手柄以后再加。

### 动作映射 `ActionMap`

RON 文件，里面是两个列表（**不用 map**：实测 ron 遇到重复的 map 键会静默保留最后一个）。`axes` 见下文「模拟量（轴）」（G1 起）：

```ron
(
    actions: [
        (action: "roll", keys: [Space]),
    ],
    axes: [
        (axis: "move_x", keys: [(KeyD, 1.0), (KeyA, -1.0)]),
        (axis: "look_x", keys: [(KeyL, 0.04), (KeyJ, -0.04)], mouse: X, mouse_scale: 0.0025),
    ],
)
```

- 动作名必须匹配 `^[a-z][a-z0-9_]*$`
- 同一个动作不能出现两次；同一个动作的 `keys` 里不能有重复按键
- `keys` 可以为空（动作只由脚本驱动）；一个按键可以绑定到多个动作
- 结构体一律 `#[serde(deny_unknown_fields)]`：实测 ron 默认会静默忽略拼错的字段名

### 按 tick 采样

一个 tick 的动作集合在这个 tick 开始时确定一次。`InputCollector` 记住两样东西：当前按住的键，和「上次采样之后按下过的键」。

- 某个动作在这个 tick 生效，当且仅当它绑定的任意一个键**正按住**，或者**上次采样之后按下过**。采样后清空「按下过」。
- 所以两次 tick 之间按下又松开的短按，**恰好生效一个 tick**，不会丢。窗口模式一帧可能跑 0 个或多个 tick，这条规则让结果只取决于按键事件落在哪两个 tick 之间，不取决于帧率。
- 同一对采样之间的**多次**点按，合并成一次（生效一个 tick）。每个 tick 的动作只有「生效/不生效」，表达不了次数。60 Hz 下一个 tick 约 16 ms，人手按不出这么快的连击，所以不是问题；以后如果有「按了几次」的需求，要另行设计。
- 已经按住的键再收到按下事件（按键重复），不算新的一次按下。
- 窗口失去焦点时，释放所有按住的键，防止松手时窗口收不到事件而卡键。失焦之前还没被采样到的点按保留，照样生效一个 tick（「短按不会丢」优先）。
- 在 Windows 上，winit 失焦时本身就会根据系统的真实键盘状态，替仍按着的键补发松开事件；引擎的「失焦释放」是第二道保险。

`ActionState` 保存本 tick 和上一个 tick 的动作集合，提供 `pressed`、`just_pressed`、`just_released`。第 1 个 tick 的「上一个 tick」是空集。

### 输入脚本（回放和录制共用同一格式）

```ron
(
    version: 1,
    tick_hz: 60,
    events: [
        (tick: 1, action: "move_forward", down: true),
        (tick: 1, action: "move_right", down: true),
        (tick: 31, action: "move_right", down: false),
    ],
)
```

- **记录的是动作，不是按键**：改了键位，旧录制照样能回放。
- tick 从 1 开始数。`tick: t` 的事件改变的是**第 t 个 tick 用的动作集合**。脚本里没提到的 tick，动作集合保持不变。
- 校验，任何一条不满足都是运行时错误（退出码 1），错误信息要写出文件路径、事件下标（`events[i]`，从 0 开始）和 tick：
  - `version` 必须是 1，`tick_hz` 必须等于引擎的 `TICK_HZ`（60）
  - `tick` ≥ 1，并且整体不递减
  - 同一个 tick 里同一个动作只能出现一次
  - 动作必须在当前 `ActionMap` 里
  - 不能按下已经按住的动作，也不能松开没按住的动作
- **录制输出是规范形式**：事件按 `(tick, 动作名)` 排序；用 ron 的 `PrettyConfig`，`depth_limit(2)`（每个事件占一行）、`new_line("\n")`，末尾补一个换行。所以对任何合法脚本 S，「回放 S 并录制」得到的就是 S 排序后的规范形式。
- 退出时还按住的动作，录制里不补松开事件。

### 模拟量（轴）（G1 起）

决策（2026-09-26，用户确认）：黑魂 3 式自由环绕相机；G1 只做键鼠，手柄以后作为轴的另一个输入来源加进来。

动作是开关量（按了/没按），**轴**是每个 tick 一个 `f32`。移动（`move_x`、`move_y`）和视角（`look_x`、`look_y`）都是轴，这样以后接手柄摇杆时，游戏代码不用改。

**单位由游戏定义**，键位文件负责把输入换算成这个单位。比如游戏规定 `look_x` 的单位是「这个 tick 转多少弧度」，键位就写 `(KeyL, 0.04)`（按住每 tick 转 0.04 弧度）和 `mouse_scale: 0.0025`（鼠标每移动一个计数转 0.0025 弧度）。`move_x`、`move_y` 按键给 ±1，游戏负责把移动向量的长度限制在 1 以内。

轴的绑定：
- `axis`：名字规则同动作；**轴和动作不能同名**
- `keys`：`(按键, 值)` 的列表，同一个轴里按键不能重复，值必须有限
- `mouse`：可选，`X` 或 `Y`，取鼠标的**原始移动量**（winit 的 `DeviceEvent::MouseMotion`，不受指针加速和屏幕边缘影响）；有 `mouse` 时必须有 `mouse_scale`，反之亦然。符号约定：鼠标向右 X 为正、向下 Y 为正，要反转就给负的 `mouse_scale`

**采样**（和动作一样，每个 tick 开始时确定一次）：
- 轴的值 = 所有「正按住或上次采样后按下过」的键的值之和 + 上次采样以来累计的鼠标移动量 × `mouse_scale`。**引擎不做截断**，由游戏按需处理。
- 鼠标累计量采样后清零。所以一帧跑多个 tick 时，这一帧的鼠标移动全部归到第一个 tick，后面的 tick 是 0。
- 失焦时鼠标累计量也清零。
- `ActionState::axis(name) -> f32`：本 tick 的值；没有输入时是 0.0。

**窗口模式抓鼠标**：窗口获得焦点时隐藏指针并锁定（`CursorGrabMode::Locked`，不支持时退回 `Confined`），失焦时释放。Esc 照常退出。`--input` 回放时鼠标和键盘一样被忽略。
- E4.3 起，**只有键位表里有鼠标轴（带 `mouse` 的轴）时才抓**。`rogue` 只用键盘，抓住指针没有意义。判断放在引擎里（`ActionMap::uses_mouse()`），不需要游戏另外声明。

**输入脚本**加一个可选的 `axes` 列表（缺省为空，`version` 仍是 1）：

```ron
(
    version: 1,
    tick_hz: 60,
    events: [],
    axes: [
        (tick: 1, axis: "move_y", value: 1.0),
        (tick: 20, axis: "look_x", value: 0.0175),
        (tick: 21, axis: "look_x", value: 0.0),
        (tick: 90, axis: "move_y", value: 0.0),
    ],
)
```

- **电平语义**：`tick: t` 把这个轴的值设为 `value`，一直保持到这个轴的下一个事件。鼠标这种一次性的移动，录下来就是「设为 v」加上下一个 tick「设回 0」。
- 校验：轴必须在映射里；`value` 必须有限；`tick` ≥ 1 并且在 `axes` 列表里整体不递减；同一个 tick 同一个轴只能出现一次；**不能把轴设成它当前的值**（和「不能按下已按住的动作」同理）。错误信息写 `axes[i]` 和 tick。
- 录制的规范形式：`axes` 按 `(tick, 轴名)` 排序，**总是写出**（没有就写 `axes: []`），放在 `events` 后面。
- 实测：ron 写出 f32 用最短的往返表示，读回按位相同（抽查约 3 万个值，0 个不同），所以录制的轴值可以逐位复现。

**状态导出**在顶层 `actions` 后面加 `axes`：映射里的每个轴 → 最后一个 tick 的值，按名字排序。

## 10. 场景

参考 Godot 的节点树和 `.tscn`（见 `docs/godot-reference.md`）。决策（2026-09-26，用户确认）：**节点树加可选组件**，不做 ECS。

### 模型

- 场景是一棵**有名字的节点树**。每个节点有局部变换（位置、旋转、缩放），可以有子节点。
- 节点身上可以挂几种**可选组件**，现在是：`mesh`（加 `color`）、`camera`、`light`。以后的 hitbox、耐力等也按这个方式加字段。
- 用**路径**指代节点：从根开始的名字用 `/` 连接，比如 `arena/pillar_3`。路径是状态导出和以后远程控制的地址。
- 游戏逻辑（Rust）按路径找到节点，每个 tick 读写它的局部变换。**节点树是模拟状态的一部分**，受第 4 节确定性规则约束。
- 运行时的节点树用「数组加下标」存储（`NodeId` 是下标），不用对象之间互相引用。

### 场景文件

RON，所有结构体 `deny_unknown_fields`：

```ron
(
    version: 1,
    camera: "main_cam",               // 当前相机节点的路径
    ambient: (0.2, 0.2, 0.25),        // 环境光，线性 RGB
    nodes: [
        (name: "ground", mesh: Plane, scale: (20.0, 1.0, 20.0), color: (0.35, 0.35, 0.38)),
        (name: "cube", position: (0.0, 0.5, 0.0), mesh: Cube, color: (0.8, 0.25, 0.2)),
        (name: "sun", position: (4.0, 8.0, 3.0), look_at: (0.0, 0.0, 0.0),
            light: (color: (1.0, 0.95, 0.9), intensity: 1.0)),
        (name: "main_cam", position: (2.5, 2.0, 3.5), look_at: (0.0, 0.5, 0.0),
            camera: (fov_y: 60.0, near: 0.1, far: 100.0)),
    ],
)
```

顶层字段 `version`、`camera`、`ambient`、`nodes` **都必填**（E2 审查决定：场景文件多由 AI 手写，宁可显式）。可选字段不用写 `Some(...)`，解析时开了 ron 的 `IMPLICIT_SOME` 扩展。

E4.3 起多一个顶层字段 `game`：字符串，用哪个游戏跑这个场景（第 1 节「选择游戏」）。`loom_scene` 不认识具体游戏，只检查写了的话不能是空串，`Scene::game()` 返回 `Option<&str>`。
- `loom run`、`loom serve` 必须有 `game`，缺了或不认识就按「场景无效」报错：
  - 缺：`the scene does not say which game runs it: add game: "rogue" or game: "souls" at the top level`
  - 不认识：`unknown game "rouge": the games are "rogue" and "souls"`
- `loom view` 不跑游戏，所以 `game` 可以不写（例如 `assets/scenes/tiles_preview.ron`）。
- **热重载不能换游戏**（E4.3 规划者决定）：改过的场景写的 `game` 和正在跑的不同时，按重载失败处理（`reload_errors` 加 1，旧场景继续跑），日志写 `hot reload cannot switch games: the scene now says "rogue", but "souls" is running; restart loom to switch`。换游戏会连带换键位表，正在回放的 `--input` 脚本、抓鼠标的状态都要跟着变，不值得为这种用法处理。serve 的 `load_scene` 是显式换场景，可以换游戏（第 13 节）。

节点字段（除 `name` 外都可省略）：

| 字段 | 类型 | 默认 | 说明 |
|---|---|---|---|
| `name` | 字符串 | 必填 | `^[a-z][a-z0-9_]*$`；同一父节点下不能重名 |
| `position` | `(x, y, z)` | `(0, 0, 0)` | 在父节点空间里 |
| `rotation` | `(x, y, z)` 度 | `(0, 0, 0)` | YXZ 欧拉角（第 5 节） |
| `look_at` | `(x, y, z)` | 无 | 让节点的 -Z 指向父节点空间里的这个点，+Y 朝上。**不能和 `rotation` 同时写** |
| `scale` | `(x, y, z)` | `(1, 1, 1)` | 每个分量必须 > 0 |
| `mesh` | `Cube` / `Plane` / `Sphere` / `Capsule(radius, height)` | 无 | 前三种都是单位大小：`Cube` 边长 1、中心在原点；`Plane` 是 XZ 平面上 1×1、法线 +Y；`Sphere` 直径 1。大小用 `scale` 调。`Capsule` 带参数（非均匀缩放会把半球压扁）：`height` 是含两端半球的总高，**原点在底部**（占 y ∈ [0, height]），方便角色站在地面上 |
| `color` | `(r, g, b)` 线性 | `(0.8, 0.8, 0.8)` | 只有带 `mesh` 时才有意义；分量在 0 到 1 之间 |
| `camera` | `(fov_y, near, far)` 或 `(ortho: (height), near, far)` | 无 | 相机看向自己的 -Z。`fov_y` 是透视相机的垂直视场角（度）。`ortho` 是正交相机（E4.2 起），`height` 是画面竖直方向覆盖的世界长度，宽度按画面比例算。`fov_y` 和 `ortho` 必须**恰好写一个** |
| `sprite` | `(sheet, tag)` 或 `(sheet, tag, alternates)` | 无 | 精灵（E4.2 起，第 14 节）：`sheet` 是 sprite sheet 的 JSON 路径，`tag` 是其中的 tag 名。`alternates`（E4.4 起，可省略）是游戏运行中可以换成的其他 tag，例如 `["corpse"]` |
| `tilemap` | `(sheet, legend, rows)` | 无 | 瓦片地图（E4.2 起，第 14 节）：`legend` 是 `[(字符, tag)]` 列表，`rows` 是字符串列表，一个字符一个格子 |
| `light` | `(color, intensity)` | 无 | 方向光，照向节点的 -Z |
| `character` | `(run_speed, turn_speed)` | 无 | 角色组件（第 11 节）。`run_speed` 米/秒，`turn_speed` 度/秒 |
| `orbit_camera` | `(target, pivot_height, distance, yaw, pitch, pitch_min, pitch_max, collision_radius, return_speed)` | 无 | 环绕相机组件（第 11 节），必须和 `camera` 在同一个节点上。`target` 是带 `character` 的节点路径；角度都是度。`collision_radius`（米）、`return_speed`（米/秒）是 G1.7 起的相机碰撞参数，必填 |
| `props` | `[(名字, 数值)]` | 空 | 游戏自定义的数值参数（E4.4 起），例如 `[("hp", 10), ("attack", 1)]`。引擎不解释它们，由游戏读取和校验（`rogue` 见第 15 节）。写在场景里，所以热重载就能调 |
| `collider` | `Box` | 无 | 静态碰撞体（第 11 节「碰撞」，G1.3 起）：和 `Cube` 一样是单位立方体，用节点的 `scale`、`rotation`、`position` 定大小、朝向和位置。只挡角色，不参与渲染（要看得见就另外写 `mesh: Cube`） |
| `children` | 节点列表 | `[]` | |

`look_at` 的算法**固定如下**（换一种等价算法，四元数的最后一位可能不同，状态导出就对不上了）：

```text
back  = normalize(position - target)
right = normalize(Vec3::Y.cross(back))
up    = back.cross(right)
rotation = Quat::from_mat3(&Mat3::from_cols(right, up, back))
```

校验（任何一条不满足都是运行时错误，退出码 1，错误信息带文件路径，结构错误带行列号，语义错误带节点路径）：
- `version` 必须是 1
- 名字格式、同级重名
- `camera` 必须指向一个存在的、带 `camera` 组件的节点
- 整个场景**最多一个** `light`（以后要多光源时再扩展）；**最多一个** `character`、**最多一个** `orbit_camera`（G1）
- `Capsule`：`radius > 0`，`height ≥ 2 × radius`
- `character`：`run_speed > 0`，`turn_speed > 0`
- `orbit_camera`：节点上必须也有 `camera`；`target` 必须指向带 `character` 的节点；`pivot_height`、`distance` 有限且 `distance > 0`；`-90 < pitch_min < pitch_max < 90`；`pitch` 在 `[pitch_min, pitch_max]` 里；`collision_radius` 有限且 `0 < collision_radius < pivot_height`（相机球从地面以上出发）；`return_speed` 有限且 `> 0`
- `collider`（G1.3）：自己的旋转只能绕 Y 轴（`rotation` 的 x、z 分量必须是 0，不能用 `look_at`）；**所有祖先节点**只能平移（`rotation` 不写或写 `(0, 0, 0)`，没有 `look_at`，`scale` 不写或写 `(1, 1, 1)`）；不能和 `character` 在同一个节点上
- `rotation` 和 `look_at` 不能同时出现；`look_at` 的目标不能等于 `position`，方向也不能和 Y 轴平行（`right` 会退化）
- 所有数字必须是有限值；`scale` 分量 > 0；颜色分量在 0 到 1 之间（节点 `color`、`light.color`、`ambient` 都是）；`light.intensity` ≥ 0；`fov_y` 在 (0, 180) 之间；`ortho.height` > 0；`0 < near < far`
- `props`（E4.4）：名字匹配 `^[a-z][a-z0-9_]*$`，同一个节点里不能重复；数值必须有限。用列表而不是 map，原因和键位表一样（ron 遇到重复的 map 键会静默保留最后一个）。数值存成 `f64`，整数在 2^53 以内都是精确的。`Node::prop(名字) -> Option<f64>`。
- `sprite` 的 `alternates`（E4.4）：每个都不能是空串，不能重复，也不能等于 `tag`。
- `sprite`、`tilemap`（E4.2）：
  - 不能和 `mesh` 挂在同一个节点上，两者也不能同时挂；
  - `tilemap` 的 `rows` 至少一行，每行字符数相同且至少 1；每个字符都要在 `legend` 里；`legend` 里的字符不能重复；
  - 这些是 `loom_scene` 能查的。`sheet` 文件存不存在、`tag` 在不在 sheet 里，要读文件才知道，由 `loom_engine` 在加载场景时检查（第 14 节）

### 游戏和场景的关系

- 场景顶层的 `game` 决定用哪个游戏跑它（第 1 节「选择游戏」）。`loom view` 不跑游戏，不看 `game`。
- 不给 `--scene` 时，用 `souls` 里内置的演示场景，用 `include_str!` 嵌进程序。`rogue` 没有内置场景，用 `assets/scenes/rogue.ron`。
- `rogue` 对场景的要求见第 15 节。以下是 `souls` 的要求。
- G1.2 起，游戏要求场景里有一个带 `character` 的节点，并且场景的当前相机（顶层 `camera`）带 `orbit_camera`、`target` 指向这个角色；角色节点和这个相机节点都必须是**根节点**（游戏直接写它们的局部变换）。G1.3 起，角色节点必须带 `mesh: Capsule(...)`（它同时是碰撞形状），`run_speed / 60` 必须小于胶囊体半径（否则一个 tick 可能穿过薄墙）；`collider` 不能在角色或相机节点的子树里（碰撞体必须是静态的）。G1.7 起，角色节点的 `scale` 必须不写或写 `(1, 1, 1)`（渲染会缩放胶囊体，碰撞不会）。不满足就报错。游戏逻辑见第 11 节。
- E0–G1.1 的旋转立方体演示（根节点 `cube`）在 G1.2 被角色取代。

### 渲染

- `loom_render` 不知道场景的存在。`loom_engine` 每帧把场景转换成一份「帧描述」交给渲染器：相机（视图矩阵、投影参数）、环境光、可选的一个方向光、以及一串绘制项（网格种类、世界矩阵、颜色）。
- 光照：`color * (ambient + max(0, dot(n, -light_dir)) * light_color * intensity)`，法线用世界矩阵的逆转置变换（支持非均匀缩放）。
- 相机的视图矩阵就是相机节点世界矩阵的逆。

### 热重载

- 只在**窗口模式**、并且给了 `--scene` 时生效。无头模式从不重载。
- 用墙钟每 0.5 秒检查一次文件的修改时间（不引入文件监听的新依赖）。这件事发生在模拟之外，不影响确定性。
- 文件变了就重新解析和校验：
  - **失败**：打一条 `warn` 日志说明原因，**保持旧场景继续跑**，`reload_errors` 加 1。
  - **成功**：用新场景**从 tick 0 重新开始**（决策，2026-09-26）。时钟、动作状态、输入录制、脚本回放全部重来；窗口和渲染器保留。`reloads` 加 1。
- 所以退出时写出的录制和状态导出，只覆盖最后一次重载之后的那段运行。它们配合当时的场景文件，照样能逐字节复现。

细节（E2.4）：
- **修改时间变了才读文件内容**；内容和当前场景的源文本逐字节相同时，什么也不做（有的编辑器保存时会碰修改时间但不改内容）。
- **文件暂时不存在**（很多编辑器保存时先删后写或者改名替换）不算错误，下次检查再试。
- **同一份坏内容只算一次** `reload_errors`：记住上次失败内容，没变就不重复计数、不重复打日志。
- 重载时**键盘状态保留**：仍然按住的键继续算按住，不用松开再按。脚本回放从第 1 个 tick 重新开始。
- 报告里 `frames` 是整次运行渲染的总帧数；`ticks`、`sim_time`、`input.applied` 只算最后一段（重载之后）。

## 11. 角色与相机（G1.2 起）

决策（2026-09-26，用户确认）：黑魂 3 式自由环绕相机，角色**相对相机方向**移动，并**平滑快速地**转向移动方向。游戏参数写在场景文件的组件里（`character`、`orbit_camera`），所以可以用热重载在运行中调手感。

实现在 `souls`（E4.3 之前叫 `loom_sim`），受第 4 节确定性规则约束：三角函数一律用 `libm`，角度用 `f32` 弧度。

### 每个 tick 的计算（顺序和公式固定）

改了任何一步的顺序或者写法，状态导出就会对不上，测试的期望值也要重算。

```text
// 1. 相机角度（look_x 向右为正，look_y 向上为正，单位是这个 tick 转的弧度）
yaw   = wrap(yaw - look_x)                  // wrap 见第 5 步；yaw 始终在 [-π, π)
pitch = clamp(pitch + look_y, pitch_min, pitch_max)

// 2. 移动输入，长度限制在 1 以内
v = Vec2(move_x, move_y)
if v.length_squared() > 1.0 { v = v / v.length() }

// 3. 相对相机的方向（yaw = 0 时相机朝 -Z）
(s, c)  = (libm::sinf(yaw), libm::cosf(yaw))
forward = Vec3(-s, 0, -c)
right   = Vec3( c, 0, -s)
dir     = right * v.x + forward * v.y

// 4. 移动，然后把胶囊体推出碰撞体（G1.3，见下文「碰撞」）
//    G2.1 起这一步换成下文「耐力、翻滚、冲刺」里的第 4 步；没有按翻滚键时结果和这里逐位相同
position = resolve(position + dir * (run_speed / 60.0))

// 5. 转身：朝 dir 转，每个 tick 最多转 turn_speed 度的 1/60（G2.1 起，翻滚和后撤步期间不做）
if v.length_squared() > 0.0 {
    target = libm::atan2f(-dir.x, -dir.z)
    diff   = wrap(target - facing)
    max    = turn_speed.to_radians() / 60.0
    facing = wrap(facing + clamp(diff, -max, max))
}
wrap(a) = a - TAU * floor((a + PI) / TAU)     // 包到 [-π, π)，floor 是精确运算

// 6. 写回节点
角色节点：position = position，rotation = Quat::from_rotation_y(facing)
相机节点：rot    = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch)
          pivot  = position + Vec3(0, pivot_height, 0)
          target = obstruct(pivot, rot * Vec3::Z)          // G1.7，见下文「相机碰撞」
          if target < cam_distance { cam_distance = target }                           // 挡住：立刻拉近
          else { cam_distance = min(cam_distance + return_speed / 60.0, target) }      // 否则按固定速度退回
          position = pivot + rot * Vec3(0, 0, cam_distance)
          rotation = rot
```

其中 `run_speed`、`turn_speed`、`pivot_height`、`distance`、`pitch_min`、`pitch_max`、`collision_radius`、`return_speed` 来自场景组件（角度在加载时转成弧度：`deg.to_radians()`）。`return_speed / 60.0` 在加载时算一次。`cam_distance` 是相机当前离 `pivot` 的距离，属于游戏状态（进状态导出）；场景里的 `distance` 是它的上限。

没有东西挡住时 `cam_distance == distance`，`min(distance + s, distance)` 就是 `distance`，相机位置和 G1.2 的公式逐位相同（`position + Vec3(0, pivot_height, 0)` 先算，和原来的结合顺序一样）。

### 初始状态（tick 0）

- `position`：角色节点在场景里的位置（胶囊体的底部，也就是脚下）
- `facing`：由角色节点在场景里的旋转算出：`f = rotation * Vec3::NEG_Z; facing = wrap(libm::atan2f(-f.x, -f.z))`，**再把 `-0.0` 规范成 `0.0`**（没有旋转时 `-f.x` 是 `-0.0`，`atan2f(-0.0, 1.0)` 得 `-0.0`，`wrap` 保留负零）
- `yaw`：`wrap(orbit_camera.yaw.to_radians())`；`pitch`：`orbit_camera.pitch.to_radians()`
- G1.3 起初始的 `yaw`、`facing` 也经过 `wrap`，所以任何时刻都在 `[-π, π)` 里（G1.2 时 tick 0 可能是场景里写的 270° 或 `atan2f` 给出的 `+π`）。初始位置**不做**碰撞推出，第 1 个 tick 才推
- 加载时就按第 6 步写一次相机节点，所以 tick 0 的画面已经是正确的环绕视角。G1.7 起，加载时 `cam_distance` **直接取** `obstruct(...)` 的结果（不经过「按固定速度退回」），所以 tick 0 的相机也不会穿墙

### 状态导出

```json
"game": {
  "player": { "position": [x, y, z], "facing": f },
  "camera": { "yaw": f, "pitch": f, "distance": f }
}
```

- `camera.distance`（G1.7 起）：`cam_distance`，相机当前离 `pivot` 的距离
- `player` 的 `stamina`、`stamina_wait`、`action`、`action_tick`、`invulnerable`（G2.1 起）见下文「耐力、翻滚、冲刺」

### 耐力、翻滚、冲刺（G2.1 起）

用户决定（2026-09-27）：
- 耐力和翻滚一起做（原 G2.1、G2.2 合并）。
- 一个键（`roll`，空格）：**点按松开时翻滚，按住超过一小段时间就冲刺**，和黑魂 3 一样。
- **只要耐力大于 0 就能出招**，扣到 0 为止。
- **不推方向时按翻滚是后撤步**：向身后跳一小步，距离短、无敌帧少、消耗少。

**参数**写在角色节点的 `props` 里（第 10 节），热重载就能调。
- 都可以省略，省略时用下表的默认值；演示场景把它们全部显式写出来。这里和 `rogue` 不同，没有要求必填：参数有 16 个，要求必填的话，每个测试场景都得抄一遍。
- 写了不认识的名字就报错（防拼写错误）。

| prop | 默认 | 单位、规则 |
|---|---|---|
| `stamina_max` | 100 | > 0 |
| `stamina_regen` | 45 | 每秒恢复，> 0 |
| `stamina_delay_ticks` | 30 | 用过耐力后，等多少 tick 才开始恢复；整数 ≥ 0 |
| `tap_ticks` | 15 | 按住不超过这么多 tick 就松开算「点按」；整数 ≥ 1 |
| `sprint_speed` | 6 | 米/秒，> 0 |
| `sprint_cost` | 15 | 冲刺时每秒消耗，> 0 |
| `roll_cost` | 20 | > 0 |
| `roll_ticks` | 32 | 翻滚持续的 tick 数，整数 ≥ 1 |
| `roll_distance` | 3.2 | 米，> 0 |
| `roll_invuln_from`、`roll_invuln_to` | 1、20 | 无敌帧：翻滚的第几个 tick 到第几个 tick（从 1 数，含两端），整数，`1 ≤ from ≤ to ≤ roll_ticks` |
| `backstep_cost` | 12 | > 0 |
| `backstep_ticks` | 20 | 整数 ≥ 1 |
| `backstep_distance` | 1.5 | 米，> 0 |
| `backstep_invuln_from`、`backstep_invuln_to` | 1、8 | 同上，`≤ backstep_ticks` |

- 加载时换算成每 tick 的量：`sprint_step = sprint_speed / 60`，`sprint_drain = sprint_cost / 60`，`regen_step = stamina_regen / 60`，`roll_step = roll_distance / roll_ticks`，`backstep_step = backstep_distance / backstep_ticks`。props 是 f64，先 `as f32` 再算。
- 和 `run_speed` 一样防穿墙：`sprint_step`、`roll_step`、`backstep_step` 都必须**小于胶囊体半径**。
- 报错（`souls::GameError`，按「场景无效」报出；`<名字>` 是角色节点的路径）：
  - `the character "<名字>": unknown prop "<p>"`
  - `the character "<名字>": prop "<p>" must be greater than 0, got v`
  - `the character "<名字>": prop "<p>" must be a whole number of at least N, got v`（N 是 1，`stamina_delay_ticks` 是 0）
  - `the character "<名字>": needs 1 <= roll_invuln_from <= roll_invuln_to <= roll_ticks`（后撤步同理，把 `roll` 换成 `backstep`）
  - `the character "<名字>": <sprint|roll|backstep> moves d m per tick, which must be less than the capsule radius r`
  - 检查顺序：不认识的名字（按文件顺序）→ 每个 prop 的取值（按上表顺序）→ 无敌帧区间（先 roll 后 backstep）→ 每 tick 距离（sprint、roll、backstep）。这些都在原有的检查（角色、相机、`TooFast`、`MovingCollider` 等）之后（G2.1 审查决定）。
  - 整数类参数没有上限：写成 `roll_ticks: 1e12` 也能通过，转成 u32 时截到最大值，结果只是动作「永远不结束」，不会出错，所以不加上限（G2.1 审查决定）。

**状态**（除了原有的 `position`、`facing`）：
- `stamina`（f32，开局 = `stamina_max`）、`stamina_wait`（还要等多少 tick 才恢复，开局 0）；
- `action`：`idle`、`sprint`、`roll`、`backstep` 之一；`action_tick`：翻滚或后撤步进行到第几个 tick（从 1 数），其他时候是 0；
- 不进状态导出的内部状态：`hold`（翻滚键已经连续按住了几个 tick）、`spent`（这次按住的冲刺已经把耐力用光）、`action_dir`（翻滚或后撤步的方向，单位向量）。

**每个 tick**：第 1–3 步不变，第 4、5 步换成下面这样，第 6 步不变。

```text
// 0.（在第 1 步之前）翻滚键
if input.pressed("roll") {
    hold = min(hold + 1, tap_ticks + 1)                          // 封顶，不会溢出
    tap = false
} else {
    tap = input.just_released("roll") && hold <= tap_ticks       // hold 是松开前按住的 tick 数
    hold = 0; spent = false
}

// 4. 动作和移动
busy = true                                                       // 这个 tick 有没有用耐力或处于动作中
if action == roll || action == backstep {
    action_tick += 1
    position = resolve(position + action_dir * step_of(action))
    if action_tick == ticks_of(action) { action = idle; action_tick = 0 }     // 最后一个 tick 仍然移动
} else if tap && stamina > 0 {
    if v.length_squared() > 0.0 {                                  // 翻滚：朝输入方向，立刻转过去
        action = roll; action_dir = dir / dir.length()
        facing = wrap(libm::atan2f(-action_dir.x, -action_dir.z))
        if facing == 0.0 { facing = 0.0 }                          // 把 -0.0 规范成 0.0，同初始状态
    } else {                                                       // 后撤步：朝身后，不转身
        action = backstep
        action_dir = Vec3(libm::sinf(facing), 0.0, libm::cosf(facing))
    }
    stamina = max(stamina - cost_of(action), 0.0); stamina_wait = stamina_delay_ticks
    action_tick = 1
    position = resolve(position + action_dir * step_of(action))
    if ticks_of(action) == 1 { action = idle; action_tick = 0 }
} else {
    sprinting = input.pressed("roll") && hold > tap_ticks && !spent && stamina > 0.0 && v.length_squared() > 0.0
    position = resolve(position + dir * (sprinting ? sprint_step : run_step))
    if sprinting {
        action = sprint
        stamina = max(stamina - sprint_drain, 0.0); stamina_wait = stamina_delay_ticks
        if stamina == 0.0 { spent = true }
    } else {
        action = idle; busy = false
    }
    第 5 步：转身（原样）
}

// 5b. 耐力恢复
if !busy {
    if stamina_wait > 0 { stamina_wait -= 1 } else { stamina = min(stamina + regen_step, stamina_max) }
}
invulnerable = (action == roll || action == backstep) && invuln_from_of(action) <= action_tick <= invuln_to_of(action)
```

- `run_step` 就是原来的 `run_speed / 60.0`。没按翻滚键时 `sprinting` 是 false，第 4 步和 G1 逐位相同。
- **点按**：`tap` 在松开的那个 tick 触发，所以翻滚比按下晚出，晚的时间就是按住的时间（黑魂 3 同样如此）。两次采样之间按下又松开的短按，`hold` 是 1，照样算点按。
- **冲刺**：按住超过 `tap_ticks` 并且推着方向才冲刺，松开时不翻滚。冲刺把耐力用光后（`spent`），这次按住就只能普通跑，要松开重按才能再冲刺。
- **翻滚和后撤步期间**：移动输入和转身都不起作用，相机照常能转；按键的松开不会排队，动作期间的点按直接丢掉。
- 翻滚方向用的是开始那个 tick 的 `dir`（相对相机），角色立刻转向翻滚方向。
- 结束的那个 tick（`action_tick == ticks`）算作动作中：仍然移动，`busy` 为 true，不恢复耐力；`invulnerable` 按结束后的 `idle` 算，是 false。
- 只用加减乘除、`sqrt`、`min`、`max` 和 libm 的三角函数，符合第 4 节。

**状态导出**的 `player`（G2.1 起，字段按这个顺序）：

```json
"player": { "position": [x, y, z], "facing": f, "stamina": 100.0, "stamina_wait": 0, "action": "idle", "action_tick": 0, "invulnerable": false }
```

- 无敌帧现在只是状态，还没有东西会打到角色。攻击判定是 G3 的事。
- **没有画面上的耐力条**：要显示得先有 HUD 或文字渲染，还没有使用者。目前在状态导出里看。

### 碰撞（G1.3 起）

用户决定（2026-09-27）：碰撞体用显式组件 `collider: Box` 指定；斜着撞墙时只保留沿墙的分量（正面撞墙就停住）。

G1 只有平地，没有重力和跳跃，角色的 y 永远不变，所以角色「对地面的碰撞」不需要计算（相机对地面的碰撞见下文「相机碰撞」）。剩下的是**竖直胶囊体对只绕 Y 旋转的方块**，等价于 XZ 平面上的**圆对有向矩形**，再加一个高度范围的判断。

加载时，按节点的先序（场景数组的顺序）为每个 `collider` 算一次（`m = scene.world_matrix(id)`，`t` 是节点的局部变换）：

```text
center = Vec2(m.w_axis.x, m.w_axis.z)
a3 = t.rotation * Vec3::X;  a = Vec2(a3.x, a3.z)      // 方块局部 X 轴在 XZ 平面上的方向
b3 = t.rotation * Vec3::Z;  b = Vec2(b3.x, b3.z)      // 方块局部 Z 轴
ha = 0.5 * t.scale.x;  hb = 0.5 * t.scale.z           // 半宽
y_min = m.w_axis.y - 0.5 * t.scale.y;  y_max = m.w_axis.y + 0.5 * t.scale.y
```

（祖先只能平移，所以局部的旋转、缩放就是世界的旋转、缩放。）

`resolve(p)`，其中 `r`、`h` 是角色胶囊体的 `radius`、`height`：

```text
repeat 4 times:
  for box in colliders (先序):
    if box.y_max <= p.y || box.y_min >= p.y + h: continue        // 高度上不重叠
    d  = Vec2(p.x, p.z) - box.center
    la = d.dot(box.a);  lb = d.dot(box.b)                         // 方块局部坐标
    qa = clamp(la, -box.ha, box.ha);  qb = clamp(lb, -box.hb, box.hb)   // 最近点
    if la == qa && lb == qb:                                      // 圆心在方块里
      if box.ha - |la| <= box.hb - |lb|:                          // 从穿透最浅的一侧推出
        na = (la >= 0 ? 1 : -1) * (box.ha + r);  nb = lb
      else:
        na = la;  nb = (lb >= 0 ? 1 : -1) * (box.hb + r)
    else:
      off = Vec2(la - qa, lb - qb);  d2 = off.length_squared()
      if d2 >= r * r: continue                                    // 没碰到
      n = Vec2(qa, qb) + off * (r / sqrt(d2));  na = n.x;  nb = n.y
    push = box.a * (na - la) + box.b * (nb - lb)
    p.x += push.x;  p.z += push.y
return p
```

- 这就是「只保留沿墙分量」：推出只改变垂直于墙的那一维，沿墙的那一维不动。
- 所有运算都是加减乘除和 `sqrt`（精确舍入），`clamp`、`abs` 也是精确的，不需要 libm。
- 对轴对齐的方块，`a = (1, 0)`、`b = (0, 1)`，推出时另一个坐标**按位不变**。
- 固定迭代 4 轮，不看是否已经收敛：轮数固定，结果才和机器无关。
- 碰撞后，转身（第 5 步）仍然朝向**输入方向** `dir`，不是实际移动的方向：顶着墙跑时角色面朝墙。

### 相机碰撞（G1.7 起）

用户决定（2026-09-27）：相机被挡住时**立刻拉近**，挡住的东西移开后**按固定速度退回**（`return_speed`，米/秒）；有了相机碰撞，演示场景的 `pitch_max` 从 20° 放开到 **60°**，仰视时相机碰到地面就拉近到角色身边。

相机看作半径 `cr = collision_radius` 的球，从 `pivot` 沿方向 `u = rot * Vec3::Z` 出发，最远走 `distance`。`obstruct(pivot, u)` 返回球第一次碰到东西时走过的距离，没碰到就是 `distance`。挡住相机的东西：

- **地面**：角色脚下的水平面（G1 只有平地，角色的 y 不变）。球心不能低于脚下 `cr`。
- **所有 `collider: Box`**：用加载时算好的同一份 `BoxCollider`（上文），把方块在三个方向上各**向外扩 `cr`**，再做射线对扩大后的方块的求交（slab 方法）。

```text
obstruct(pivot, u):
    t = distance
    if u.y < 0.0 { t = min(t, (cr - pivot_height) / u.y) }       // 地面；pivot_height > cr，所以结果 > 0
    uxz = Vec2(u.x, u.z)
    for box in boxes（先序）:
        o  = Vec2(pivot.x, pivot.z) - box.center
        t0 = 0.0; t1 = distance
        for (s, w, lo, hi) in [(o.dot(a),  uxz.dot(a), -(ha + cr),    ha + cr),     // 顺序固定：a、y、b
                               (pivot.y,   u.y,        y_min - cr,    y_max + cr),
                               (o.dot(b),  uxz.dot(b), -(hb + cr),    hb + cr)]:
            if w == 0.0 {
                if s < lo || s > hi { 跳过这个方块 }              // 平行于这一对面，而且在外面
            } else {
                ta = (lo - s) / w; tb = (hi - s) / w
                t0 = max(t0, min(ta, tb)); t1 = min(t1, max(ta, tb))
            }
        if t0 <= t1 { t = min(t, t0) }                          // pivot 在扩大后的方块里时 t0 = 0
    return t
```

- 只用加减乘除、`min`、`max`、比较，都是精确运算，不需要 libm。`w == 0.0` 的分支要保留：`yaw = 0` 时 `u.x` 恰好是 0，轴对齐方块的 `a = (1, 0)`，`w` 恰好是 0。
- 角色自己不挡相机（它不是 `collider`）。
- `collision_radius` 要大于相机近裁剪面的半对角线，否则贴着墙时近裁剪面会切进墙里。演示场景 `near` 0.1、`fov_y` 60°、16:9 时半对角线约 0.12 m，`collision_radius` 用 0.2。

### 已知限制

- **碰撞只对静态方块**：没有胶囊体对胶囊体、没有斜坡和台阶。高度上重叠的方块一律挡住，矮到脚下的（`y_max <= 脚的高度`）一律忽略，没有「迈上去」。
- **两个方块夹出的窄缝和凹角**：4 轮推出不保证完全分开，可能留下很小的重叠（下一个 tick 继续推）。
- **相机可能进到角色身体里**：背靠墙时相机被拉到很近（规划者原型：斜墙场景里约 0.16 m），会在胶囊体里面。以后如果需要，再在相机很近时隐藏或淡出角色。
- **相机碰撞把方块的角当成方的**：方块向外扩 `cr` 得到的是更大的方块，而不是圆角方块，所以在方块的棱角附近，相机会比必要的早一点点拉近（棱附近最多多出约 `0.41 * cr`，顶点附近约 `0.73 * cr`）。
- 没有加速和减速：移动速度在一个 tick 内从 0 到满。

## 12. 外部工具对接（2026-09-27 起的原则）

用户希望 loom 能和 Aseprite、Blender、Houdini、Substance、Cascadeur、REAPER、SuperCollider、FL Studio、FMOD 这类工具通过 AI 协作。原则：

1. **松耦合，AI 在中间串联**。AI 用工具自己的命令行或脚本接口操作它（例如 `blender -b --python`、`hython`、`sbsrender`、`aseprite -b --script`），用 loom 的远程控制（E3）加载资源、截图、导出状态。loom 不链接这些工具的代码，也不调用它们。
2. **交换用开放格式**：3D 模型、骨骼和动画用 glTF 2.0；贴图、高度图用 PNG；精灵用 Aseprite 的 sprite sheet 导出（PNG 加 `--data` JSON）；音频用 WAV/OGG；元数据用 JSON。loom **只读导出结果**，不读工具的工程文件（`.aseprite`、`.blend`、`.hip`、`.sbs`、`.flp`、`.fspro`）。
3. **能用文本当源头就用文本**：生成资源的脚本（Python、VEX、Lua、SuperCollider 的 sclang）和文本格式的工程（REAPER 的 `.rpp`）进仓库，资源可以重新生成。导出的二进制资源按 CLAUDE.md 走 Git LFS；很小的测试资源可以直接提交。
4. **loom 的测试不依赖外部工具**：测试用的是提交进仓库的导出结果。重新生成资源的脚本单独放，不进 `verify.ps1`，和 GPU 不进 `cargo test` 是同一个思路。
5. **按需求引进**：每种格式的导入都要等有真实的使用者需要时才做（第 1 节）。各工具的可行性和启用条件见 TODO 的延后项 X.2。

### Aseprite（E4）

aseprite-rs（`B:\Material\Project\claude\ports\aseprite-rs`，ADR-0004）和 loom 的约定：交换格式是原版的 sprite sheet 导出。原版 `aseprite.exe -b x.aseprite --sheet x.png --data x.json` 现在就能生成，以后 aseprite-rs 输出同样的格式。

已知要点（规划者 2026-09-27 实测原版导出）：
- 先只支持 `--format json-array`。`json-hash` 用对象的键表示帧，serde_json 默认按键排序，帧顺序会错（`x 10` 排在 `x 2` 前面）。
- 索引色精灵导出的 sheet 是索引色 PNG（`meta.format` 为 `I8`），读入时要展开成 RGBA。
- 帧时长是毫秒（`duration`，例如 100），不一定是 tick（1/60 s）的整数倍。换算规则要定成整数运算，保证确定性。
- 测试精灵和游戏素材都不用 aseprite 仓库里的样例（受它的 EULA 约束），要自己画。

**美术管线**（2026-09-27，用户决定）：
- 用 aseprite-rs 的 MCP（`ase-mcp`）画，AI 通过它的 `create_sprite`、`run_lua`、`preview`、`export_sheet` 等工具操作。
- **注册方式**：按 aseprite-rs 那边给的方案，用 `claude mcp add aseprite --scope local` 注册在本机的 loom 项目上，**不进仓库**，写在用户的 `~/.claude.json` 里。`--root` 是 loom 的 `assets/art`，所有文件操作都限制在这个目录里。
  - 代价：它直接运行 aseprite-rs 的 `target/release/ase-mcp.exe`。只要有 loom 会话开着它，aseprite-rs 那边就重新编译不了这个 exe（Windows 的文件锁）。遇到时在 loom 会话里用 `/mcp` 断开即可。
- **文件**：
  - `assets/art/<名字>.aseprite` 是源文件（二进制，走 Git LFS）；
  - 导出的 `<名字>.png`（LFS）和 `<名字>.json`（文本）放在同一个目录；
  - 导出用 `json-array`，带上 tag。
- **命名**：每张瓦片或每个精灵用 **tag** 命名，一个 tag 覆盖它的帧。loom 按 tag 名找帧，不依赖帧的顺序和文件名。
- **`assets/art/README.md`** 是二进制源文件的文本说明：每个 tag 画的是什么、调色板、重建源文件用的 Lua、重新导出的参数。改图之前先读它，改完同步更新。
- ase-mcp 会在 root 里建私有目录 `.ase-mcp/`，存 undo 快照，快照的扩展名也是 `.aseprite`。这个目录已经写进 `.gitignore`。
- Aseprite 在 Windows 上导出的 JSON 用 CRLF 换行，git 按 `.gitattributes` 规范成 LF 存进仓库。所以读 sprite sheet 的代码**不能依赖换行符**，也不要拿导出文件和仓库里的文件逐字节比较。
- 导出结果是确定的：同一个源文件导出两次，PNG 和 JSON 逐字节相同（E4.1 实测）。

## 13. 远程控制 `loom serve`（E3 起）

用户决定（2026-09-27）：
- 通道用 **stdin/stdout**，不开网络端口。
- MCP 服务（E3.2）是**独立的适配程序** `loom_mcp`：它启动 `loom serve`，把 MCP 的工具调用翻译成下面的命令。引擎本身不引入 tokio 和 MCP 的 SDK。

E3.1 只做**无头**：`serve` 不创建窗口，时间只在收到 `step` 时推进，所以同样的命令序列得到同样的结果。窗口模式下的 serve 等有真实需要时再做。stdio 接不上用户手动打开的窗口，这是选 stdio 的代价。

### 启动

`loom serve [--scene PATH] [--size WxH]`

- `--scene` 省略时用内置演示场景。
- `--size` 是截图的默认尺寸，默认 1280x720。
- 启动时场景有错，就按普通的命令行错误处理：stdout 输出一份错误 JSON（第 6 节），退出码 1。

### 协议

- **JSON-RPC 2.0，一行一条消息**：UTF-8，以 `\n` 结尾，行尾的 `\r` 忽略。
  - 请求：`{"jsonrpc":"2.0","id":1,"method":"step","params":{"ticks":60}}`。
  - 成功：`{"jsonrpc":"2.0","id":1,"result":{...}}`。
  - 失败：`{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"..."}}`。
- `id` 必须是数字或字符串，回复里原样带回。**不支持通知（没有 `id` 的消息）和批量（数组）**，两者都按「无效请求」回复一条 `id` 为 `null` 的错误。
- `params` 可以省略，省略等于 `{}`。**多余的字段是错误**（`-32602`），和场景文件的 `deny_unknown_fields` 同理：拼错的参数名不能被静默忽略。
- 细则（E3.1 实现时定下）：
  - 顶层有多余的成员（例如把 `params` 拼成 `param`），报 `-32600`。否则 `{"method":"step","param":{"ticks":60}}` 会被静默当成 `step` 1。
  - `-32600` 的回复里，请求有合法的 `id`（数字或字符串）就原样带回；没有 `id`、`id` 类型不对、数组、不是对象时，`id` 为 `null`。
  - `params` 只能省略或者是对象。`"params": null`、数组、数字都报 `-32602`。serde 能把数组按位置解析成结构体，所以这一步要显式拦下。
  - 空行和不是 UTF-8 的行报 `-32700`，会话继续。
  - 参数值为 `null` 等于没给这个参数，例如 `load_scene` 的 `"path": null`。
  - `step` 的 `ticks` 没有上限，给一个很大的数会一直跑下去。
  - 写 stdout 失败（对方关了管道）时会话以错误结束，退出码 1。
- 严格一问一答，按收到的顺序回复。**stdout 只有回复**，一行一条，每条写完立刻 flush。日志照旧写到 stderr。
- 任何一条请求出错，会话都继续，状态不变。只有两种情况结束进程：stdin 读到 EOF（退出码 0，不再输出），或者 `quit`（先回复，再以退出码 0 退出）。

错误码：

| code | 含义 |
|---|---|
| -32700 | 这一行不是合法的 JSON（`id` 为 `null`） |
| -32600 | 是 JSON，但不是合法的请求：不是对象、缺 `jsonrpc: "2.0"`、缺 `method`、没有 `id`、是数组 |
| -32601 | 没有这个方法 |
| -32602 | 参数不对：多余的字段、类型错、动作或轴不在键位表里、轴的值不是有限数、`ticks` 为 0、`path` 和 `source` 同时给出 |
| -32000 | 引擎错误：场景文件读不了或校验失败、写文件失败、GPU 错误。`message` 和命令行的错误文案一致 |

### 方法

| 方法 | params | result |
|---|---|---|
| `describe` | 无 | `{"tick_hz":60,"tick":当前tick,"game":"souls"或"rogue","scene":路径或null,"actions":[...],"axes":[...]}`，动作和轴是**当前游戏**键位表里的，按名字排序（`game` 是 E4.3 加的） |
| `load_scene` | `{"path":"..."}` 或 `{"source":"RON 文本"}`，都不给就是内置演示场景 | `{"tick":0,"game":…,"scene":路径或null}`。成功后从 tick 0 重新开始，清空保持的输入和录制。场景可以换成另一个游戏的，键位表跟着换（E4.3 起）。失败时保留原来的场景和状态（和热重载一样） |
| `reset` | 无 | `{"tick":0}`。当前场景回到 tick 0，清空保持的输入和录制 |
| `input` | `{"actions":{"名字":true/false},"axes":{"名字":数值}}`，两项都可以省略 | 设置**保持的输入**（电平语义，和输入脚本一样），从下一个 tick 起生效，直到再次修改。没提到的动作和轴不变。轴的值按 `canonical_axis_value` 规范化（`-0.0` 变成 `0.0`）。result 是修改后的完整输入：`{"actions":[按下的动作，排序],"axes":{非零的轴}}` |
| `step` | `{"ticks":N}`，N ≥ 1，省略为 1 | 用保持的输入跑 N 个 tick。`{"tick":跑完之后的tick}` |
| `state` | 无 | 状态导出的内容（第 6 节），作为 JSON 值，不是字符串 |
| `screenshot` | `{"path":"...","size":[W,H]}`，`size` 可以省略 | 把**当前状态**渲染成 PNG（自动创建父目录）。`{"path":...,"size":[W,H],"adapter":{...},"render_ms":毫秒}` |
| `record` | `{"path":"..."}` | 把从上次 `load_scene`/`reset`（或启动）以来的输入写成输入脚本，规范形式和 `--record` 相同。`{"path":...,"ticks":当前tick}` |
| `quit` | 无 | `{}`，然后退出 |

### 保证

- **确定性**：模拟不读墙钟。同样的启动参数和同样的请求序列，回复逐字节相同，只有 `render_ms` 和 `adapter` 除外。
- **`state` 和 `--dump-state` 是同一份内容**：字段顺序和每个数值的文本都相同，只是 `state` 用紧凑格式（没有空格和换行）。所以把 `--dump-state` 文件的空白全部去掉，就等于 `state` 回复里 `result` 的原文。
  - 实现上，回复要从带类型的结构体直接序列化，**不能经过 `serde_json::Value`**：`Value` 会把对象的键按字母排序，还会把 f32 转成 f64，写出 `0.2617993950843811` 这样更长的数字，逐位比较就对不上了。
- **能回放**：`record` 写出的脚本，用 `loom run --headless --frames <tick> --input <脚本> [--scene 同一个场景] --dump-state` 回放，得到的状态导出去掉空白后，和回放前 `state` 的 `result` 原文相同。
- **截图和 `loom run` 一致**：同一个状态，`screenshot` 和 `loom run --headless --screenshot` 的 PNG 逐字节相同。
- **渲染器只建一次**：第一次 `screenshot` 时创建，之后反复使用，尺寸变了就 resize。不截图的会话完全不碰 GPU，和第 2 节的无头模式一样。
- `render_ms` 是这一次截图的墙钟耗时（包括第一次创建渲染器），只用于诊断。

### MCP 适配程序 `loom-mcp`（E3.2 起）

用户决定（2026-09-27）：
- 独立的适配程序，引擎不引入 tokio。
- 在仓库根目录用项目级的 `.mcp.json` 注册，这个文件提交进仓库。
- 截图**内联返回图片**，同时照常存文件。

- **crate**：`crates/loom_mcp`，二进制名 `loom-mcp`。
  - 依赖官方 Rust SDK `rmcp`（features：`server`、`macros`、`transport-io`）、`tokio`、`base64`。
  - 它和引擎 crate 互不依赖。它在同一个 workspace 里，所以 `Cargo.lock` 里会有 tokio，但 `loom` 二进制不链接它们。
- **做什么**：
  - 用 MCP 的 stdio 传输对外；
  - 对内启动一个 `loom serve` 子进程，按第 13 节的协议转发请求；
  - 子进程的 stderr 直接继承，日志进 Claude Code 的 MCP 日志。
- **找 `loom`**：默认用 `loom-mcp` 所在目录里的 `loom.exe`，`--loom <PATH>` 可以覆盖。
- **工具**：每个 serve 方法对应一个 MCP 工具，名字和方法相同。
  - `describe`、`load_scene`、`reset`、`input`、`step`、`state`、`screenshot`、`record`。
  - **没有 `quit`**：子进程的生命周期由适配程序管理。
  - 工具的参数和 serve 方法的 params 一一对应；参数的 JSON Schema 由 rmcp 按参数结构体生成。
- **结果**：
  - 文本内容是 serve 回复里 `result` 的**原文**，从回复行里截出来，不经过 `Value` 重新序列化，数值保持逐位。
  - `screenshot` 另加一个 image 内容：读出刚写的 PNG，base64 编码，`image/png`。它的 `path` 可以省略，省略时存到 `out/mcp/shot-<序号>.png`，序号在每次启动时从 1 开始。
- **错误**：serve 回复的错误变成 MCP 工具错误（`isError: true`），文本是 `code` 加 `message`。子进程意外退出时，这次调用报错并说明退出码；下一次调用自动启动一个新会话，结果里注明「新会话，tick 从 0 开始」。
- **并发**：同一时间只转发一个请求。MCP 客户端并发调用时，排队处理。

**`.mcp.json` 和 Windows 的文件锁**：Windows 不允许覆盖正在运行的 exe。如果 MCP 直接运行 `target/debug/` 里的 `loom.exe`，只要有会话开着 MCP，实现者就编译不了。所以：

- `.mcp.json` 运行 `pwsh -NoProfile -File ${CLAUDE_PROJECT_DIR:-.}/scripts/mcp.ps1`。Claude Code 的文档没有保证 MCP 服务的工作目录是项目根目录，但会设置 `CLAUDE_PROJECT_DIR`；这个变量没设置时，退回相对路径（E3.2 实现者查文档发现）。
- 脚本做这几件事：
  1. 切到仓库根目录；
  2. `cargo build -q -p loom_engine -p loom_mcp`，编译输出进 stderr；
  3. 把 `loom.exe` 和 `loom-mcp.exe` 复制到 `target/mcp-run/<时间戳>-<pid>/`；
  4. 从那里运行 `loom-mcp`，stdin 和 stdout 直接交给它；
  5. 顺手删掉 `target/mcp-run/` 下没在用的旧目录。**先单独删目录里的 `loom-mcp.exe`**，删不掉说明那个会话还在，整个目录跳过。原因是会话启动后、第一次调用工具之前，它的 `loom.exe` 还没运行，也没被锁住，直接整个目录递归删除会把它删掉（E3.2 实现者发现）。
- 所以每次启动都用最新代码，而且不会锁住 `target/debug/`。
- 工具里的相对路径（截图、场景、录制）都相对**仓库根目录**。

## 14. 2D：精灵和瓦片地图（E4.2 起）

用户决定（2026-09-27）：
- 2D 和 3D 用**同一棵场景树、同一个渲染器**：相机可以是正交的，节点可以挂 `sprite` 和 `tilemap`。
- 不做独立的 2D 子系统。
- 纯 2D 场景没有游戏逻辑时，用 `loom view` 预览，不经过 `loom run`。有游戏逻辑的 2D 场景（`rogue`，第 15 节）用 `loom run`，按场景顶层的 `game` 选游戏（E4.3 起）。

### 约定

- 2D 画在世界的 **XY 平面**上，+X 向右，+Y 向上；z 决定前后，z 大的在前面。
- 正交相机放在 +Z 一侧，不旋转，就是看向 -Z，也就是俯视这个平面。
- **1 个格子 = 1 个世界单位**。
  - `sprite` 是一个 1×1 的四边形，中心在节点原点，用节点的 `scale` 改大小。帧不是正方形时，要自己用 `scale` 配比例。
  - `tilemap` 的第 `r` 行第 `c` 列（都从 0 数）这个格子，中心在节点局部坐标 `(c, -r, 0)`。**第 0 行在最上面**，和文本里看到的一样。所以格子坐标是整数时，站在格子上的精灵位置也是整数。
- 精灵和瓦片**不受光照影响**，颜色就是贴图的颜色。

### sprite sheet 的读取（`loom_engine`）

- `sheet` 路径和命令行路径一样，相对**当前工作目录**（通常是仓库根目录）。
- JSON 是 Aseprite 的 `json-array` 导出。只读用得到的字段：`frames[].frame`、`meta.image`、`meta.size`、`meta.frameTags[].name/from/to`，其他字段忽略。这是外部格式，所以**不**加 `deny_unknown_fields`，也不依赖换行符。
- PNG 路径是 JSON 所在目录加上 `meta.image`。用 workspace 已有的 `image` crate 解码成 RGBA8，索引色、灰度都会被展开。
- 精灵和瓦片取 tag 的**第一帧**（`from`）。动画还没做（延后项 X.3）。
- **运行中换 tag**（E4.4 起）：游戏用 `Scene::set_sprite_tag(id, tag)` 把精灵换成它在场景里声明过的 tag（`tag` 或 `alternates` 里的一个），没声明过的返回错误。这是模拟状态，所以只能换成加载时检查过的 tag，渲染时不会遇到不存在的 tag。
- 加载场景时就读所有用到的 sheet，同一个路径只读一次，**不论之后会不会渲染**。这样 tag 写错、文件缺失，在无头不截图的运行里也会报错。报错要写出节点路径、sheet 路径和原因，例如 `node map: tilemap: sheet assets/art/tiles.json has no tag "flor"`。检查这几项：
  - JSON 能解析；
  - PNG 存在，并且尺寸等于 `meta.size`；
  - 每一帧都非空（宽高大于 0），并且在图片范围内（非空是 E4.2 实现者加的：着色器里的 `frame.wh - 1` 在 0 时会下溢）；
  - tag 名不重复，`from ≤ to <` 帧数；
  - 场景用到的 tag 都存在，包括精灵的 `alternates`（E4.4 起）。

### 渲染

- 渲染器新增贴图，引擎按 sheet 上传，每个 sheet 一张，格式 `Rgba8UnormSrgb`。
- 帧描述里新增一串精灵绘制项。每项有：
  - 贴图；
  - 世界矩阵（单位四边形，XY 平面，中心在原点）；
  - 帧在贴图里的像素矩形。
- `tilemap` 在引擎里展开成一格一个绘制项。
- **不采样，直接取像素**：片元着色器用四边形的 uv 算出整数像素坐标，`frame.xy + min(floor(uv * frame.wh), frame.wh - 1)`，再用 `textureLoad` 读。不经过采样器，就不会有过滤和相邻帧渗色，天然是最近邻。
- **透明用 alpha 裁剪，不用混合**：alpha < 0.5 的片元丢掉，其余照常写深度。像素画的透明度只有全透明和不透明两种，所以前后关系完全由深度决定，和绘制顺序无关，结果也是确定的。代价是半透明像素不支持。
- 精灵和网格在**同一个 render pass** 里、共用深度缓冲，所以 3D 场景里也能放精灵。
- **像素对齐**：想让每个贴图像素正好落在整数个屏幕像素上，就要选 `ortho.height`，使 `画面高度 / height` 是 16 的整数倍（16×16 的瓦片），并且相机坐标是 1/(每单位像素数) 的整数倍。例如 720 像素高、每格 64 像素（放大 4 倍），`height` 就是 11.25。引擎不强制做到。

### 已知限制

- **`TextureId` 只是渲染器里的下标**，不认是哪个渲染器创建的。拿到别的渲染器上用，下标没越界时会静默画错贴图，越界才报错。现在每个 `Sim`（和它的 `Sheets`）只配一个渲染器，不会发生。要从机制上防住，可以给每个渲染器一个进程内唯一的序号，让 `TextureId` 带上并在 `render()` 时核对（约 10 行）。启用条件：出现一个进程里有多个渲染器、或 sheet 要在渲染器之间共享的需求。
- **贴图不释放**：窗口热重载和 serve 的 `load_scene`、`reset` 每次都会重新上传场景用到的 sheet，旧贴图留在显存里，直到进程结束。现在只有一张 64×16 的 sheet。启用条件：sheet 变大、或出现长时间反复重载的用法时，按 sheet 路径复用贴图或给渲染器加删除接口。

## 15. `rogue`：2D roguelike 示例（E4.3 起）

用户决定（2026-09-27）：
- 简化版的 Cataclysm: Dark Days Ahead，只借鉴玩法（TODO 的 E4 决策）。
- 移动键用小键盘和 vi 键，和 CDDA 默认一样；**按一下走一步**，按住不连走。
- E4.4 起：
  - 僵尸**直线硬追**，被墙挡住就卡住；
  - **比玩家慢**，每 3 回合走 2 步；
  - **固定伤害，不随机**，数值写在场景里；
  - 僵尸死后**留下尸体**，可以踩上去。

E4.3 有地图、玩家、八方向移动、墙和原地等待；E4.4 加上僵尸、撞击攻击、血量、死亡、消息记录。视野、随机地图、物品等以后按兴趣再加。

### 场景要求

- 场景顶层 `game: "rogue"`。
- 根节点 `map`，带 `tilemap`。**`#` 是墙**，挡路；legend 里的其他字符都是能走的地面。
- 根节点 `player`，带 `sprite`。
- **僵尸**（E4.4 起）：名字是 `zombie` 或以 `zombie_` 开头的**根节点**，可以没有，也可以有多个。僵尸的先后顺序就是场景里的先序顺序，每回合按这个顺序行动。
- **只能平移**：`map`、`player` 和每个僵尸都一样，`rotation` 不写或全 0，`scale` 不写或 `(1, 1, 1)`，不能用 `look_at`。
  - 实际检查的是运行时的变换：旋转等于单位四元数、缩放等于 `(1, 1, 1)`。运行时的节点不记录是否用过 `look_at`，所以恰好算出单位旋转的 `look_at`（看向正 -Z）也能通过，效果和不写一样，不算问题（E4.3 审查决定）。
- **所在的格子**（玩家和僵尸都这样算）：`c = x - map.x`，`r = map.y - y`（第 14 节：格子 `(c, r)` 的中心在地图局部的 `(c, -r)`）。
  - 两者都必须是整数，并且在地图范围内（`0 ≤ c < 宽`，`0 ≤ r < 高`）。
  - 格子不能是墙。
  - 两个角色不能站在同一格。
- **前后顺序**：
  - `map.z < 僵尸的 z < player.z`；
  - 所有僵尸的 z 相同。
  - 僵尸死后，尸体的 z 改成 `(map.z + 僵尸的 z) / 2`，所以活着的僵尸、玩家都画在尸体前面。
- **尸体的图**：玩家和每个僵尸的 `sprite` 都要在 `alternates` 里声明 `"corpse"`，死的时候换成它（第 14 节「运行中换 tag」）。
- **数值**写在节点的 `props` 里（第 10 节），都必须是 1 到 1000 之间的整数：
  - 玩家：`hp`、`attack`；
  - 僵尸：`hp`、`attack`、`speed`。
  - 缺了、不是整数、超出范围，或者写了不认识的名字（防拼写错误），都报错。
- 相机是场景里写的，游戏不动它。地图一屏放得下，跟随相机以后需要时再加。
- **报错**（`rogue::GameError`，按「场景无效」报出）：
  - `the scene needs a root node named "map" with a tilemap`
  - `the scene needs a root node named "player" with a sprite`
  - `"<名字>" needs a sprite`（僵尸没有 `sprite`）
  - `"<名字>" must only be translated: no rotation or look_at, scale (1, 1, 1)`
  - `"<名字>" at (x, y) is not on a map cell: x - map.x and map.y - y must be whole numbers inside the WxH map`
  - `"<名字>" starts on a wall at column c, row r`
  - `"<名字>" starts on the same cell as "<名字>"`（后出现的那个在前面，先出现的在后面）
  - `the player must be in front of the map: its z (pz) must be greater than the map's (mz)`（没有僵尸时）
  - `"<名字>" must be between the map and the player: its z (z) must be greater than the map's (mz) and less than the player's (pz)`
  - `"<名字>" must have the same z as "zombie…" (z0), not z`（和第一个僵尸比）
  - `"<名字>": its sprite needs "corpse" in alternates, shown when it dies`
  - `"<名字>": prop <p> is missing`
  - `"<名字>": prop <p> must be a whole number from 1 to 1000, got v`
  - `"<名字>": unknown prop "<p>" (known: hp, attack)` 或 `(known: hp, attack, speed)`
  - 检查顺序：先 `map`，再 `player`，再按先序检查每个僵尸；同一个角色依次查 sprite、只平移、格子、墙、重叠、z、corpse、props。
  - props 内部：先按 `hp`、`attack`、`speed` 的顺序逐个查缺失和范围，最后查不认识的名字。所以把 `hp` 拼成 `hpp` 报的是 `prop hp is missing`，两条都能让人找到拼写错误，不再调整（E4.4 审查决定）。
  - E4.3 的 `the player at …`、`the player starts on a wall …` 两条，在 E4.4 统一改成带名字的写法（`"player" at …`）。

### 键位（`games/rogue/src/bindings.ron`）

| 动作 | 方向 `(dc, dr)` | 小键盘 | vi 键 |
|---|---|---|---|
| `move_n` | (0, -1) | `Numpad8` | `KeyK` |
| `move_ne` | (1, -1) | `Numpad9` | `KeyU` |
| `move_e` | (1, 0) | `Numpad6` | `KeyL` |
| `move_se` | (1, 1) | `Numpad3` | `KeyN` |
| `move_s` | (0, 1) | `Numpad2` | `KeyJ` |
| `move_sw` | (-1, 1) | `Numpad1` | `KeyB` |
| `move_w` | (-1, 0) | `Numpad4` | `KeyH` |
| `move_nw` | (-1, -1) | `Numpad7` | `KeyY` |
| `wait` | 原地 | `Numpad5` | `Period` |

没有轴。行号向下增大，所以「北」是 `dr = -1`。

### 每个 tick

回合只由按键推进，和 tick 数无关（tick 照常以 60 Hz 计数，状态导出的 `tick` 照常增加）。所有运算都是整数，没有随机数。

```text
if player.hp == 0: return                          // 死了：之后的输入全部忽略
action = 上表顺序里第一个 just_pressed 的动作（move_n, move_ne, …, move_nw, wait）
if 没有: return                                    // 按住不连走：只看这个 tick 刚按下的
if action 是移动 (dc, dr):
    (c2, r2) = (c + dc, r + dr)
    if (c2, r2) 上有活着的僵尸 z:                   // 撞击攻击
        z.hp = max(z.hp - player.attack, 0)
        消息 z.hp == 0 ? "You kill <z>." : "You hit <z>."
        if z.hp == 0: z 的精灵换成 "corpse"，z 节点的 z 改成 (map.z + z.z) / 2
    else if (c2, r2) 在地图外，或者是墙:
        return                                      // 撞墙不算一回合，僵尸也不动
    else:
        (c, r) = (c2, r2)；写回 player 节点的位置    // 尸体的格子可以走上去
turn += 1                                           // 移动、攻击、等待都算一回合（实际在分支里先加，见下）
for z in 活着的僵尸（先序）:                         // 本回合刚被打死的不再行动
    z.energy += z.speed
    while z.energy >= 100 and player.hp > 0:
        z.energy -= 100
        zombie_act(z)
```

`zombie_act(z)`：

```text
if max(|z.c - c|, |z.r - r|) == 1:                  // 和玩家相邻（含斜向）：咬
    player.hp = max(player.hp - z.attack, 0)
    消息 "<z> bites you."
    if player.hp == 0: 消息 "You die."；玩家的精灵换成 "corpse"
    return
key(c', r') = (max(|c' - c|, |r' - r|), (c' - c)² + (r' - r)²)   // 先比切比雪夫距离，再比欧氏距离的平方
best = 当前格子，best_key = key(当前格子)
for (dc, dr) in 上表八个方向的顺序:
    n = (z.c + dc, z.r + dr)
    if n 在地图外、是墙、是玩家的格子、有活着的僵尸: continue    // 尸体的格子可以走
    if key(n) < best_key（字典序严格小于）: best = n, best_key = key(n)
z 移到 best（没有更近的格子就不动），写回 z 节点的位置
```

- **消息的回合数**：伪代码为了好读，把 `turn += 1` 写在分支后面；实际上攻击的消息记的是**加过 1 之后**的回合数，和僵尸在同一回合产生的消息相同。也就是说，一旦确定这次按键算一回合（攻击、移动、等待），先 `turn += 1`，再做其他事（E4.4 实现者指出）。
- **撞墙不消耗回合**，和 CDDA 一样。
- **斜着走可以穿过两堵墙的夹角**，玩家和僵尸都可以，和 CDDA 一样。
- **同一个 tick 刚按下好几个方向**时，只取上表顺序里的第一个，其余的丢掉（不排队）。
- **速度**：用 CDDA 的能量制。玩家速度固定是 100，每回合行动一次；僵尸每回合攒 `speed` 点能量，攒够 100 行动一次。`speed: 67` 时，僵尸在第 2、3、5、6、8、9… 回合行动，正好是「每 3 回合走 2 步」。能量从 0 开始，死了就不再增加。
- **僵尸的行动**：和玩家相邻就咬，否则走一步。「走」只看八个邻格里哪个离玩家更近，**不寻路**，所以会被墙卡住（直线硬追）。
- 写回节点位置时，只做整数到 f32 的转换和加减，地图不大时都是精确的。尸体的 z 是一次 f32 加法和一次乘 0.5，也是精确的。

### 状态导出

```json
"game": {
  "turn": 13,
  "player": { "cell": [4, 1], "hp": 9 },
  "zombies": [
    { "name": "zombie", "cell": [4, 2], "hp": 0, "energy": 69 },
    { "name": "zombie_2", "cell": [5, 1], "hp": 3, "energy": 71 }
  ],
  "messages": [
    { "turn": 6, "text": "You hit zombie." },
    { "turn": 6, "text": "zombie bites you." }
  ]
}
```

- `turn`：已经过去的回合数（移动、攻击、原地等待）。
- `player.cell`、`zombies[].cell`：`[c, r]`，列在前。`hp` 为 0 就是死了。
- `zombies`：按先序排列，死了的也留着（`hp` 为 0，格子就是尸体所在的格子）。`name` 是节点名。
- `messages`：开局以来的全部消息，按产生的顺序；`turn` 是产生这条消息的那个回合（已经加过 1 之后的值）。E4.4 不限条数，以后游戏变长了再截断。
- 消息文案固定是英文：`You hit <名字>.`、`You kill <名字>.`、`<名字> bites you.`、`You die.`。
