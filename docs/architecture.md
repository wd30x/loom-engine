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
- 同一块 GPU 上，Dx12 和 Vulkan 的截图**几乎**相同：G3.1 之前逐字节相同；G3.1 加了假人之后，演示场景有 1 个像素的绿色通道差 1（着色器编译的舍入不同）。所以截图基准以 Dx12 为准，smoke 的 H4 只要求两个后端每个通道最多差 1（G3.1 审查决定）。

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
- **图像**：只保证同一台机器、同一驱动、同一后端下逐字节相同。同一块 GPU 上 Vulkan 和 Dx12 的截图每个通道最多差 1（见第 3 节），换 GPU 或换驱动不作保证。PNG 只用于本机回归和人（AI）眼检查。
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
    --trace <PATH>         边跑边把每个 tick 的事件和状态写成 JSONL（E5.1 起，见下文「轨迹」），会自动创建父目录
    --debug-draw[=<类别,…>] 在画面上叠加调试线框（E5.2 起，第 16 节）。不带值是游戏的全部类别，带值只开列出的
    --scene <PATH>         加载场景文件（第 10 节），按它顶层的 game 选游戏；不给时用 souls 内置的演示场景。窗口模式下文件变化会热重载
    --no-capture           窗口模式下不抓鼠标（不锁定、不隐藏指针），也不读鼠标移动（轴只来自按键）。
                           smoke 测试用它：不占用桌面鼠标，有人动鼠标也不会污染录制。和 --headless 同时给是参数错误
    --size <WxH>           渲染分辨率，默认 1280x720；窗口模式下只决定初始窗口尺寸（第 2 节）

loom info                  输出 GPU adapter 信息，不渲染

loom view [OPTIONS]        只加载场景、画一帧，不跑任何游戏逻辑（E4.2 起，第 14 节）。只有无头
    --scene <PATH>         必填（E4.3 起）。内置演示场景的相机要靠游戏逻辑摆位，不跑游戏画出来没有意义
    --screenshot <PATH>    必填
    --size <WxH>           默认 1280x720

loom schema                输出机器可读的格式说明（E5.4 起，第 17 节）：场景文件、每个游戏的 props、状态导出、事件

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
- `debug_draw`（E5.2 起，紧跟在 `game` 后面）：没有 `--debug-draw` 时为 `null`，否则是开着的类别列表（按游戏定义的顺序），是命令行给的值，不管运行中按没按 F3
- `trace`（E5.1 起，紧跟在 `record` 后面）：没有 `--trace` 时为 `null`，否则是 `{"path":…,"lines":写了几行,"events":一共几个事件}`，都只算最后一次热重载之后的部分
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

### 轨迹 `--trace`（E5.1 起）

用户决定（2026-09-27）：
- 每个 tick 写**完整的状态导出**，和 `--dump-state`、serve 的 `state` 是同一份内容。
- 游戏发出的**事件和状态写在同一行**。
- serve 的 `step` 回复直接带事件，另有 `trace` 方法把轨迹写进文件（第 13 节）。

**格式**：JSONL，一行一个 tick，`
` 结尾（不写 `
`）：

```json
{"tick":57,"events":[{"type":"hit","by":"player",…}],"state":{"tick":57,"sim_time":0.95,…}}
```

- 字段顺序固定：`tick`、`events`、`state`。
- `state`：这个 tick 跑完之后的状态导出，紧凑格式（没有空格和换行）。它的原文**逐字节等于**同一时刻 serve `state` 回复的 `result`，也就是 `--dump-state` 文件去掉所有空白之后的内容。
- `events`：这个 tick 里游戏发出的事件，按发生的先后排列；没有就是 `[]`。每个事件是一个 JSON 对象，第一个字段是 `type`，其余字段由游戏定义：`souls` 见第 11 节「事件」，`rogue` 见第 15 节「事件」。
- **第一行是 tick 0**：开始运行时、跑第 1 个 tick 之前的状态，`events` 是 `[]`。所以 `--frames N` 写 N + 1 行。
- 边跑边写（带缓冲），不在内存里攒着；进程正常结束时写完并关闭。写失败按运行时错误报出，退出码 1。
- 窗口模式也能用。**热重载时文件清空，从新的 tick 0 重新写**，和录制、状态导出一样只覆盖最后一段。
- 只看事件：`jq -c 'select(.events != []) | {tick, events}' trace.jsonl`。
- 大小：内置演示场景约 3.7 KB/tick（13 MB/分钟），几乎全是 `nodes`。以后场景变大、轨迹太大时，再考虑只写部分字段或增量格式。

**保证**：
- 同样的输入写出逐字节相同的文件（第 4 节）。
- 第 t 行（从 0 数）的 `state` 等于 `--frames t --dump-state` 去掉空白之后的内容（t ≥ 1）。
- 事件是模拟状态的一部分：同一个 tick 的事件只由这个 tick 之前的状态和输入决定，不读墙钟，顺序固定。事件不进 `--dump-state`。
- 加事件不改变任何状态：打开或不打开 `--trace`，`--dump-state` 逐字节相同。

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
- **`Escape` 不是 `Key`**：它保留给引擎用来退出，不能绑定。`F3` 也不是 `Key`，保留给引擎开关调试绘制（E5.2 起，第 16 节）。
- 鼠标的移动量作为轴的来源，见下文「模拟量（轴）」。手柄以后再加。
- **鼠标按键**（G2.3 起）：`MouseLeft`、`MouseRight`、`MouseMiddle` 也是 `Key` 的变体，和键盘键一样写在键位表的 `keys` 里，也按同样的规则采样（按住、上次采样后按下过、失焦释放）。W3C 没有给鼠标按键起 `code` 名，这三个名字是 loom 自己定的。平台层把 winit 的 `WindowEvent::MouseInput` 翻译过来，其他鼠标按键忽略。抓不抓鼠标、`--no-capture` 都不影响按键；没抓鼠标时，点击窗口让它获得焦点的那一下也会算一次按下。

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
- G3.1 起，**抓之前先把指针移到客户区中央**（`Window::set_cursor_position`，失败只打 `warn`）。Windows 上 winit 0.30 的 `Locked` 是在指针当前的位置做一个 1×1 的 `ClipCursor`，指针原来在窗口外时就被锁在窗口外，第一次点击会落到别的程序上（G2.3 实现者实测发现）。
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
- 节点身上可以挂几种**可选组件**，现在是：`mesh`（加 `color`）、`camera`、`light` 等（见下表）。游戏自己的数值（耐力、锁定、受击判定……）不加组件，写在 `props` 里，由游戏解释（E4.4 起）。
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
| `display_name` | 字符串 | 无 | 给人看的名字（E6.2 起），例如 Boss 血条上的名字。引擎不解释，只要求不是空字符串（报错 `node "<路径>": display_name must not be empty`，和 `loom_scene` 其他节点错误同一种前缀；游戏的报错才用 `the node "<路径>"`）；游戏自己决定能用哪些字符。规范格式里写在 `name` 后面 |
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
| `props` | `[(名字, 数值)]` | 空 | 游戏自定义的数值参数（E4.4 起），例如 `[("hp", 10), ("attack", 1)]`。引擎不解释它们，由游戏读取和校验（`souls` 见第 11 节，`rogue` 见第 15 节）。写在场景里，所以热重载就能调 |
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
- **运行中改颜色**（G3.2 起）：游戏用 `Scene::set_mesh_color(id, color)` 改节点网格的颜色，下一帧就按新颜色画。
  - 节点没有 `mesh` 时返回错误：`the node "<路径>" has no mesh`；
  - 颜色的分量必须有限、在 0 到 1 之间（和场景文件的 `color` 同一条规则），否则返回错误：`the node "<路径>": mesh color components must be between 0 and 1, got [r, g, b]`（glam `Vec3` 的 Display 格式）；
  - 出错时不改任何东西。颜色是模拟状态的一部分（游戏在 tick 里改），但不进状态导出的 `nodes`。
- **运行中隐藏**（G3.3 起）：游戏用 `Scene::set_visible(id, bool)` 隐藏或重新显示一个节点。
  - 隐藏的节点和它的**整个子树**都不画，网格、精灵、瓦片地图都算。`Scene::is_drawn(id)` 返回「自己和所有祖先都可见」，`loom_engine` 收集绘制项时用它过滤。
  - 只影响画不画：变换、组件、碰撞体都照旧。相机、光源节点被隐藏也照常起作用。
  - 场景文件里没有对应的字段，加载时所有节点都可见。和颜色一样，可见性是模拟状态的一部分，但不进状态导出的 `nodes`。
  - 不会失败，没有返回值。

### 规范格式（E5.3 起）

运行中编辑（第 13 节）改的是**场景文件的内容**，`save_scene` 把它写回成文本。写出来的文本叫规范格式，规则固定，由 `loom_scene` 负责：
- `loom_scene` 保留解析出来的文件结构（场景文件的节点描述，不是运行时的节点树），编辑作用在它上面；规范格式就是把它序列化回 RON。
- 用 ron 的 `PrettyConfig`：缩进 4 个空格，开 `IMPLICIT_SOME`（可选字段写成裸值），不写结构体名。
- 顶层字段顺序：`version`、`game`、`camera`、`ambient`、`nodes`。节点字段的顺序同上文的节点字段表：`name`、`position`、`rotation`、`look_at`、`scale`、`mesh`、`color`、`camera`、`sprite`、`tilemap`、`light`、`character`、`orbit_camera`、`props`、`collider`、`children`。
- 没写的可选字段不写；`props`、`children` 为空时不写；`sprite.alternates` 为空时不写。写了的字段原样保留，即使等于默认值（比如 `scale: (1.0, 1.0, 1.0)`）。
- 旋转保留文件里写的欧拉角（度），不换成四元数；`look_at` 也原样保留。
- 数字用 ron 的输出（最短的能精确还原的十进制）；`props` 的值是 f64，其余是 f32。`props` 里写成整数的值写出来带小数点，比如 `("hp", 100)` 变成 `("hp", 100.0)`，解析结果相同。
- **没有注释**：原文件里的注释会丢。所以 `save_scene` 建议另存新文件，要合回原文件时由人或 AI 手工改。
- 保证：
  - `parse(规范格式(文件))` 和 `parse(文件)` 得到的场景完全相同（变换逐位相同）；
  - 规范格式是幂等的：规范格式再解析、再写出，逐字节不变。
  - 仓库里所有场景文件（内置演示场景、两个冻结场景、`assets/scenes/` 下的）都要有测试覆盖这两条。

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
//    G3.1 起前面多了锁定键的处理，锁定时这一步换成相机追目标，见下文「锁定」
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
- `game.lock`（G3.1 起，在 `camera` 后面）见下文「锁定」
- `game.hurtboxes`（G3.2 起，在 `lock` 后面）见下文「命中判定」；每项的 `health`（G3.3 起）见下文「血量、韧性、死亡」
- G3.3 起第 0 步和第 1 步之间多一个第 0b 步（受伤方的计时），见下文「血量、韧性、死亡」
- G3.4 起 `player` 最后多 `hits`、`last_hit`、`health`，`game` 在 `hurtboxes` 后面多 `swingers`；第 5b 步和第 6 步之间多一个第 5c 步（挥砍者），见下文「角色受击和木桩」

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

- 无敌帧在 G3.2、G3.3 只是状态，没有东西会打到角色。G3.4 起木桩会打角色，无敌帧真正生效，见下文「角色受击和木桩」。

### 攻击（G2.3 起）

用户决定（2026-09-27）：
- **只做攻击动作**：前摇、出招、后摇三段，加上耐力消耗；打不到任何东西，命中判定留到 G3（G3.2 起见下文「命中判定」）。
- **鼠标左键轻攻击、右键重攻击**。
- **后摇里可以预输入**：接下一刀，或者接翻滚。轻攻击最多连 3 刀。
- **出招时向前滑一小段**：前摇里还能小幅转向。

**键位**：动作 `attack` 绑 `MouseLeft`，`heavy` 绑 `MouseRight`。按下的那个 tick 触发（`just_pressed`），不像翻滚那样等松开。

**参数**：角色节点的 props，可以省略，规则和上一小节相同。

| prop | 默认 | 说明 |
|---|---|---|
| `attack_cost` | 15 | 轻攻击消耗的耐力，> 0 |
| `attack_startup_ticks`、`attack_active_ticks`、`attack_recovery_ticks` | 10、4、16 | 前摇、出招、后摇的 tick 数，整数 ≥ 1 |
| `attack_distance` | 0.3 | 出招阶段向前滑的总距离，米，> 0 |
| `heavy_cost` | 25 | > 0 |
| `heavy_startup_ticks`、`heavy_active_ticks`、`heavy_recovery_ticks` | 22、5、23 | 整数 ≥ 1 |
| `heavy_distance` | 0.6 | > 0 |
| `attack_turn_speed` | 180 | 前摇里转向的速度，度/秒，> 0；轻重攻击共用 |
| `combo_max` | 3 | 轻攻击连段的最多刀数，整数 ≥ 1 |

- 换算：`attack_step = attack_distance / attack_active_ticks`，`heavy_step` 同理，`attack_turn_step = attack_turn_speed.to_radians() / 60`。
- `attack_step`、`heavy_step` 也必须小于胶囊体半径，报错文案和翻滚的一样，`<action>` 写 `attack` 或 `heavy`。
- 检查顺序：所有 prop 的取值检查是一次遍历同一张表，所以新参数的取值检查接在 G2.1 参数的取值检查后面、无敌帧区间检查前面；每 tick 距离的检查接在 sprint、roll、backstep 后面（G2.3 审查时按实现定下）。

**状态**：
- `action` 多了 `attack`、`heavy` 两个值；`action_tick` 从 1 数到 `startup + active + recovery`。
- 以下三个新字段追加在 `player` 的**最后**：
  - `attack_phase`：`"startup"`、`"active"`、`"recovery"`，不在攻击中时是 `null`；
  - `combo`：当前是轻攻击连段的第几刀（1 到 `combo_max`），其他时候是 0；
  - `buffered`：预输入的下一个动作，`"attack"`、`"heavy"`、`"roll"`，没有时是 `null`。
- 攻击期间 `invulnerable` 是 false。

**每个 tick 的第 4 步**（在上一小节的基础上改）：

```text
if action == roll || action == backstep {
    （同上一小节，不变）
} else if action == attack || action == heavy {
    action_tick += 1
    attack_tick(action)
} else if let Some(next) = next_action() {                 // 见下
    start(next)
} else {
    （冲刺或普通跑，同上一小节，不变）
}

next_action():
    if buffered != null { n = buffered; buffered = null; from_buffer = true }
    else if input.just_pressed("heavy") { n = heavy }
    else if input.just_pressed("attack") { n = attack }
    else if tap { n = roll }                              // roll 再按有没有方向分成翻滚或后撤步
    else { return None }
    if stamina > 0.0 { return Some(n) } else { return None }   // 耐力为 0：这次输入（包括预输入）丢掉，本 tick 普通移动

start(roll):   同上一小节的翻滚或后撤步开始
start(attack 或 heavy):
    combo = (n == attack) ? (from_buffer && 上一个动作是 attack && 上一刀的 combo < combo_max ? 上一刀的 combo + 1 : 1) : 0
    action = n; action_tick = 1
    stamina = max(stamina - cost_of(n), 0.0); stamina_wait = stamina_delay_ticks
    attack_tick(n)

attack_tick(a):                                           // S、A、R 是 a 的前摇、出招、后摇 tick 数
    if action_tick <= S {
        attack_phase = startup
        if v.length_squared() > 0.0 { 按第 5 步的公式朝 dir 转身，但每 tick 最多转 attack_turn_step }
    } else if action_tick <= S + A {
        attack_phase = active
        position = resolve(position + Vec3(-sinf(facing), 0.0, -cosf(facing)) * step_of(a))
    } else {
        attack_phase = recovery
    }
    if attack_phase != startup {                          // 出招和后摇里记下预输入，同一个 tick 取优先的那个
        if input.just_pressed("heavy") { buffered = heavy }
        else if input.just_pressed("attack") { buffered = attack }
        else if tap { buffered = roll }
    }
    if action_tick == S + A + R {
        记下「上一个动作 = a，上一刀的 combo = combo」       // 内部变量，不进状态导出
        action = idle; action_tick = 0; attack_phase = null; combo = 0
    }
```

- 「上一个动作是 attack」：用一个不进状态导出的内部变量记住最近结束的动作和它的 combo。导出的 `combo` 在攻击之外总是 0（G2.3 审查决定）。连段只在**预输入**接上时继续；攻击结束后等一会儿再按，是新的第 1 刀。
- 预输入会被后来的输入**覆盖**（同一次攻击里先按了轻、后按了重，最后接的是重）。前摇里的按键直接丢掉。
- 预输入在攻击结束后的**下一个 tick** 执行，所以最后一个后摇 tick 和下一刀的第 1 个 tick 相邻，中间没有空档。
- 翻滚和后撤步期间的按键仍然丢掉，不预输入（和上一小节相同）。
- 冲刺中按攻击：冲刺停下，出普通攻击（没有冲刺攻击）。
- `busy` 规则不变：攻击期间（包括开始和结束的那个 tick）都算 busy，不恢复耐力。
- 攻击方向：出招阶段沿**当前 facing** 的前方滑动，也就是前摇里转完之后的朝向。
- **没有画面上的耐力条**：要显示得先有 HUD 或文字渲染，还没有使用者。目前在状态导出里看。

### 锁定（G3.1 起）

用户决定（2026-09-27）：
- **鼠标中键和 Q 都能锁定**，按一下锁定，再按一下解锁。
- **锁离视线方向最近的目标**：在锁定距离内，选和相机朝向夹角最小的那个，和黑魂 3 一样。
- **锁定时横甩鼠标换目标**：鼠标在锁定时不再转相机，横向甩过一个阈值，就换到那一侧的下一个目标。
- **黑魂 3 式移动**：
  - 走路时始终面朝目标，可以横移、后退；
  - 翻滚仍朝输入方向；
  - 攻击的前摇自动转向目标；
  - 冲刺时面朝跑的方向；
  - 相机自动转到角色身后，看向目标。

**锁定目标**：带 prop `lock_height` 的节点（第 10 节的 `props`）。锁定点在节点位置上方 `lock_height` 米处，相机对准的是这个点。
- 目标必须是**根节点**，并且不能是相机节点。每个 tick 从场景读它的位置（G3.1 的目标不会动，G4 的敌人会动）。
- 目标的先后顺序是场景的先序。「相等时取先出现的」都按这个顺序。
- 角色节点之外的节点，props 只认 `lock_height`（G3.2 起还有 `hurt_radius`、`hurt_height`，见下文「命中判定」；G3.3 起还有 `hp` 等 5 个，见「血量、韧性、死亡」；G3.4 起还有 `swing_damage` 等 9 个，见「角色受击和木桩」），写了别的名字就报错（防拼写错误）。
- 演示场景里放了三个假人 `dummy_a`、`dummy_b`、`dummy_c`：
  - 根节点带 `lock_height`；
  - 子节点 `body` 是胶囊体网格，子节点 `block` 是挡路的 `collider: Box`（角色不能穿过假人）。

**键位**：动作 `lock` 绑 `MouseMiddle`、`KeyQ`，按下的那个 tick 触发（`just_pressed`）。

**参数**：角色节点的 props，可以省略，规则和「耐力、翻滚、冲刺」相同。

| prop | 默认 | 说明 |
|---|---|---|
| `lock_range` | 15 | 米，> 0。按锁定键和切换目标时，只考虑水平距离不超过它的目标 |
| `lock_break_range` | 20 | 米，> 0，并且 ≥ `lock_range`。锁定中，目标的水平距离超过它就自动解锁 |
| `lock_angle` | 60 | 度，> 0。按锁定键时，目标方向和相机朝向的水平夹角不超过它才能选 |
| `lock_camera_speed` | 360 | 度/秒，> 0。锁定时相机 yaw、pitch 追目标的最大速度 |
| `lock_pitch_down` | 15 | 度，0 ≤ v < 90。锁定时相机比「正对锁定点」再往下压这么多，让目标露在角色头顶上方 |
| `switch_threshold` | 0.1 | 弧度（`look_x` 的单位），> 0。锁定时 `look_x` 连续累计到这么多就换目标 |
| `switch_cooldown_ticks` | 20 | 整数 ≥ 0。换目标（或者甩了但那一侧没有目标）之后，这么多 tick 内不再换 |

- 换算：`lock_angle`、`lock_pitch_down` 转成弧度，`cam_step = lock_camera_speed.to_radians() / 60`。和前面一样，props 是 f64，先 `as f32` 再算。
- 新的报错：
  - `the character "<名字>": prop "lock_pitch_down" must be at least 0 and less than 90, got v`
  - `the character "<名字>": needs lock_range <= lock_break_range`
  - `the node "<路径>": unknown prop "<p>" (known: lock_height)`（G3.2 起括号里是 `known: lock_height, hurt_radius, hurt_height`）
  - `the node "<路径>": prop "lock_height" must be greater than 0, got v`
  - `the lock target "<路径>" must be a root node, and not the camera`
- 检查顺序：
  - 7 个新参数的取值检查，接在 G2.3 参数的后面（同一张表）；
  - `lock_range <= lock_break_range` 接在无敌帧区间的后面、每 tick 距离的前面；
  - 目标节点的检查放在最后：按先序逐个节点，依次查不认识的名字、`lock_height` 的取值、根节点。

**状态**：
- `game.lock`：锁定目标的节点路径，没锁定时是 `null`。放在 `camera` 后面：

```json
"game": { "player": { … }, "camera": { … }, "lock": "dummy_b" }
```

- 不进状态导出的内部状态：
  - `flick`：锁定时 `look_x` 的连续累计量，f32；
  - `switch_wait`：还要等多少 tick 才能再换目标。

下面的伪代码里：
- `bearing(from, to) = atan2f(-(to.x - from.x), -(to.z - from.z))`，也就是从 `from` 看 `to` 的水平方向，和 `facing`、`yaw` 用同一种角度：0 是 -Z，正数往左（从上往下看是逆时针）。
- `hdist(from, to) = Vec2(to.x - from.x, to.z - from.z).length()`。
- `lock_point(t) = t.position + Vec3(0, lock_height, 0)`。
- `position` 在第 1 步里是本 tick 开始时角色的位置，在第 5 步和攻击里是当时的位置。

**第 1 步**换成：

```text
// 1a. 锁定键和自动解锁
if input.just_pressed("lock") {
    lock = (lock == null) ? acquire() : null
    flick = 0.0; switch_wait = 0
} else if lock != null && hdist(position, lock) > lock_break_range {
    lock = null
}

if lock == null {
    （原来的第 1 步：look_x 转 yaw，look_y 转 pitch）
} else {
    // 1b. 横甩换目标（look_y 不起作用）
    if switch_wait > 0 {
        switch_wait -= 1; flick = 0.0
    } else if look_x == 0.0 {
        flick = 0.0
    } else {
        flick += look_x
        if |flick| >= switch_threshold {
            if let Some(t) = switch_target(flick > 0.0 ? right : left) { lock = t }
            flick = 0.0; switch_wait = switch_cooldown_ticks
        }
    }
    // 1c. 相机追目标
    pivot = position + Vec3(0, pivot_height, 0); p = lock_point(lock)
    dh = Vec2(p.x - pivot.x, p.z - pivot.z).length()
    if dh > 0.0 {
        yaw_t   = bearing(pivot, p)
        pitch_t = clamp(atan2f(p.y - pivot.y, dh) - lock_pitch_down, pitch_min, pitch_max)
        yaw     = wrap(yaw + clamp(wrap(yaw_t - yaw), -cam_step, cam_step))
        pitch   = pitch + clamp(pitch_t - pitch, -cam_step, cam_step)
    }
}

acquire():                                    // 离视线方向最近的
    best = null
    for t in 目标（先序）:                     // G3.3 起跳过死掉的目标
        d = hdist(position, t)
        if d == 0.0 || d > lock_range: continue
        a = |wrap(bearing(position, t) - yaw)|    // yaw 是本 tick 开始时的值
        if a > lock_angle: continue
        if best == null || a < best_a: best = t; best_a = a
    return best

switch_target(side):                          // 那一侧离当前目标方向最近的
    cur = bearing(position, lock)
    best = null
    for t in 目标（先序），t != lock:          // G3.3 起跳过死掉的目标
        d = hdist(position, t)
        if d == 0.0 || d > lock_range: continue
        δ = wrap(bearing(position, t) - cur)       // 正数在左边，负数在右边
        right：要求 δ < 0，取 δ 最大的；left：要求 δ > 0，取 δ 最小的；相等时取先出现的
    return best
```

**第 5 步（转身）**，只在「冲刺或普通跑」的分支里执行，改成：

```text
if lock != null && action != sprint {
    if hdist(position, lock) > 0.0 { 按原公式朝 bearing(position, lock) 转，每 tick 最多 max_turn }
} else if v.length_squared() > 0.0 {
    （原来的第 5 步：朝 dir 转）
}
```

**攻击的前摇**（`attack_tick` 的 startup 分支）改成：

```text
if lock != null {
    if hdist(position, lock) > 0.0 { 朝 bearing(position, lock) 转，每 tick 最多 attack_turn_step }
} else if v.length_squared() > 0.0 {
    （原来的：朝 dir 转）
}
```

- **移动方向**照旧相对相机（第 3 步）。锁定时相机在角色身后对着目标，所以「前」就是朝目标走，「左右」就是绕着目标横移。
- **翻滚、后撤步**不变：翻滚朝输入方向，并且立刻转过去；后撤步朝 facing 的反方向（锁定时 facing 通常对着目标，所以是背离目标）。结束后，普通跑的分支会按上面的第 5 步转回去对着目标。
- 锁定键和动作无关，翻滚、攻击期间也能锁定和解锁。
- 解锁的那个 tick，第 1 步已经按「没锁定」走，`look_x`、`look_y` 照常转相机。
- 按锁定键时范围内没有目标：什么也不发生（黑魂 3 这时会把相机转回角色身后，这里没做）。
- 键盘的 `J`、`L` 也是 `look_x`，锁定时按住它们同样会换目标：`0.04` 每 tick，第 3 个 tick 累计到 0.12，达到默认阈值。
- 只用加减乘除、`sqrt`、`abs`、`min`、`max` 和 libm 的 `atan2f`，符合第 4 节。

**已知限制**（G3.1）：
- **不检查视线**：隔着柱子也能锁定，被挡住也不解锁。
- 目标之间方向完全相同（`δ == 0`，一个挡在另一个正后方）时，后面那个换不过去。
- 画面上没有锁定标记（要有 HUD 或者额外的标记节点），目前在状态导出的 `lock` 里看。
- 目标不会死，所以不会因为目标死亡而解锁。G3.3 起会了，见下文「血量、韧性、死亡」。

### 命中判定（G3.2 起）

用户决定（2026-09-27）：
- **攻击判定是挥砍扫过的扇形**：出招的几个 tick 里，判定从右往左依次扫过扇形的一段，轻、重攻击各有自己的半径和角度。
- **受击判定写在节点的 props 里**：`hurt_radius`、`hurt_height`，和 `lock_height` 一样，不改引擎的场景格式。
- **命中后记进状态导出，被打中的东西闪白几帧**，截图看得见。血量、硬直、死亡是 G3.3 的事。
- **一刀对每个受击判定最多命中一次**，一刀可以同时打中几个；**不管墙**，隔着柱子也打得到。

**受击判定**（hurtbox）：带 prop `hurt_radius`、`hurt_height` 的节点。
- 形状是竖直的圆柱：XZ 平面上以节点位置为圆心、半径 `hurt_radius` 的圆，高度范围 `[y, y + hurt_height]`，`y` 是节点位置的 y。和角色碰撞一样，只看「XZ 上的圆加一个高度范围」，不区分胶囊体和圆柱。
- 两个 prop 必须一起写，都 > 0。
- 节点必须是**根节点**，并且不能是相机节点；每个 tick 从场景读它的位置（和锁定目标相同）。角色节点的 props 是另一张表，写这两个名字按「不认识的 prop」报错。
- 受击判定和锁定目标互不依赖，演示场景的假人两样都有。
- 先后顺序是场景的先序。

**参数**：角色节点的 props，可以省略，规则和「耐力、翻滚、冲刺」相同。

| prop | 默认 | 说明 |
|---|---|---|
| `attack_reach` | 2 | 米，> 0。轻攻击扇形的半径，从角色位置（胶囊体底部的中心）量起 |
| `attack_arc` | 120 | 度，0 < v ≤ 360。轻攻击扇形的总角度，以 `facing` 为中线左右对称 |
| `heavy_reach` | 2.4 | 米，> 0 |
| `heavy_arc` | 90 | 度，0 < v ≤ 360 |
| `swing_low`、`swing_high` | 0.3、1.6 | 米，`0 ≤ swing_low < swing_high`。挥砍的高度范围，相对角色的脚；轻重攻击共用 |

- 换算：角度转成弧度；`slice = arc / active_ticks`（`active_ticks` 是这一招的出招 tick 数），加载时各算一次。和前面一样，props 是 f64，先 `as f32` 再算。
- 新的报错：
  - `the character "<名字>": prop "<p>" must be greater than 0 and at most 360, got v`（`attack_arc`、`heavy_arc`）
  - `the character "<名字>": prop "<p>" must be at least 0, got v`（`swing_low`）
  - `the character "<名字>": needs swing_low < swing_high`
  - `the node "<路径>": prop "<p>" must be greater than 0, got v`（`hurt_radius`、`hurt_height`；和 `lock_height` 的报错同一种）
  - `the node "<路径>": needs both hurt_radius and hurt_height, or neither`
  - `the hurtbox "<路径>" must be a root node, and not the camera`
- 检查顺序：
  - 6 个新参数的取值检查接在锁定参数的后面（同一张表）；
  - `swing_low < swing_high` 接在 `lock_range <= lock_break_range` 的后面、每 tick 距离的前面；
  - 节点的检查仍然按先序逐个节点，每个节点依次查：不认识的名字、`lock_height` 的取值、`hurt_radius` 的取值、`hurt_height` 的取值、两个 hurt prop 是否成对、根节点。根节点的检查：带 `lock_height` 的用锁定目标的报错，只带 hurt prop 的用上面 hurtbox 的报错。

**状态**：
- `game.hurtboxes`：每个受击判定一项，按先序。放在 `lock` 后面：

```json
"game": { "player": { … }, "camera": { … }, "lock": null, "hurtboxes": [
  { "node": "dummy_a", "hits": 0, "last_hit": null },
  { "node": "dummy_b", "hits": 2, "last_hit": { "tick": 57, "attack": "attack", "combo": 2 } }
] }
```

- `hits`：加载以来被打中的次数。
- `last_hit`：最近一次命中，没被打中过是 `null`。`tick` 是命中那个 tick 的编号（和状态导出顶层的 `tick` 相同）；`attack` 是 `"attack"` 或 `"heavy"`；`combo` 是那一刀的 `combo`（重攻击是 0）。
- 不进状态导出的内部状态：`struck`，这一刀已经打中过的受击判定。每次开始攻击（包括连段的下一刀）时清空。

**攻击的出招**（`attack_tick` 的 active 分支）在滑动之后加上命中检查：

```text
} else if action_tick <= S + A {
    attack_phase = active
    position = resolve(...)                                   // 不变
    k    = action_tick - S                                    // 出招的第几个 tick，1..A
    half = arc * 0.5
    lo   = -half + (k - 1) as f32 * slice                     // 本 tick 扫过的一段，相对 facing，正数在左
    hi   = -half + k as f32 * slice
    for h in 受击判定（先序），h 不在 struck 里:          // G3.3 起还要跳过死掉的
        if swept(position, facing, reach, lo, hi, h) {
            h.hits += 1; h.last_hit = { tick, attack: a, combo }
            struck 加上 h
        }
}

swept(p, facing, reach, lo, hi, h):
    // 高度：两个闭区间有重叠
    if h.y > p.y + swing_high || p.y + swing_low > h.y + hurt_height { return false }
    // XZ：圆心到扇形 [lo, hi]（半径 reach）的距离不超过 hurt_radius
    d   = Vec2(h.x - p.x, h.z - p.z)
    rel = wrap(bearing(p, h) - facing)
    if lo <= rel && rel <= hi {
        dist = max(d.length() - reach, 0.0)                   // 在角度范围里：只看半径
    } else {
        dist = min(ray(lo), ray(hi))                          // 在外面：到两条边的最近距离
    }
    return dist <= hurt_radius

ray(θ):                                                        // 从 p 出发、长 reach 的一条边
    u = Vec2(-sinf(facing + θ), -cosf(facing + θ))            // 和 forward 同一种写法
    t = clamp(d.dot(u), 0.0, reach)
    return (d - u * t).length()
```

- `arc`、`slice`、`reach` 按这一招（`attack` 或 `heavy`）取。`slice` 让 A 个 tick 首尾相接地扫满整个扇形，最后一段的 `hi` 就是 `half`（浮点上可能差最后一位，不影响规则）。
- 命中检查用滑动**之后**的 `position`。出招期间 `facing` 不变（转向只在前摇里）。
- `d` 为零（圆心正好在角色位置）时，不管 `bearing` 给什么（两个差都是 `-0.0` 时是 `-π`），两种分支算出的 `dist` 都是 0，算命中。
- 同一个 tick 里按先序逐个检查，命中的都记下。
- 命中不改变角色、相机、锁定的任何状态：**不攻击时和攻击时，`player`、`camera`、`lock` 都和 G3.1 逐位相同**。G3.3 起有两处例外：死掉的受击判定不再被打中（检查时直接跳过，`struck` 也不加它）；命中后按下文「血量、韧性、死亡」扣血，打死锁定目标时解锁。
- 只用加减乘除、`sqrt`、`abs`、`min`、`max`、`clamp` 和 libm 的 `sinf`、`cosf`、`atan2f`，符合第 4 节。

**闪白**：第 6 步写完节点之后，对每个受击判定：
- 如果 `last_hit` 不是 `null`，并且 `tick - last_hit.tick < 8`，把它子树（包括自己）里每个带 `mesh` 的节点的颜色设成 `(1, 1, 1)`；
- 否则设回加载时场景里的颜色。
- 闪白的时长（8 tick）和颜色是 `souls` 里的常量，不是 prop。G3.3 起保留闪白，作为命中的即时反馈；硬直另有姿势表现。
- G3.3 起的例外：`hp_bar` 节点不闪；死掉的受击判定不闪而是变暗。见下文「血量、韧性、死亡」的「表现」。
- 命中那个 tick 画出来就已经是白的，一共白 8 个 tick。
- 颜色用 `Scene::set_mesh_color`（第 10 节「渲染」）。节点颜色不在状态导出的 `nodes` 里，要看状态就看 `last_hit.tick`。

**已知限制**（G3.2）：
- **不检查遮挡**（用户决定）：隔着柱子、墙也打得到。
- 每一刀都从右往左扫，连段不左右交替。
- 受击判定是平顶的圆柱，角色的挥砍高度范围是一个水平的带子。
- 只有角色打别人：角色没有受击判定，假人不会还手（G3.4 起有会挥砍的木桩，见下文「角色受击和木桩」）。
- 命中只计数和闪白：没有血量、硬直、击退、顿帧（G3.3 加了血量、硬直、死亡）。

### 血量、韧性、死亡（G3.3 起）

用户决定（2026-09-27）：
- **假人和角色都会受伤**，场景里加一个按固定节奏挥砍的木桩来打角色。拆成两个切片：G3.3 做受击判定一方（假人）的血量、韧性、死亡和血条；G3.4 做角色的受击判定和木桩。
- **韧性，黑魂 3 式**：每招有冲击值，累计超过对方的韧性才硬直；一段时间不挨打，累计就清零。轻攻击要连几下才打出硬直，重攻击一下就行。
- **死了倒下、变暗，过一段时间原地满血复活**，像训练场的木桩，演示场景可以一直反复打。死掉的不能被锁定，也不会再被打中。
- **头顶的方块血条**：受伤方头顶的细长方块，按剩余血量缩短。它在 3D 空间里，不是永远正对屏幕。

**受伤方**：带 prop `hp` 的受击判定。
- 节点 props（非角色节点）在 G3.2 的三个之后再多 5 个：

| prop | 默认 | 说明 |
|---|---|---|
| `hp` | 无 | > 0。满血的血量。写了它，这个受击判定才会受伤；不写就和 G3.2 一样，只计数和闪白 |
| `poise` | 0 | ≥ 0。韧性：累计冲击值**超过**它就硬直。0 表示每次挨打（冲击值 > 0）都硬直 |
| `stagger_ticks` | 30 | 整数 ≥ 1。硬直持续的 tick 数 |
| `poise_reset_ticks` | 60 | 整数 ≥ 1。这么多 tick 不挨打，累计冲击值清零 |
| `revive_ticks` | 180 | 整数 ≥ 1。死后躺多少 tick 复活 |

- 后 4 个必须和 `hp` 一起写，可以省略（用默认值）。
- `hp` 必须和 `hurt_radius`、`hurt_height` 一起写：打不到的东西没有血量可言。
- **血条**：受伤方如果有一个名叫 `hp_bar` 的**直接子节点**，它就是血条，必须带 `mesh`。
  - 游戏按剩余血量改它的局部 `scale.x` 和 `position.x`，左端（局部 -X 端）不动；
  - 场景里写的 `scale.x` 是满血时的长度。
  - 可以没有血条。不是受伤方的节点，名叫 `hp_bar` 的子节点没有特殊含义。

角色的 props 多 4 个：

| prop | 默认 | 说明 |
|---|---|---|
| `attack_damage` | 20 | > 0。轻攻击每次命中扣的血 |
| `heavy_damage` | 35 | > 0 |
| `attack_impact` | 10 | ≥ 0。轻攻击的冲击值 |
| `heavy_impact` | 30 | ≥ 0 |

- 演示场景的假人：`hp` 100、`poise` 25。
  - 轻攻击每下 20 血、冲击 10：第 3 下累计 30，超过 25，硬直；第 5 下打死。
  - 重攻击每下 35 血、冲击 30：一下就硬直，第 3 下打死。
- 新的报错：
  - `the node "<路径>": prop "<p>" must be at least 0, got v`（`poise`）
  - `the node "<路径>": prop "<p>" must be a whole number of at least 1, got v`（三个 `_ticks`）
  - `the node "<路径>": prop "<p>" needs hp`（写了后 4 个之一却没写 `hp`）
  - `the node "<路径>": hp needs hurt_radius and hurt_height`
  - `the node "<路径>": its hp_bar needs a mesh`
  - `unknown prop` 的 `known:` 名单改成 `lock_height, hurt_radius, hurt_height, hp, poise, stagger_ticks, poise_reset_ticks, revive_ticks`（G3.4 起再接上 9 个挥砍参数）。
  - 角色的 4 个新 prop 用已有的文案：`must be greater than 0`（两个 `_damage`）、`must be at least 0`（两个 `_impact`）。
- 检查顺序：
  - 角色的 4 个新参数接在 G3.2 参数的后面（同一张表）。
  - 节点：每个节点依次查：
    1. 不认识的名字；
    2. 8 个 prop 的取值，按上面名单的顺序；
    3. 两个 hurt prop 是否成对；
    4. `hp` 有没有受击判定；
    5. 后 4 个有没有 `hp`，按名单顺序报第一个；
    6. 根节点；
    7. 血条有没有 mesh。

**状态**：受伤方的状态存在它的受击判定里。
- `game.hurtboxes` 的每一项在 `last_hit` 后面多一个 `health`，不是受伤方时是 `null`：

```json
{ "node": "dummy_b", "hits": 3, "last_hit": { … },
  "health": { "hp": 40.0, "state": "stagger", "state_ticks": 30, "impact": 0.0, "impact_wait": 60 } }
```

- `hp`：当前血量，f32，0 到满血。
- `state`：`"ok"`、`"stagger"`（硬直）、`"dead"`。
- `state_ticks`：硬直或死亡还剩多少 tick，`ok` 时是 0。
- `impact`：累计的冲击值，f32。
- `impact_wait`：还剩多少 tick 累计冲击值清零，0 表示没有在等。
- 加载时：满血，`ok`，其余都是 0。

**每个 tick**：
- 新的**第 0b 步**，在第 0 步（翻滚键）之后、第 1 步之前。对每个受伤方（先序）：

```text
if state != ok {
    state_ticks -= 1
    if state_ticks == 0 {
        if state == dead { hp = hp_max }
        state = ok
    }
}
if impact_wait > 0 {
    impact_wait -= 1
    if impact_wait == 0 { impact = 0.0 }
}
```

- **命中**（「命中判定」伪代码里 `h.hits += 1` 那一处）：先按 G3.2 记 `hits`、`last_hit`、`struck`，然后如果 `h` 是受伤方：

```text
hp = max(hp - damage_of(a), 0.0)
if hp == 0.0 {
    state = dead; state_ticks = revive_ticks
    impact = 0.0; impact_wait = 0
    fall = bearing(position, h)                // 审查时加：往哪边倒，见「表现」
    if lock 是 h 的节点 { lock = null }       // 下一个 tick 起相机照常转
} else {
    impact = impact + impact_of(a); impact_wait = poise_reset_ticks
    if impact > poise {
        state = stagger; state_ticks = stagger_ticks
        impact = 0.0
        fall = bearing(position, h)
    }
}
```

- 命中检查跳过 `state == dead` 的受击判定。硬直中照常能被打中：累计冲击值再次超过韧性时，硬直重新计时。
- 计时放在 tick 开头，所以命中那个 tick 算硬直（或死亡）的第 1 个 tick：硬直的状态导出一共出现 `stagger_ticks` 个 tick，第 `stagger_ticks + 1` 个 tick 开头变回 `ok`。复活同理，复活那个 tick 已经是满血的 `ok`。
- 同理，最后一次命中之后，第 `poise_reset_ticks` 个 tick 开头累计冲击值清零。
- **锁定**：`acquire()` 和 `switch_target()` 跳过死掉的目标（目标节点同时是受伤方，并且 `state == dead`）。锁定中的目标被打死时立刻解锁，见上面的伪代码。
- 只用加减乘除、`max` 和比较，符合第 4 节。

**表现**：在「闪白」那一步里一起做，对每个受击判定：
- **颜色**：子树里除了血条以外的 mesh：
  - 死了：场景原色乘以 0.4（变暗）；
  - 没死、在闪白：白色；
  - 否则：原色。
  - 血条永远用它自己的原色。
- **姿势**：**朝远离攻击者的方向倒**（G3.3 审查时改：原来写的是绕节点自己的 X 轴向后仰，但演示场景的假人面朝 -Z，角色从 +Z 过来打，「向后」正好倒向攻击者，死后躺在角色脚下）。
  - `fall`：让受伤方硬直或死掉的那一下，从角色位置看受伤方的水平方向 `bearing(position, h)`（`position` 是命中检查时、滑动之后的角色位置）。硬直中再次被打出硬直时更新。内部状态，不进状态导出，从 `nodes` 里的旋转能看出来。加载时是 0。
  - 受击判定节点的局部旋转设成 `Quat::from_rotation_y(fall) * Quat::from_rotation_x(-tilt) * Quat::from_rotation_y(-fall) * base`，`base` 是加载时的旋转。节点的上方（+Y）倒向 `fall` 那个方向，也就是远离角色的一侧；节点自己的朝向不变。
  - `ok`：直接设成 `base`（不走上面的式子，保证逐位等于加载时的值）；
  - `stagger`：`tilt = 20°`；
  - `dead`：`tilt = 90°`，躺平。
  - 角色和受伤方水平重合（`hdist` 为 0）时，`fall` 直接取 0，往 -Z 倒。不能用 `bearing` 的值：两个差都是 `-0.0`，`atan2f(-0.0, -0.0)` 是 `-π`（G3.3 审查时实现者指出）。
- **血条**：受伤方有血条时，`frac = hp / hp_max`：
  - `scale.x = width * frac`，`position.x = x0 - width * (1 - frac) / 2`，`width`、`x0` 是场景里的 `scale.x`、`position.x`；
  - 死了隐藏血条（`Scene::set_visible`，第 10 节「渲染」），复活时重新显示。死了 `frac` 是 0，缩放成 0 的方块没法正常画，所以要隐藏。
- 变暗系数 0.4、两个角度都是 `souls` 的常量，和闪白一样。
- 姿势和血条改的是节点的局部变换，所以会出现在状态导出的 `nodes` 里。

**演示场景**：三个假人都是受伤方：`hp` 100、`poise` 25，其余用默认值。
- 每个假人加一个血条子节点：`(name: "hp_bar", position: (0.0, 2.1, 0.0), scale: (0.8, 0.08, 0.08), mesh: Cube, color: (0.8, 0.1, 0.1))`。
- 内置场景 `--frames 60` 的截图会变（多了三根血条），要换基准。

**已知限制**（G3.3）：
- **尸体仍然挡路**：碰撞体在加载时算好，假人倒下后，它原来站的地方仍然挡路。
- 姿势只是绕节点原点转，没有动画，状态一变就立刻到位。
- 血条是 3D 空间里的方块，从侧面看会变窄，从正后方看左右是反的。
- 硬直和死亡对假人只是姿势：假人本来就不会动。「硬直中不能行动」见 G3.4 的「角色受击和木桩」。
- `revive_ticks` 小于闪白时长（8）时，复活后可能还白几个 tick（颜色的优先级是变暗、闪白、原色）。演示场景是 180，碰不到；不值得为它加规则（G3.3 审查时定）。
- `hp_bar` 下面如果还有带 mesh 的子节点，它们照常闪白、变暗；只有 `hp_bar` 节点本身不参与（G3.3 实现时定）。
- 没有击退、顿帧。

### 角色受击和木桩（G3.4 起）

用户决定（2026-09-27）：
- **角色死了回出生点复活**：倒地躺一段时间，然后在场景开局的位置满血、满耐力复活，解除锁定。假人和木桩不重置。以后 G5 做篝火，把复活点换成篝火就行。
- **木桩出招有预警**：木桩有一根「手臂」子节点，前摇时转到扇形的起点并变色；出招时手臂沿扇形从右往左扫，和判定一一对应。
- **木桩能打，也能打死**：和假人一样有血量、韧性，硬直和死亡时停止挥砍，恢复后从头开始计节奏。
- **角色挨打的表现和假人一样**：闪白，硬直时歪一下，死了倒地变暗。血量只在状态导出里看，不做血条。

规划者决定：
- 会挥砍的节点叫**挥砍者**（swinger）：带 prop `swing_damage` 的根节点。它只打角色，不打假人，也不打别的挥砍者。
- 角色的受击判定就是它的胶囊体：竖直圆柱，半径和高都取胶囊体的 `radius`、`height`，不另设 prop。
- 角色的血量、韧性沿用受伤方的规则（上一小节），参数写在角色 props 里。

**角色的参数**：角色节点的 props，可以省略，规则和「耐力、翻滚、冲刺」相同，接在 G3.3 的 4 个后面。

| prop | 默认 | 说明 |
|---|---|---|
| `hp` | 100 | > 0。满血的血量 |
| `poise` | 0 | ≥ 0。0 表示冲击值 > 0 的每一下都硬直 |
| `stagger_ticks` | 30 | 整数 ≥ 1 |
| `poise_reset_ticks` | 60 | 整数 ≥ 1 |
| `respawn_ticks` | 120 | 整数 ≥ 1。死后躺多少 tick，回出生点复活 |

- 报错用已有的文案（`must be greater than 0`、`must be at least 0`、`must be a whole number of at least 1`）。
- 角色 props 和节点 props 是两张表，名字相同（`hp`、`poise` 等）不冲突。

**挥砍者**：带 prop `swing_damage` 的节点。节点 props 在 G3.3 的 8 个之后再多 9 个：

| prop | 默认 | 说明 |
|---|---|---|
| `swing_damage` | 无 | > 0。每次命中扣角色的血。写了它，这个节点才会挥砍 |
| `swing_impact` | 20 | ≥ 0。冲击值 |
| `swing_startup_ticks` | 40 | 整数 ≥ 1。前摇 |
| `swing_active_ticks` | 6 | 整数 ≥ 1。出招 |
| `swing_recovery_ticks` | 74 | 整数 ≥ 1。后摇。一轮 `period = startup + active + recovery` 个 tick（加法饱和，不会溢出） |
| `swing_reach` | 2.2 | 米，> 0。扇形半径，从节点位置量起 |
| `swing_arc` | 150 | 度，0 < v ≤ 360。以挥砍者的朝向为中线左右对称 |
| `swing_low` | 0.3 | 米，≥ 0。挥砍的高度范围，相对节点位置的 y |
| `swing_high` | 1.6 | 米，> `swing_low` |

- 后 8 个必须和 `swing_damage` 一起写，可以省略（用默认值）。
- 挥砍者必须是**根节点**，不能是相机节点。
- 挥砍者**不必**是受击判定或受伤方：没有 `hp` 的挥砍者永远按节奏挥砍。演示场景的木桩三样都有（再加 `lock_height`）。
- **朝向**：加载时由节点的旋转算出，公式和角色的初始 `facing` 相同（包括把 `-0.0` 规范成 `0.0`）。挥砍者不转身。
- **手臂**：挥砍者如果有一个名叫 `arm` 的**直接子节点**，它就是手臂，必须带 `mesh`。
  - 加载时记下它的局部 `position`、`rotation`（`p0`、`r0`）和颜色。场景里写的姿势就是「手臂在中线上」的样子（角度 0），演示场景把它写成从节点伸向 -Z 的细长方块。
  - 可以没有手臂。不是挥砍者的节点，名叫 `arm` 的子节点没有特殊含义。
- 换算：角度转成弧度，`slice = arc / active_ticks`。和前面一样，props 是 f64，先 `as f32` 再算。
- 新的报错：
  - `the node "<路径>": prop "<p>" must be greater than 0 and at most 360, got v`（`swing_arc`）
  - `the node "<路径>": prop "<p>" needs swing_damage`（写了后 8 个之一却没写 `swing_damage`）
  - `the node "<路径>": needs swing_low < swing_high`（没写的一方按默认值算）
  - `the swinger "<路径>" must be a root node, and not the camera`
  - `the node "<路径>": its arm needs a mesh`
  - `unknown prop` 的 `known:` 名单在原有 8 个后面接上这 9 个，按上表顺序。
  - `swing_damage`、`swing_reach` 用 `must be greater than 0`，`swing_impact`、`swing_low` 用 `must be at least 0`，三个 `_ticks` 用 `must be a whole number of at least 1`，都是节点的已有文案。`swing_high` 只查 > 0，和 `swing_low` 的大小关系由上面那条查。
- 检查顺序：每个节点依次查：
  1. 不认识的名字；
  2. 17 个 prop 的取值，按名单顺序；
  3. 两个 hurt prop 是否成对；
  4. `hp` 有没有受击判定；
  5. `hp` 的后 4 个有没有 `hp`；
  6. `swing_damage` 的后 8 个有没有 `swing_damage`，按名单顺序报第一个；
  7. `swing_low < swing_high`（只查挥砍者）；
  8. 根节点：同时是几种时，按锁定目标、受击判定、挥砍者的顺序用第一个的报错；
  9. 血条有没有 mesh；
  10. 手臂有没有 mesh。

**状态**：
- `player` 的最后追加三个字段，含义和 `hurtboxes` 的同名字段相同：

```json
"player": { …, "buffered": null, "hits": 1,
  "last_hit": { "tick": 145, "by": "stake" },
  "health": { "hp": 70.0, "state": "stagger", "state_ticks": 30, "impact": 0.0, "impact_wait": 60 } }
```

  - `last_hit.by`：打中角色的挥砍者的路径。
  - `health` 永远有值（角色的参数都有默认值），结构和受伤方的相同。`state_ticks` 在死亡时是离复活还剩的 tick 数。
- `game.swingers`：每个挥砍者一项，按先序，放在 `hurtboxes` 后面：

```json
"swingers": [ { "node": "stake", "swing_tick": 43, "phase": "active" } ]
```

  - `swing_tick`：这一轮的第几个 tick，1 到 `period`；硬直或死亡时是 0。
  - `phase`：`"startup"`、`"active"`、`"recovery"`，`swing_tick` 是 0 时是 `null`。
  - 加载时 `swing_tick` 是 0，所以第 1 个 tick 就进入前摇。
- 不进状态导出的内部状态：挥砍者的 `struck`（这一轮已经打中过角色），角色的 `fall`（同受伤方）、出生点（加载时角色的 `position`、`facing`）。

**每个 tick**：
- **第 0b 步**：受伤方计时之后，角色也按同样的伪代码计时。角色从 `dead` 变回 `ok` 的那个 tick 就是**复活**：

```text
position = 出生点的 position; facing = 出生点的 facing
stamina = stamina_max; stamina_wait = 0
yaw = 场景里的初始 yaw; pitch = 场景里的初始 pitch        // 同「初始状态」；cam_distance 不动，第 6 步照常处理
flick = 0.0; switch_wait = 0
```

  复活的那个 tick 已经是满血的 `ok`，后面的步骤照常执行（能动、能出招）。
- **第 1a 步**：角色死着的时候，锁定键不起作用（`lock` 在死的那一下已经清空，见下）。`look_x`、`look_y` 照常转相机。
- **第 4 步**：角色的 `state` 不是 `ok`（硬直或死亡）时，整个第 4、5 步都不做：不移动、不转身、不出招，按键（包括翻滚的点按）丢掉，不预输入。`busy` 为 false，耐力照常恢复。第 0 步的 `hold` 照常计。
- **第 5c 步（新增）**，在第 5b 步（耐力恢复、算 `invulnerable`）之后、第 6 步之前。对每个挥砍者（先序）：

```text
if 挥砍者是受伤方 && 它的 state != ok {                  // 硬直或死亡：停下，恢复后从头开始
    swing_tick = 0
    continue
}
swing_tick = swing_tick % period + 1
if swing_tick == 1 { struck = false }
S = startup; A = active
if S < swing_tick && swing_tick <= S + A {
    k    = swing_tick - S                                    // 出招的第几个 tick，1..A
    half = arc * 0.5
    lo   = -half + (k - 1) as f32 * slice
    hi   = -half + k as f32 * slice
    if !struck && 角色的 state != dead && !player.invulnerable
       && swept(挥砍者的 position, 挥砍者的 facing, reach, lo, hi, 角色的圆柱) {   // swept 同「命中判定」，高度用挥砍者的 swing_low、swing_high
        struck = true
        player_hit(挥砍者)
    }
}

player_hit(s):
    hits += 1; last_hit = { tick, by: s 的路径 }
    fall = hdist(s, position) > 0.0 ? bearing(s, position) : 0.0     // 远离挥砍者；重合时取 0，同受伤方
    died = 按受伤方的「命中」伪代码扣血、加冲击值（damage、impact 取 s 的）
    if 角色的 state != ok {                                  // 被打出硬直，或者死了：打断当前动作
        action = idle; action_tick = 0; attack_phase = null; combo = 0; buffered = null
        invulnerable = false
        「上一个动作」= (idle, 0)                             // 硬直后不接连段
    }
    if died { lock = null }
```

- 第 5c 步用的是本 tick 第 4 步之后的角色位置和第 5b 步算出的 `invulnerable`，所以状态导出里 `invulnerable` 为 true 的 tick，就是打不中的 tick。
- **无敌帧**：翻滚、后撤步的无敌帧从这里起真正生效。无敌时直接跳过，不算命中、不记 `struck`：这一轮后面的 tick 扫到角色时，照样能打中。
- 硬直中照常能被打中：累计冲击值再次超过韧性时，硬直重新计时（同受伤方）。
- **同一个 tick 互砍**：角色的攻击在第 4 步，挥砍者在第 5c 步。角色这一下把挥砍者打出硬直或打死时，挥砍者这个 tick 就停下，不再挥砍。
- 挥砍者被打出硬直或打死，`swing_tick` 归零，正在进行的这一轮作废；恢复成 `ok` 的那个 tick 从 1 开始新的一轮。
- 只用加减乘除、`sqrt`、`abs`、`min`、`max`、`clamp`、取余和 libm 的 `sinf`、`cosf`、`atan2f`，符合第 4 节。

**表现**：
- **角色**，在第 6 步写角色节点时：
  - 姿势：`ok` 时 `rotation = Quat::from_rotation_y(facing)`，和原来逐位相同；硬直、死亡时 `Quat::from_rotation_y(fall) * Quat::from_rotation_x(-tilt) * Quat::from_rotation_y(-fall) * Quat::from_rotation_y(facing)`，`tilt` 同受伤方（20°、90°）。
  - 颜色：角色子树里每个带 mesh 的节点（胶囊体和面罩），规则同受伤方：死了原色 × 0.4；`tick - last_hit.tick < 8` 时白色；否则原色。
- **手臂**，在第 6 步写完受击判定的表现之后。角度 `θ`：
  - 前摇：`θ = -half`（扇形右边的起点）；
  - 出招的第 k 个 tick：`θ = hi`，也就是这个 tick 扫到的位置；
  - 后摇、`swing_tick` 为 0：回到中线，**直接设成 `p0`、`r0`**（不走下面的式子，保证逐位等于场景里的值）。
  - 不在中线时：`position = Quat::from_rotation_y(θ) * p0`，`rotation = Quat::from_rotation_y(θ) * r0`，也就是绕挥砍者的局部 Y 轴转 `θ`（正数往左）。
  - 颜色：挥砍者是受击判定、并且正在变暗或闪白时，手臂跟着变暗或闪白（它在子树里）；否则前摇时是**预警色** `(1.0, 0.55, 0.1)`；否则原色。
- 预警色是 `souls` 的常量，和闪白一样。

**演示场景**：加一个木桩 `stake`，位置 `(6, 0, 2)`，不旋转（朝 -Z）：
- props：`lock_height` 1.0，`hurt_radius` 0.3，`hurt_height` 1.6，`hp` 100，`poise` 25，挥砍的 9 个都写出来（`swing_damage` 30，其余等于默认值）；
- 子节点：木色的 `post`（`Cube`，位置 `(0, 0.8, 0)`、缩放 `(0.3, 1.6, 0.3)`）、挡路的 `block`（`collider: Box`，同样大小）、`arm`（`Cube`，位置 `(0, 1.1, -1.2)`、缩放 `(0.1, 0.1, 2.0)`，从中心外 0.2 米伸到 2.2 米）、`hp_bar`（同假人，高度 1.9）。
- 角色的 props 写出 5 个新参数，值等于默认值。
- 角色 100 血，木桩每下 30：第 4 下打死。木桩一轮 120 tick（2 秒），前摇 40 tick。
- 木桩在开局相机的视野外，内置场景 `--frames 60` 的截图预期不变。

**已知限制**（G3.4）：
- 木桩不追人、不转身，只打角色。
- 手臂没有动画：状态一变就跳到位，后摇开始时从扇形左端直接回到中线。
- 角色没有血条，也没有耐力条，要看就看状态导出（没有 HUD）。
- 复活不重置世界：假人、木桩的血量和节奏都照旧。**G5.1 起作废**：复活时世界重置，见「篝火」。
- 角色硬直时没有击退，站在原地。
- 木桩死后，手臂跟着木桩倒下，转到了地面以下，画面上看不见（状态导出里它的局部变换仍是中线姿势）。反正死了不挥砍，不值得为它加规则（G3.4 审查时定）。
- `respawn_ticks` 小于闪白时长（8）时，角色复活后可能还白几个 tick，同 G3.3 的 `revive_ticks`。
- 复活不清空挥砍者的 `struck`：`respawn_ticks` 比一轮短时，把角色打死的那一轮不会再打中复活后的角色。符合「一轮最多打中一次」，不改（G3.4 实现者指出）。

### 事件（E5.1 起）

事件进轨迹和 serve 的 `step` 回复（第 6 节「轨迹」、第 13 节）。用户决定（2026-09-27）：
- **状态里看不出来的都要**：命中、无敌帧躲掉的挥砍、被丢掉的按键、被覆盖的预输入。它们要么一闪而过，要么说的是「为什么」，比较前后两个 tick 的状态还原不出来。
- **状态变化只要时间轴上的大事**：动作开始和结束、锁定变化、复活。攻击阶段切换、木桩每一轮的节奏不发事件，从 `action_tick`、`swing_tick` 就能算出来。
- 硬直、死亡不单独发事件，写在 `hit` 的 `result` 里。挥砍者被打出硬直或打死时这一轮作废，也看这个 `result`。

路径都是节点路径（角色节点通常是 `player`）。字段按表里的顺序，`type` 在最前面。

| `type` | 字段 | 什么时候发 |
|---|---|---|
| `hit` | `by`、`target`、`attack`、`combo`、`damage`、`hp`、`result` | 一次命中：角色打中受击判定（「命中判定」），或者挥砍者打中角色（第 5c 步） |
| `dodged` | `by`、`action`、`action_tick` | 挥砍者出招的这一段扫到了角色，但角色在无敌帧里，没算命中 |
| `input_dropped` | `input`、`reason`、`buffered` | 一次按键没有产生任何效果，见下 |
| `buffer_replaced` | `old`、`new` | 已有的预输入被后来的按键覆盖 |
| `action_start` | `action`、`combo`、`buffered` | 角色开始翻滚、后撤步、攻击、重攻击、冲刺 |
| `action_end` | `action`、`reason` | 上面这些动作结束 |
| `lock` | `target`、`reason` | 锁定目标变了（锁上、解开、换目标） |
| `revive` | `node` | 受伤方复活（第 0b 步） |
| `respawn` | `position` | 角色在出生点复活（第 0b 步），`position` 是 `[x, y, z]` |

- `hit`：
  - `by`：角色节点的路径，或者挥砍者的路径。`target`：受击判定的路径，或者角色节点的路径。
  - `attack`：`"attack"`、`"heavy"`，挥砍者是 `"swing"`。`combo`：轻攻击的第几刀，其余是 0。
  - `damage`：这一招的伤害值；`hp`：命中之后目标剩下的血量。目标不是受伤方时两者都是 `null`。
  - `result`：`"hit"`、`"stagger"`（这一下打出了硬直，包括硬直中再次打出）、`"death"`。目标不是受伤方时是 `"hit"`。
- `dodged`：`action` 是 `"roll"` 或 `"backstep"`，`action_tick` 是它的第几个 tick。这一轮后面的 tick 可能再发 `dodged`，也可能打中。
- `input_dropped`：`input` 是 `"attack"`、`"heavy"`、`"roll"`（翻滚键的点按）或 `"lock"`。`reason`：
  - `"no_stamina"`：耐力为 0，要开始的动作被丢掉（`next_action` 的最后一行）。执行预输入时丢掉的，`buffered` 为 true，`input` 是那个预输入。
  - `"outranked"`：同一个 tick 按了优先级更高的键（重攻击 > 轻攻击 > 翻滚），这个被挤掉。开始动作和记预输入时都适用。
  - `"rolling"`：翻滚或后撤步中。
  - `"startup"`：攻击的前摇中。
  - `"staggered"`、`"dead"`：角色硬直或死亡中（锁定键只有 `"dead"`）。
  - `"no_target"`：按锁定键时范围内没有能锁的目标（`input` 是 `"lock"`）。
  - 除了 `no_stamina` 的那种情况，`buffered` 都是 false。
  - 执行预输入的那个 tick 又有新按键：新按键算 `outranked`，被这个预输入挤掉（按键在开始动作之前判定，不按开始后的前摇、翻滚算）。
  - 硬直或死亡清空的预输入**不发**事件：它不是这个 tick 的按键，而且从前一个 tick 的 `buffered` 加上 `action_end`（`interrupted`）看得出来（E5.1 审查时定）。
  - 同时符合几条时，按这个顺序取第一条：`dead`、`staggered`、`rolling`、`startup`、`outranked`、`no_stamina`。所以前摇里同时按轻重攻击，两个都是 `startup`。`outranked` 只用于这个 tick 本来会有一个键生效（开始动作或记下预输入）时被挤掉的那些。
  - 一个 tick 里同一个 `input` 最多发一次。按的是 tick 采样之后的动作（`just_pressed`，翻滚键是点按），两次采样之间的连按只算一次。
- `buffer_replaced`：`old`、`new` 都是 `"attack"`、`"heavy"`、`"roll"`。第一次记下预输入（原来是 `null`）不发事件，看状态里的 `buffered`。
  - 同一个键记两次（`old == new`）也发。这样每一次按键都有着落：生效、记下、被覆盖、被丢掉，四者必居其一（E5.1 审查时定）。
- `action_start`：`action` 是 `"roll"`、`"backstep"`、`"attack"`、`"heavy"`、`"sprint"`；`combo` 同 `hit`；`buffered`：是不是从预输入开始的。
- `action_end`：`reason` 是 `"finished"`（跑完了自己的 tick 数，或者冲刺停下）或 `"interrupted"`（被打出硬直或打死，第 5c 步）。
  - 一个 tick 里先结束、后开始：冲刺中按攻击，先发冲刺的 `action_end`，再发攻击的 `action_start`。
  - 只有 1 个 tick 的动作，同一个 tick 里先发 `action_start` 再发 `action_end`。
- `lock`：`target` 是新目标的路径，解锁时是 `null`。`reason`：
  - `"pressed"`：按锁定键锁上或解开；
  - `"switched"`：横甩换目标；
  - `"out_of_range"`：超过 `lock_break_range` 自动解锁；
  - `"target_died"`：锁定的目标被打死；
  - `"died"`：角色死了。
- **同一个 tick 里的先后**（E5.1 审查时按实现定下）：
  - 按步骤顺序：第 0b 步（`revive`、`respawn`）→ 第 1 步（锁定键的 `lock`、`input_dropped`）→ 第 4 步 → 第 5c 步。
  - 第 4 步里被丢掉的键，按优先级排：重攻击、轻攻击、翻滚。
  - 开始动作时：冲刺的 `action_end` → `action_start` →（只有 1 个 tick 的动作的 `action_end`）→ 被挤掉的 `outranked`。
  - 耐力为 0 时：候选键的 `no_stamina` 在前，被它挤掉的 `outranked` 在后。
  - 命中：`hit` → 被打断的 `action_end`（`interrupted`）→ `lock`（`target_died` 或 `died`）。
- 发事件不改变任何状态，状态导出和 E5.1 之前逐位相同。

### 调试绘制（E5.2 起）

引擎的部分见第 16 节。souls 的类别，按这个顺序：`hurtboxes`、`swings`、`colliders`、`lock`（用户决定四类都要，2026-09-27）。颜色是 `souls` 的常量，线性 RGB。画的都是**判定实际用的形状**：比如假人倒下时网格歪了，受击圆柱仍然竖直地画在节点位置上，因为命中检查用的就是它。

- **`hurtboxes`**：每个受击判定（先序），然后是角色。
  - 形状：`cylinder`。受击判定用节点位置和 `hurt_radius`、`hurt_height`；角色用它的位置和胶囊体的 `radius`、`height`。
  - 颜色，按这个顺序取第一条：
    - 死了：灰 `(0.4, 0.4, 0.4)`；
    - 硬直：黄 `(1.0, 0.9, 0.1)`；
    - 角色无敌（`invulnerable`）：青 `(0.1, 0.9, 1.0)`；
    - 其他（包括不是受伤方的受击判定）：绿 `(0.2, 1.0, 0.2)`。
- **`swings`**：角色的攻击，然后是每个挥砍者（先序）。只画前摇和出招中的，后摇和不在攻击中的不画。
  - 扇形用攻击者的位置、朝向、`reach`、`arc`，在 `y + low` 和 `y + high` 两个高度各画一遍。角色的高度带是 `swing_low`、`swing_high`，挥砍者用自己的。
  - 前摇：整个扇形，橙 `(1.0, 0.55, 0.1)`。角色前摇里还在转向，扇形跟着当前的 `facing`。
  - 出招：整个扇形暗红 `(0.5, 0.1, 0.1)`，再用亮红 `(1.0, 0.15, 0.15)` 画这个 tick 扫的那一段 `[lo, hi]`（和命中检查同一个公式）。
- **`colliders`**：每个碰撞体（先序），按游戏加载时算好的 `BoxCollider` 画 `wire_box`：XZ 上的中心和两个轴、半长，高度范围 `y_min`..`y_max`。浅灰 `(0.8, 0.8, 0.8)`。
  - E5.2 审查时定：浅灰压在浅色的墙、柱子上几乎看不见，**E5.3 顺手改成品红 `(1.0, 0.2, 0.9)`**，调试绘制的截图基准跟着换。
- **`lock`**：
  - 每个锁定目标（先序）的锁定点画 `cross`，大小 0.3：活着的白 `(0.9, 0.9, 0.9)`，死了的灰 `(0.4, 0.4, 0.4)`。
  - 当前锁定的目标：再画一个大小 0.5 的黄 `(1.0, 0.9, 0.1)` `cross`，加一条从相机 pivot（角色位置加 `pivot_height`）到锁定点的黄线。
  - 角色脚下画 `lock_range` 的圆，蓝 `(0.3, 0.5, 1.0)`；锁定中再画 `lock_break_range` 的圆，暗蓝 `(0.2, 0.3, 0.6)`。

### 敌人和 Boss（G4.1 起）

用户决定（2026-09-27）：
- **四种招式**：横扫、下劈、突进、连击。都复用「命中判定」的扇形，前摇有预警。
- **按距离加权随机**：每招有适用的距离范围和权重，在能用的招里按权重随机选。随机数带种子，种子写在场景里，同样的输入永远同样的结果。
- **移动**：离得远就走过来，始终以有限的速度转向玩家（绕到背后有机会）；每招之后有一段休息，是玩家还手的窗口；和角色一样用碰撞，不穿墙。
- **单独的 Boss 场景** `assets/scenes/boss.ron`，内置演示场景不变。

规划者决定：
- 会自己行动的节点叫**敌人**（enemy）：带 prop `enemy_speed` 的根节点。Boss 就是一个敌人，只是招式参数调得大一些。G4.1 只有这一种敌人。
- 敌人的受击、血量、锁定沿用已有的节点 props：它同时是受击判定、受伤方、锁定目标，`hp_bar`、`arm` 子节点的规则同前。敌人的受击圆柱也是它的碰撞圆柱。
- 招式种类写死在 `souls` 里（`sweep`、`cleave`、`lunge`、`combo`），数值都是节点 props，有默认值。

**敌人的 props**：接在节点 props 的后面。除 `enemy_speed` 外都可以省略；写了其中任何一个，都必须写 `enemy_speed`。

| prop | 默认 | 说明 |
|---|---|---|
| `enemy_speed` | 无 | 米/秒，> 0。写了它才是敌人。每 tick 的步长 `enemy_speed / 60` 必须小于 `hurt_radius` |
| `enemy_turn_speed` | 180 | 度/秒，> 0。走路、休息、准备出招时的转向速度 |
| `enemy_face_angle` | 30 | 度，0 < v ≤ 180。朝向和玩家方向的夹角不超过它才出招 |
| `enemy_rest_ticks` | 40 | 整数 ≥ 0。每招结束后休息多少 tick |
| `enemy_aggro_range` | 15 | 米，> 0。玩家的水平距离超过它时，敌人原地待着 |
| `enemy_seed` | 1 | 整数 ≥ 0。随机数种子 |
| `enemy_swing_low`、`enemy_swing_high` | 0.3、2.4 | 米，`0 ≤ low < high`。所有招式共用的挥砍高度范围，相对敌人的脚 |

每种招式 `m`（`sweep`、`cleave`、`lunge`、`combo`）有下面 11 个 prop，名字是 `m_` 加表里的后缀：

| 后缀 | 规则 | `sweep` | `cleave` | `lunge` | `combo` | 说明 |
|---|---|---|---|---|---|---|
| `damage` | > 0 | 25 | 45 | 30 | 20 | 每次命中的伤害 |
| `impact` | ≥ 0 | 20 | 40 | 30 | 15 | 冲击值 |
| `startup_ticks` | 整数 ≥ 1 | 30 | 45 | 35 | 25 | 前摇 |
| `active_ticks` | 整数 ≥ 1 | 8 | 4 | 10 | 5 | 出招（连击是每一刀的） |
| `recovery_ticks` | 整数 ≥ 1 | 30 | 40 | 35 | 45 | 后摇 |
| `reach` | > 0 | 3.0 | 4.0 | 2.5 | 2.8 | 扇形半径 |
| `arc` | 0 < v ≤ 360 | 160 | 20 | 90 | 140 | 扇形角度 |
| `min_range` | ≥ 0 | 0 | 0 | 4 | 0 | 玩家的水平距离在 `[min_range, max_range]` 里才能选这招 |
| `max_range` | > 0 | 3.0 | 4.0 | 9.0 | 2.8 | 必须 ≥ `min_range` |
| `weight` | 整数 ≥ 0 | 3 | 2 | 2 | 2 | 随机选招的权重，0 表示不用这招 |
| `track_speed` | ≥ 0 | 90 | 150 | 180 | 90 | 度/秒，前摇里转向玩家的速度 |

另有 3 个：

| prop | 默认 | 说明 |
|---|---|---|
| `lunge_distance` | 5 | 米，> 0。突进在出招的几个 tick 里匀速向前冲的总距离；每 tick 的步长必须小于 `hurt_radius` |
| `combo_hits` | 3 | 整数 ≥ 1。连击的刀数 |
| `combo_gap_ticks` | 12 | 整数 ≥ 0。连击两刀之间的准备时间（下一刀的前摇） |

- 换算：角度转弧度；`slice = arc / active_ticks`；速度、距离换成每 tick 的步长。和前面一样，先 `as f32` 再算。
- 节点 props 从 17 个变成 72 个（新增 55 个：敌人 8 个、招式 4 × 11 个、另外 3 个；原来写的 70 是算错了，G4.1 实现者指出）。`unknown prop` 的报错不再列出全部名字，改成 `the node "<路径>": unknown prop "<p>" (see loom schema: games.souls.props.node)`（G4.1 起；原来列名单的文案作废）。
- 新的报错：
  - `the node "<路径>": prop "<p>" needs enemy_speed`
  - `the node "<路径>": needs <m>_min_range <= <m>_max_range`
  - `the node "<路径>": needs enemy_swing_low < enemy_swing_high`
  - `the enemy "<路径>" needs hurt_radius, hurt_height, hp and lock_height`
  - `the enemy "<路径>" cannot also be a swinger`
  - `the enemy "<路径>" must be a root node, and not the camera`
  - `the enemy "<路径>": <enemy|lunge> moves d m per tick, which must be less than hurt_radius r`
  - `the enemy "<路径>" must not have a collider in its subtree: colliders are static`
  - `enemy_face_angle` 的取值用新规则的文案 `must be greater than 0 and at most 180`；其余用已有文案。
- 检查顺序：接在原有 10 步之后：
  1. 敌人的 props 有没有 `enemy_speed`；
  2. 四个招式的 `min_range <= max_range`（按 `sweep`、`cleave`、`lunge`、`combo`）；
  3. `enemy_swing_low < enemy_swing_high`；
  4. 敌人要有 hurt、`hp`、`lock_height`；
  5. 不能同时是挥砍者；
  6. 根节点；
  7. 每 tick 步长（先 `enemy`，再 `lunge`）；
  8. 子树里没有碰撞体。

**随机数**：每个敌人一个 SplitMix64，状态是 u64，加载时等于 `enemy_seed`。取一个数：

```text
state = state + 0x9E3779B97F4A7C15（回绕加）
z = state
z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9（回绕乘）
z = (z ^ (z >> 27)) * 0x94D049BB133111EB（回绕乘）
return z ^ (z >> 31)
```

只在选招时取数，每次选招恰好取一个。

**状态**：`game.enemies`，每个敌人一项，按先序，放在 `swingers` 后面：

```json
"enemies": [ { "node": "boss", "state": "attack", "move": "combo", "move_tick": 37, "phase": "active", "hit": 2, "facing": 3.1, "rest": 0, "rng": 1234567 } ]
```

- `state`：`"idle"`（待着或者原地转向）、`"chase"`（走过来）、`"attack"`、`"rest"`、`"stagger"`、`"dead"`。
- `move`：`"sweep"`、`"cleave"`、`"lunge"`、`"combo"`，不在攻击中时是 `null`。
- `move_tick`：这一招的第几个 tick，从 1 数，不在攻击中时是 0。
- `phase`：`"startup"`、`"active"`、`"recovery"` 或 `null`。连击两刀之间的准备算 `startup`。
- `hit`：连击的第几刀（1 到 `combo_hits`），其余是 0。
- `facing`：同角色的 `facing`。
- `rest`：还要休息几个 tick。
- `rng`：随机数的状态（JSON 整数，最大可以到 2^64 - 1）。
- 位置看 `nodes`，血量看 `hurtboxes`。
- 加载时：`idle`，`facing` 由节点的旋转算（同挥砍者），`rng` 等于种子，其余是 0 或 `null`。

**一招的时间线**，`S`、`A`、`R` 是这招的前摇、出招、后摇：
- 横扫、下劈、突进：`move_tick` 1..S 是前摇，S+1..S+A 是出招，之后 R 个 tick 是后摇，一共 `S + A + R`。
- 连击：
  - 先 S 个 tick 前摇，然后第 1 刀出招 A 个 tick；
  - 之后每一刀先准备 `combo_gap_ticks` 个 tick（`phase` 是 `startup`），再出招 A 个 tick；
  - 最后一刀之后是 R 个 tick 后摇。
  - 一共 `S + hits * A + (hits - 1) * gap + R`。
- 前摇（包括连击的准备）里按这招的 `track_speed` 转向玩家；出招、后摇里不转。
- 出招的第 k 个 tick 扫扇形的一段，和「命中判定」用同一个公式，高度用 `enemy_swing_low`、`enemy_swing_high`。
  - 连击的偶数刀方向相反，从左往右扫：`lo`、`hi` 取相反数再互换。
- 突进：出招的每个 tick，先沿 `facing` 前进 `lunge_distance / A`（带碰撞，见下），再做命中检查。
- 每一刀（连击的每一刀分开算）对角色最多命中一次。命中、无敌、死亡的规则和挥砍者完全相同：走同一个 `player_hit`，`hit` 事件的 `attack` 是招式名，`dodged` 照发。

**每个 tick**：第 5c 步先处理敌人（先序），再处理挥砍者。对每个敌人：

```text
d = hdist(敌人, 角色)
if 敌人（作为受伤方）的 state 是 stagger 或 dead:
    state 跟受伤方一样; move = null; move_tick = 0; phase = null; hit = 0; rest = 0
    continue                                     // 招式被打断
if state 原来是 stagger 或 dead: state = idle        // 刚恢复，从 idle 开始
if (角色死了 || d > enemy_aggro_range) && state != attack:
    state = idle; continue                       // 进行中的招照常做完
match state:
  attack: move_tick += 1; 按时间线做这一 tick
          最后一个 tick 做完后：move = null, move_tick = 0, phase = null, hit = 0
          enemy_rest_ticks > 0 时 state = rest, rest = enemy_rest_ticks；否则 state = idle
  rest:   朝玩家转（enemy_turn_speed）; rest -= 1; if rest == 0 { state = idle }
  idle 或 chase:
      候选 = weight > 0 且 min_range <= d <= max_range 的招，按 sweep、cleave、lunge、combo 的顺序
      a = |wrap(bearing(敌人, 角色) - facing)|
      if 候选不为空 && a <= enemy_face_angle:
          total = 候选的权重之和; r = rng.next() % total
          按顺序累加权重，选第一个累加值 > r 的招
          发 enemy_move 事件; state = attack; move = 它; move_tick = 1; 按时间线做第 1 个 tick
      else:
          朝玩家转（enemy_turn_speed）
          if 候选为空: 朝玩家的水平方向走 enemy_speed / 60（带碰撞）; state = chase
          else: state = idle
```

- 转向用 `turn()`，和角色相同。`d == 0` 时不转也不走。
- 敌人被角色打出硬直或打死，发生在第 4 步（角色的攻击）里；第 5c 步看到的已经是新的 state，这个 tick 敌人不再出手。
- 角色死了：进行中的招做完，然后原地待着，直到角色复活。
- 敌人死了照受伤方的规则躺着，按 `revive_ticks` 原地复活。复活后从 `idle` 开始，`rng` 不重置。

**碰撞**：敌人和角色都是竖直圆柱，互相推开。
- 角色的 `resolve`：每一轮先处理碰撞体，再把角色推出每个**活着的**敌人的圆（XZ 平面，两个半径之和 = 胶囊体半径 + `hurt_radius`）。推的方向是从敌人中心指向角色中心；两者重合时沿 +X。
- 敌人移动（走路、突进）时的 `resolve`：碰撞体照旧（把 `hurt_radius`、`hurt_height` 当胶囊体），再推出角色的圆（角色死了也推）。敌人之间不互相推（G4.1 只有一个敌人）。
- 没有敌人的场景，`resolve` 的结果和原来逐位相同。
- 敌人死了以后，角色不再被它推开。这和假人的「尸体仍然挡路」不同：敌人没有碰撞体子节点。

**事件**：新增一种：

| `type` | 字段 | 什么时候发 |
|---|---|---|
| `enemy_move` | `by`、`move`、`distance`、`candidates` | 敌人选定一招的那个 tick，排在这招第 1 个 tick 的其他事件之前。`distance` 是 `d`，`candidates` 是候选招名的列表 |

**表现**：
- 敌人节点的旋转：`ok` 时是 `Quat::from_rotation_y(facing)`；硬直、死亡时按受伤方「姿势」的公式，`base` 换成 `Quat::from_rotation_y(facing)`。
- 手臂（`arm`）：规则同挥砍者。
  - 前摇时在 `-half`（连击的偶数刀在 `+half`）；
  - 出招时在这个 tick 扫到的位置；
  - 其余时候回到中线。
  - 前摇时是预警色。
- 颜色（闪白、变暗）和血条：同受伤方。

**调试绘制**：`hurtboxes` 已经包括敌人（它是受击判定）。`swings` 加上敌人的招式，规则同挥砍者：前摇画橙色的整个扇形，出招画亮红的当前一段。

**Boss 场景** `assets/scenes/boss.ron`（`game: "souls"`）：
- 40×40 的地面，四根柱子；光源、环绕相机同演示场景。
- 角色在 `(0, 0, 8)`，props 写出全部 50 个（值等于默认值，照抄演示场景的注释）。
- `boss` 在 `(0, 0, -4)`，`rotation: (0, 180, 0)`（面朝 +Z，看着角色）：
  - props：
    - `lock_height` 2.0，`hurt_radius` 0.7，`hurt_height` 3.0；
    - `hp` 600，`poise` 80，`stagger_ticks` 40，`revive_ticks` 300；
    - `enemy_speed` 2.5，`enemy_seed` 7；
    - 其余敌人和招式的 props 全部写出，值等于默认值，每个带英文注释。
  - 子节点：
    - `body`：`Capsule(radius: 0.7, height: 3.0)`，深色；
    - `arm`：`Cube`，位置 `(0, 1.8, -1.9)`，缩放 `(0.15, 0.15, 2.8)`；
    - `hp_bar`：位置 `(0, 3.4, 0)`，缩放 `(1.6, 0.1, 0.1)`。

**已知限制**（G4.1）：
- 只有一种敌人；敌人之间不互相推开。
- 下劈的扇形也是水平的，只是很窄、很远，没有竖直方向的判定。
- 敌人不检查视线，也不寻路：隔着柱子也会直线走过来，被柱子挡住就沿着柱子滑。
- 招式没有动画，只有手臂的转动和预警色。
- 敌人和角色互相推开只看 XZ 平面，不看高度。平地场景里没有影响（G4.1 审查时记）。
- 突进的默认步长是每 tick 0.5 米，`hurt_radius` 不超过 0.5 的小敌人必须改小 `lunge_distance` 或加长 `lunge_active_ticks`，否则报步长过大。
- 敌人的 `hit` 事件的 `combo` 恒为 0，连击的第几刀要看状态里的 `hit`。
- 一招的最后一个 tick，状态导出里已经是 `rest`（或 `idle`），`move` 为 `null`。

### 走近（G4.2 起）

G4.1 的问题（1200 tick 观察）：只要有一招能用，敌人就不再走近。下劈的范围最远，所以 Boss 从远处走过来后，停在 4 米处一直下劈。

用户决定（2026-09-27）：
- **「走近」也算一个选项**：选招时，把「朝玩家走一段」和招式放在一起按权重随机选。距离越远，越容易选中它。
- **Boss 的 `poise` 降到 50**：连续两下重攻击，或者五下轻攻击，就能打出硬直。

**新的 props**（敌人的，都可以省略，写了必须有 `enemy_speed`；接在敌人 props 的最后，节点 props 变成 75 个）：

| prop | 默认 | 说明 |
|---|---|---|
| `approach_min_range` | 2 | 米，≥ 0。距离超过它，「走近」才是候选 |
| `approach_weight_per_meter` | 2 | ≥ 0。「走近」的权重是 `floor(approach_weight_per_meter * (d - approach_min_range))`，算出来是 0 就不是候选 |
| `approach_ticks` | 30 | 整数 ≥ 1。选中后最多走多少 tick |

- 默认值下：4 米处，下劈 2、突进 2（范围正好从 4 米开始）、走近 4，走近的概率是 1/2；3.5 米处，下劈 2、走近 3；2.5 米处，横扫 3、下劈 2、连击 2、走近 1。
- 权重的算法：`f32` 乘法和减法，然后 `floorf`（libm），再转成 `u32`。

**选招**（替换「敌人和 Boss」伪代码里 `idle 或 chase` 分支的候选部分）：

```text
候选 = weight > 0 且 min_range <= d <= max_range 的招，按 sweep、cleave、lunge、combo 的顺序
w = floor(approach_weight_per_meter * (d - approach_min_range))，d > approach_min_range 时才算
if w >= 1: 候选最后加上 approach，权重 w
```

- 候选不为空、朝向也对得上时，照原来的方法按权重选。选中招式同原来；选中 `approach`：
  - 发 `enemy_move` 事件（`move` 是 `"approach"`，`candidates` 里也有 `"approach"`）；
  - `state = chase`，`chase = approach_ticks`；
  - 这个 tick 就按下面的 `chase` 规则走第一步。
- 候选**只有** `approach`，也要朝向对得上才选；朝向对不上时照原来原地转（`idle`）。
- 候选为空（离得比所有招都远，并且走近的权重也是 0）：照原来一边转一边走，`state = chase`，`chase` 保持 0。

**`chase` 状态**：

```text
if chase > 0:                                     // 选中了「走近」
    朝玩家转（enemy_turn_speed）; 朝玩家走 enemy_speed / 60（带碰撞）
    chase -= 1
    if chase == 0 || d_走完之后 <= approach_min_range: chase = 0; state = idle
else:                                             // 候选为空时的走路，同 G4.1
    按 idle 的规则重新判断（每个 tick 都选一次）
```

- `d_走完之后` 是这一步走完、推开之后的水平距离。
- 走近途中被打出硬直、打死，或者角色跑出仇恨范围、角色死了：`chase = 0`，按原来的规则处理。

**状态导出**：`enemies` 的每项在 `rest` 后面多一个 `chase`（走近还剩多少 tick，不在走近时是 0）。

**G4.1 留下的两处小问题，一起改**：
- **离开 `rest` 就清零**：不管从哪条路径离开 `rest` 状态（休息完、角色死了、超出仇恨范围、被打出硬直），`rest` 都设成 0。`chase` 同理：离开 `chase` 状态就是 0。导出里不会再出现 `state` 是 `idle`、`rest` 却是 40 这种过期的值。
- **删掉 `the enemy "<路径>" must be a root node, and not the camera`**：敌人必须有 `lock_height`，而锁定目标的根节点检查排在前面，所以这条永远走不到。检查顺序里去掉「根节点」这一步。

**Boss 场景**：`poise` 改成 50，三个新 prop 写出来（值等于默认值，带英文注释）。

### 二阶段（G4.3 起）

用户决定（2026-09-27）：
- **血量降到一半进入二阶段**，阈值是 prop。
- **咆哮加冲击波**：打断当前动作，原地咆哮一段时间（无敌、变色）；咆哮开始后不久，对身边一圈造成伤害和冲击，逼玩家拉开或者翻滚。
- **二阶段变强**：
  - 更快：前摇和休息缩短，走路和转向加快；
  - 连击多一刀；
  - 新招「回旋斩」：原地转一圈的 360° 横扫，只在二阶段出现；
  - 外观变化：身体变成暗红色。
- 另外（G4.2 审查时用户定）：走近的权重加上限 `approach_weight_max`，默认 4。这样远处也有大约 1/3 的机会出突进。

**新的 props**（敌人的，都可以省略，写了必须有 `enemy_speed`；接在敌人 props 的最后）：

| prop | 默认 | 说明 |
|---|---|---|
| `approach_weight_max` | 4 | 整数 ≥ 0。「走近」权重的上限：`w = min(floor(…), approach_weight_max)` |
| `phase2_at` | 无 | 0 < v < 1。血量占满血的比例降到不超过它时进入二阶段。不写就没有二阶段 |
| `phase2_speed_scale` | 0.75 | 0 < v ≤ 1。二阶段的时间系数，见下 |
| `phase2_combo_hits` | 4 | 整数 ≥ 1。二阶段连击的刀数 |
| `roar_ticks` | 70 | 整数 ≥ 1。咆哮一共多少 tick |
| `roar_blast_tick` | 20 | 整数 ≥ 1，≤ `roar_ticks`。咆哮的第几个 tick 放出冲击波 |
| `roar_damage` | 30 | > 0 |
| `roar_impact` | 40 | ≥ 0 |
| `roar_reach` | 4 | 米，> 0。冲击波的半径 |

- `phase2_speed_scale` 等 7 个 phase2、roar 开头的 prop 只在写了 `phase2_at` 时才有意义；没写 `phase2_at` 却写了它们，报 `the node "<路径>": prop "<p>" needs phase2_at`。
- 新招 `spin` 有和其他招式一样的 11 个 prop（`spin_damage` 等），默认值：

| 后缀 | `spin` |
|---|---|
| `damage` | 40 |
| `impact` | 40 |
| `startup_ticks` | 50 |
| `active_ticks` | 12 |
| `recovery_ticks` | 40 |
| `reach` | 3.5 |
| `arc` | 360 |
| `min_range` | 0 |
| `max_range` | 3.5 |
| `weight` | 2 |
| `track_speed` | 0 |

  - `spin` 的 prop 不需要 `phase2_at` 也能写（没有二阶段时它们没有作用，但检查照常）。
- 节点 props 增加 20 个（`approach_weight_max`、8 个 phase2 和 roar、11 个 spin），变成 95 个。
- 新的报错：
  - `the node "<路径>": prop "<p>" needs phase2_at`；
  - `the node "<路径>": needs roar_blast_tick <= roar_ticks`；
  - `spin_min_range <= spin_max_range` 用已有的那条文案；
  - 二阶段的走路步长（`enemy_speed / phase2_speed_scale / 60`）也必须小于 `hurt_radius`，报错里 `<enemy|lunge>` 写 `enemy in phase 2`；
  - `phase2_at` 的取值用新规则 `must be greater than 0 and less than 1`（schema 里的规则名是 `between`，两端都不含）。
  - 二阶段步长只在写了 `phase2_at` 时检查：没有二阶段的敌人不拿默认的 0.75 去算（G4.3 实现时定）。
  - 二阶段的步长、转向、追踪由一阶段的每 tick 值除以 `s` 得到（不是从 prop 原值重新算），报错里的步长也是这个值（G4.3 实现时定）。
- 检查顺序：`needs phase2_at` 接在 `needs enemy_speed` 后面；`roar_blast_tick <= roar_ticks` 接在四个招式（加上 `spin`，排在最后）的范围检查后面；二阶段步长接在原来的步长检查后面。

**二阶段的参数**：加载时算好一份二阶段用的参数，`s = phase2_speed_scale`：
- 所有招式（包括 `spin`）的 `startup_ticks`：`max(1, round(t * s))`；
- `combo_gap_ticks`、`enemy_rest_ticks`：`max(0, round(t * s))`；
- `round(x)` 是 `floorf(x + 0.5)`，`t * s` 用 f32 算；
- `enemy_speed`、`enemy_turn_speed`、每招的 `track_speed`：除以 `s`；
- 连击的刀数：`phase2_combo_hits`；
- 出招、后摇的 tick 数，伤害、冲击、范围、权重：不变；
- 二阶段的候选多一个 `spin`，排在 `combo` 之后、`approach` 之前。一阶段没有 `spin`。

**进入二阶段**：在「命中」里（角色打中敌人、`health.take` 之后）：

```text
if 敌人有 phase2_at && stage == 1 && hp > 0 && hp <= phase2_at * hp_max:
    stage = 2
    health: state = ok, state_ticks = 0, impact = 0, impact_wait = 0   // 这一下即使打出了硬直也作废，改成咆哮
    敌人: state = roar, move = null, move_tick = 0, phase = null, hit = 0, rest = 0, chase = 0
    发 stage 事件（紧跟在这一下的 hit 事件之后）
```

- 这一下打死了（`hp == 0`）就直接死，不进入二阶段。
- `hit` 事件的 `result` 仍然按 `take` 的结果写（可能是 `stagger`）；看 `stage` 事件就知道它被咆哮取代了。
- 复活时回到一阶段：`stage = 1`。

**咆哮**（第 5c 步，`state == roar`）：

```text
move_tick += 1
朝玩家转（二阶段的 enemy_turn_speed）
if move_tick == roar_blast_tick:
    冲击波：以敌人位置为圆心、半径 roar_reach 的整圈（lo = -π, hi = π），高度用 enemy_swing_low、enemy_swing_high
    命中、无敌、死亡的规则同其他招式（player_hit，attack 是 "roar"；无敌时发 dodged）
if move_tick == roar_ticks: state = idle; move_tick = 0
```

- 进入二阶段的那个 tick，第 5c 步已经看到 `state == roar`，`move_tick` 从 0 加到 1，所以这个 tick 就是咆哮的第 1 个 tick。
- **咆哮期间无敌**：角色的攻击扫到它时直接跳过，不计 `hits`、不扣血、不闪白、不加 `struck`，也不发事件。咆哮结束后，这一刀后面的 tick 扫到它时照常能打中。
- 咆哮期间不走、不选招；角色死了、跑出仇恨范围也照样咆哮完。
- 状态导出里 `phase`：`move_tick < roar_blast_tick` 时是 `startup`，等于时是 `active`，之后是 `recovery`。

**状态导出**：
- `enemies` 的每项在 `node` 后面多一个 `stage`：1 或 2。
- `state` 多一个取值 `"roar"`。
- `move` 多一个取值 `"spin"`。

**事件**：
- 新增 `stage`：字段 `by`、`stage`（2），在进入二阶段时发。
- `hit` 的 `attack` 多了 `"spin"`、`"roar"`。
- `enemy_move` 的 `move`、`candidates` 多了 `"spin"`。

**表现**：
- 二阶段时，敌人子树里的 mesh（不包括 `hp_bar`、`arm`）用场景原色乘以暗红系数 `(1.0, 0.3, 0.3)`（每个分量分别相乘）。
- 颜色的优先级：死了变暗 > 闪白 > 咆哮到冲击波之前（`move_tick < roar_blast_tick`）用预警色 `(1.0, 0.55, 0.1)` > 二阶段暗红 > 原色。
- `spin` 的手臂：规则同其他招式。360° 的扇形分 `active_ticks` 段扫一整圈，手臂跟着转一圈。
- 调试绘制：`swings` 画咆哮的冲击波，规则同招式（冲击波之前是橙色整圈，冲击波那个 tick 是亮红整圈）；`spin` 同其他招式。
- 暗红系数是 `souls` 的常量。
- 手臂不跟身体变橙、变暗红，保持自己的原色（闪白、变暗照常）（G4.3 实现时定）。

**Boss 场景**：
- 写上 `phase2_at` 0.5；
- 写出其余 19 个新 prop，值等于默认值，带英文注释。

**已知限制**（G4.3）：
- 只有两个阶段。
- 咆哮没有击退：冲击波只造成伤害和冲击，角色站在原地硬直。
- 咆哮期间的无敌是整个敌人无敌，不区分部位。

### 篝火（G5.1 起）

用户决定（2026-09-27）：
- **黑魂式休息**：
  - 角色满血、满耐力；
  - 这个篝火成为复活点，取代出生点；
  - 所有敌人、假人、木桩回到开局的位置和状态（Boss 回满血、回一阶段）。
  - **角色死后复活时，世界也一样重置**。这取代了 G3.4 的「复活不重置世界」。
- **E 键坐下**：
  - 靠近篝火按 E 坐下，坐下的那一刻生效；
  - 坐着时不能动，按移动键或 E 站起来；
  - 敌人在附近战斗中（在它的仇恨范围内）时不能坐。
- 原素瓶是下一项（G5.2），篝火先不管它。
- 另外（G4.3 审查时用户定）：**回旋斩改成从身前开始扫**。

**篝火**：带 prop `bonfire_range` 的节点。
- `bonfire_range`：米，> 0，没有默认值。角色的水平距离不超过它才能坐下。
- 必须是根节点，不能是相机节点；报错 `the bonfire "<路径>" must be a root node, and not the camera`。
- 不能同时是锁定目标、受击判定、挥砍者、敌人；报错 `the bonfire "<路径>" cannot also be a lock target, hurtbox, swinger or enemy`。
- 可以有碰撞体子节点（它是静态的）。
- 节点 props 多 1 个，变成 96 个。检查接在敌人的检查之后：先查根节点，再查身份冲突。

**键位**：新动作 `interact` 绑 `KeyE`，按下的那个 tick 触发（`just_pressed`）。

**坐下**（第 4 步开头，在所有动作之前）：

```text
if input.just_pressed("interact"):
    if action == sit: 站起来（见下）
    else if 角色死了: 丢掉（dead）
    else if 硬直: 丢掉（staggered）
    else if action 不是 idle 或 sprint: 丢掉（busy）
    else if 没有篝火的水平距离 <= bonfire_range: 丢掉（no_bonfire）
    else if 有活着的敌人，它到角色的水平距离 <= 它的 enemy_aggro_range: 丢掉（enemy_near）
    else: 坐下（先序里第一个够得着的篝火）
```

- **坐下**：
  1. `action = sit`，`action_tick = 0`；冲刺停下（发冲刺的 `action_end`）；
  2. 角色 `hp` 回满，`health` 回到加载时的状态（`ok`、冲击值清零）；
  3. `stamina = stamina_max`，`stamina_wait = 0`；
  4. 复活点设成**角色现在的位置和 `facing`**：它站着的地方一定是能站的，所以不用篝火的位置；
  5. `lock = null`（发 `lock` 事件，`reason` 是新的 `"rested"`）；
  6. **世界重置**，见下；
  7. 发 `rest` 事件（`bonfire` 是篝火的路径）。
- **坐着**（`action == sit`）：
  - 第 4、5 步不移动、不转身；
  - 攻击、翻滚、锁定键丢掉，`reason` 是新的 `"sitting"`；
  - 推方向（`v` 不为零）或者再按 E 就站起来：`action = idle`，发 `stand` 事件；站起来的这个 tick 不移动；
  - 耐力照常恢复（不 busy）；
  - 相机照常能转；
  - 坐着时被打出硬直或打死，照常打断（`action_end` 的 `reason` 是 `interrupted`）。
- `sit` 的 `action_start`、`action_end` 不发，用 `rest`、`stand` 事件表示。例外：坐着时被打出硬直或打死，照常发 `action_end`（`action` 是 `sit`，`reason` 是 `interrupted`）（G5.1 实现时定）。
- 坐下的那个 tick 即使推着方向也不站起来，从下一个 tick 起才判断，免得边走边按 E 在同一个 tick 坐下又站起。坐下和站起的 tick，同时按的攻击、翻滚都按 `sitting` 丢掉（G5.1 实现时定）。

**世界重置**（坐下时，和角色复活时，都做同样的事）：
- 每个受击判定：`health` 回到加载时的状态；`fall` 回到 0。`hits`、`last_hit` 不清（它们是「加载以来」的计数）。
- 每个挥砍者：`swing_tick = 0`，`struck = false`。
- 每个敌人：
  - 回到加载时的全部状态：位置、`facing`、`state = idle`、`stage = 1`；
  - `rng` 回到种子；
  - 招式、休息、走近的计数清零；
  - 节点的位置、旋转写回加载时的值。
- 受击判定节点的姿势、颜色、血条，在这个 tick 的第 6 步按新的状态照常写。
- 事件：重置本身不单独发事件，`rest` 和 `respawn` 事件就表示发生了重置。
- 角色复活（第 0b 步）：位置和 `facing` 用复活点，其余照 G3.4；然后做世界重置。**G3.4 已知限制里「复活不重置世界」作废。**

**表现**：
- 坐着时，角色节点的位置比 `position` 低 0.5 米（`y - 0.5`），看起来像坐下了；`position` 本身不变。
- 篝火的外观由场景决定，游戏不改它。

**状态导出**：
- `action` 多一个取值 `"sit"`。
- `game` 在 `enemies` 后面加 `respawn`：`{ "position": [x, y, z], "facing": f, "bonfire": 路径或 null }`。加载时是出生点，`bonfire` 是 `null`。

**事件**：
- 新增 `rest`（字段 `bonfire`）、`stand`（没有字段）。
- `input_dropped` 的 `input` 多了 `"interact"`，`reason` 多了 `"busy"`、`"no_bonfire"`、`"enemy_near"`、`"sitting"`。
- `lock` 的 `reason` 多了 `"rested"`。
- `input_dropped` 的原因优先级：`sitting` 排在最前面；`interact` 的判定顺序按上面的伪代码。

**调试绘制**：新类别 `bonfires`，排在最后。每个篝火在脚下画 `bonfire_range` 的圆，暖橙 `(1.0, 0.6, 0.2)`；当前复活点所在的篝火再画一个大小 0.5 的同色 `cross`，位置在篝火上方 1 米。

**场景**：
- 演示场景加一个篝火 `bonfire`，位置 `(-6, 0, 6)`：
  - props `bonfire_range` 1.5；
  - 子节点 `fire`（`Cube`，位置 `(0, 0.25, 0)`，缩放 `(0.5, 0.5, 0.5)`，颜色 `(1.0, 0.45, 0.1)`）；
  - 子节点 `block`（`collider: Box`，同样大小）。
- Boss 场景同样加一个，位置 `(4, 0, 12)`。它到 Boss 的距离超过 Boss 的仇恨范围。

**回旋斩从身前开始扫**（修改「二阶段」）：
- `spin` 的扇形以 `facing + π` 为中线计算：`-half`（`-π`）就是正前方。所以出招的第 1 段从正前方开始，向左扫一整圈回到正前方。
- 手臂、调试绘制用同一个中线。

**已知限制**（G5.1）：
- 坐下、站起没有动画，只是节点下沉 0.5 米。
- 篝火没有「点燃」的概念，第一次靠近就能坐。
- 世界重置不重置角色的 `hits`、受击判定的 `hits`、`last_hit`。
- Boss 追着的时候基本坐不了篝火：要离开它的仇恨范围（15 米），Boss 走得不比角色慢多少，冲刺又有耐力限制。这和黑魂「附近有敌人不能休息」一致，Boss 场景的篝火主要用于开打前和死后（G5.1 审查时确认）。

### 原素瓶（G5.2 起）

用户决定（2026-09-27）：
- **3 瓶，每瓶回满血的 40%**。坐篝火补满，死后复活也补满。
- **喝的时候能慢慢走**：不能翻滚、攻击、冲刺，可以边退边喝。
- **动作中段回血**：按下后先举瓶一段时间，之后才回血；举瓶期间被打出硬直就打断，瓶子已经用掉，血没回。

**参数**：角色 props，可以省略，接在 G3.4 的 5 个之后（角色 props 变成 55 个）：

| prop | 默认 | 说明 |
|---|---|---|
| `flask_charges` | 3 | 整数 ≥ 0。满的时候有几瓶 |
| `flask_heal` | 0.4 | 0 < v ≤ 1。每瓶回满血的多少 |
| `flask_ticks` | 60 | 整数 ≥ 1。喝一瓶的动作一共多少 tick |
| `flask_heal_tick` | 30 | 整数 ≥ 1，≤ `flask_ticks`。第几个 tick 回血 |
| `flask_walk_speed` | 1 | 米/秒，> 0。喝的时候走路的速度；每 tick 的步长必须小于胶囊体半径 |

- 新的报错：
  - `the character "<名字>": needs flask_heal_tick <= flask_ticks`；
  - 步长用已有的文案，`<action>` 写 `drink`；
  - `flask_heal` 的取值用 `positive_at_most`（上限 1）的文案。
- 检查顺序：取值检查照表的顺序；`flask_heal_tick <= flask_ticks` 接在 `swing_low < swing_high` 后面；步长接在已有的步长检查后面。

**键位**：新动作 `drink` 绑 `KeyR`，按下的那个 tick 触发（`just_pressed`）。

**喝**：`drink` 是和攻击、翻滚一样的动作，`action` 多一个取值 `"drink"`。
- **优先级**：`next_action()` 里，同一个 tick 按了几个键时，顺序是重攻击 > 轻攻击 > 喝 > 翻滚。
- **能预输入**：攻击的出招、后摇里按 R，记成预输入 `"drink"`；`Buffered` 多一个取值。
- **不要耐力**：`next_action()` 最后的「耐力为 0 就丢掉」对 `drink` 不适用。
- **没瓶了**：`flask == 0` 时按 R，丢掉，`input_dropped` 的 `reason` 是新的 `"empty"`。
- **开始**（`action_tick = 1`）：`flask -= 1`，发 `action_start`。冲刺中按 R：冲刺停下，开始喝。
- **每个 tick**（`action == drink`）：
  - `action_tick` 从 1 数到 `flask_ticks`；
  - 移动：和普通跑一样用 `dir`（相对相机），但步长是 `flask_walk_speed / 60`，带碰撞；
  - 转身：同普通跑的第 5 步（锁定时面朝目标）；
  - `action_tick == flask_heal_tick` 时回血：`hp = min(hp + flask_heal * hp_max, hp_max)`，发 `heal` 事件；
  - 最后一个 tick 结束（`action = idle`，发 `action_end`，`reason` 是 `finished`）。
- **喝的时候**：
  - 攻击、翻滚、E 键丢掉，`reason` 是新的 `"drinking"`；
  - 锁定键照常能用；
  - `busy` 为 false，耐力照常恢复；
  - `invulnerable` 为 false。
- **打断**：喝的时候被打出硬直或打死，照 G3.4 打断（`action_end` 的 `reason` 是 `interrupted`）。已经过了回血的 tick，血已经回了；还没到，瓶子白用。

**补满**：
- 坐下篝火时（「篝火」的坐下效果里，第 2 步之后），`flask = flask_charges`；
- 角色复活时同样补满。

**状态导出**：`player` 的最后追加 `flask`：剩下几瓶，加载时等于 `flask_charges`。

**事件**：
- 新增 `heal`：字段 `amount`（实际回了多少，满血时是 0）、`hp`（回完之后的血量）。
- `action_start`、`action_end` 的 `action` 多了 `"drink"`。
- `input_dropped` 的 `input` 多了 `"drink"`，`reason` 多了 `"empty"`、`"drinking"`。
  - `drink` 键的丢弃原因用已有的判定顺序（`dead`、`staggered`、`rolling`、`startup`、`outranked`），然后是 `drinking`、`empty`。
  - `sitting`（G5.1）仍然排在最前面。

**表现**：
- 角色如果有一个名叫 `flask` 的**直接子节点**，它就是瓶子，必须带 `mesh`（报错 `the character "<名字>": its flask needs a mesh`）。
- 喝的时候显示，其余时候隐藏（`Scene::set_visible`）。
- 演示场景和 Boss 场景的角色各加一个：`(name: "flask", position: (0.5, 1.2, -0.3), scale: (0.12, 0.25, 0.12), mesh: Cube, color: (0.9, 0.7, 0.2))`。
  - 瓶子伸到胶囊体（半径 0.35）外面，默认相机在角色背后时也看得见（G5.2 审查时定：原来的 `x = 0.3` 被胶囊体整个挡住）。

**已知限制**（G5.2）：
- 喝没有动画，只有瓶子显示出来。
- 画面上看不到剩下几瓶（没有 HUD），看状态导出。

### 魂（G5.3 起）

用户决定（2026-09-27）：
- **有血的都给魂，数量是 prop**：假人、木桩、敌人、Boss 都可以在场景里写 `souls`，不写就不给。每次被角色打死都给，复活后再打死还给。
- **黑魂规则**：
  - 角色死了，身上的魂全部掉在死的地方，留下一个血痕；
  - 再死一次之前没捡回，这些魂就永久消失，新的血痕取代旧的。
- **靠近按 E 捡回**：和坐篝火共用 `interact` 键。
- **只计数**：这一项只做获得、掉落、捡回。魂的数量在状态导出里看，花魂（升级）以后有需要再加。

**参数**：
- 节点 props（非角色节点）多 1 个，接在最后（变成 97 个）：

| prop | 默认 | 说明 |
|---|---|---|
| `souls` | 0 | 整数 ≥ 0。被角色打死时给多少魂 |

  - 必须和 `hp` 一起写，用已有的文案 `the node "<路径>": prop "souls" needs hp`；
  - 这条检查接在已有的 `needs hp` 名单最后，`requires` 的文字变成 `"poise, stagger_ticks, poise_reset_ticks, revive_ticks, souls: need hp"`。
- 角色 props 多 1 个，接在 G5.2 的 5 个之后（变成 56 个）：

| prop | 默认 | 说明 |
|---|---|---|
| `bloodstain_range` | 1.5 | 米，> 0。角色到血痕的水平距离不超过它才能捡回 |

**获得**：在「命中」里，角色打中受伤方、这一下打死了它（`hit` 的 `result` 是 `death`）：
- `souls += 它的 souls`；
- `souls` 大于 0 时发 `souls` 事件，紧跟在这一下的 `hit` 事件之后。
- 只有角色能打死受伤方（挥砍者、敌人、冲击波只打角色），所以不用区分是谁打死的。

**掉落**：角色被打死时（第 5c 步，`player_hit` 里 `hp` 变成 0）：
1. 旧血痕（如果有）消失，里面的魂记为 `lost`；
2. 身上的 `souls > 0` 时，在角色现在的位置留下新血痕，里面是全部的 `souls`；
3. `souls = 0`；
4. `amount > 0` 或 `lost > 0` 时发 `souls_dropped` 事件，排在这一下的 `hit`、`action_end`（`interrupted`）、`lock`（`died`）之后。

- 血痕不受世界重置影响：坐篝火、角色复活都不动它。
- 身上没有魂时死了，旧血痕照样消失，也不留新血痕。

**捡回**：在「篝火」的 `interact` 判定里加一支，放在 `busy` 之后、`no_bonfire` 之前：

```text
if input.just_pressed("interact"):
    if action == sit: 站起来
    else if 角色死了: 丢掉（dead）
    else if 硬直: 丢掉（staggered）
    else if action == drink: 丢掉（drinking）
    else if action 不是 idle 或 sprint: 丢掉（busy）
    else if 有血痕，且它到角色的水平距离 <= bloodstain_range: 捡回
    else if 没有篝火的水平距离 <= bonfire_range: 丢掉（no_bonfire）
    else if 有活着的敌人在仇恨范围内: 丢掉（enemy_near）
    else: 坐下
```

- **捡回**：
  - `souls += 血痕里的魂`，血痕消失，发 `souls_recovered` 事件；
  - 是瞬间的，`action` 不变，冲刺中捡回也继续冲刺；
  - 敌人在附近也能捡，不查 `enemy_near`。
- 血痕和篝火都在范围内时，按一次 E 只捡回；再按一次才坐下。

**状态导出**：
- `player` 的最后追加 `souls`：身上的魂，加载时是 0。
- `game` 在 `respawn` 后面加 `bloodstain`：`{ "position": [x, y, z], "souls": n }`，没有血痕时是 `null`，加载时是 `null`。

**事件**：
- 新增：
  - `souls`：字段 `from`（被打死的节点路径）、`amount`、`total`（加完之后身上的魂）；
  - `souls_dropped`：字段 `position`（`[x, y, z]`）、`amount`（新血痕里的魂，没有留下血痕时是 0）、`lost`（旧血痕消失的魂，没有旧血痕时是 0）；
  - `souls_recovered`：字段 `amount`、`total`。
- `interact` 的丢弃原因不变。

**表现**：
- **血痕标记**：名叫 `bloodstain` 的**根节点**。
  - 它必须带 `mesh`，不能带 props，不能有子节点；
  - 报错 `the bloodstain marker "<路径>" needs a mesh, and no props or children`，检查接在篝火的检查之后；
  - 可以没有，那样血痕只在状态导出里。
- 有血痕时显示，节点位置是「血痕位置 + 场景里写的位置」（场景里写的是离地的偏移）；没有血痕时隐藏。
- 加载时就隐藏（同 G5.2 的瓶子）。每个 tick 在第 6 步设一次位置和可见性。
- 演示场景和 Boss 场景各加一个：`(name: "bloodstain", position: (0.0, 0.05, 0.0), scale: (0.4, 0.1, 0.4), mesh: Cube, color: (0.3, 0.9, 0.5))`。

**调试绘制**：新类别 `bloodstain`，排在 `bonfires` 之后。有血痕时，在血痕位置画 `bloodstain_range` 的圆，颜色 `(0.3, 0.9, 0.5)`。

**场景**：
- 演示场景：三个假人各写 `souls` 50，木桩写 100；
- Boss 场景：Boss 写 2000。
- 角色写出 `bloodstain_range` 1.5，带英文注释。

**已知限制**（G5.3）：
- 画面上看不到身上有多少魂（没有 HUD），看状态导出。
- 魂没有用处，只计数。
- 血痕留在死的地方，不像黑魂那样退回到最后站稳的位置。这里没有坑和摔死，所以死的地方总是能走到的。
- 角色的 `hits`、`last_hit` 不因死亡清零，同 G5.1。

### HUD（E6.1 起）

用户决定（2026-09-27）：显示血条和耐力条、原素瓶和魂、Boss 血条、锁定点。引擎部分见第 18 节。E6.1 做条和锁定点，E6.2 加文字（瓶数、魂数、Boss 名字）。

坐标都是 HUD 单位（画面高 720），`W` 是画面宽度。颜色写成 `(r, g, b, a)`，线性。

**血条和耐力条**（左上角）：
- **血条**：
  - 位置 `(40, 40)`，高 12，满长 `L = hp_max × 3`（100 血是 300）；
  - 从下往上叠三层：
    1. 底 `(0.02, 0.02, 0.02, 0.75)`，长 `L`；
    2. 延迟条 `(0.85, 0.7, 0.35, 1)`，长 `L × hp_trail / hp_max`；
    3. 血量 `(0.6, 0.05, 0.05, 1)`，长 `L × hp / hp_max`。
- **耐力条**：
  - 位置 `(40, 58)`，高 8，满长 `stamina_max × 3`；
  - 两层：底同上；耐力 `(0.1, 0.5, 0.15, 1)`，长按 `stamina / stamina_max`。
- 长度是 0 的层不画。
- 角色死了也照常画，血是 0。

**延迟条**：扣血后，原来的血量先停一会儿，再慢慢降下来。它是模拟状态，进状态导出。
- `player` 在 `souls` 后面加两个字段：
  - `hp_trail`：延迟条停在多少血，f32，加载时等于满血；
  - `hp_trail_wait`：还要停多少 tick，加载时是 0。
- 角色每次扣血（`player_hit` 里 `hp` 变少），`hp_trail_wait = 30`。
- **第 5d 步**（新增，第 5c 步之后）：

```text
if hp >= hp_trail: hp_trail = hp; hp_trail_wait = 0
else if hp_trail_wait > 0: hp_trail_wait -= 1
else: hp_trail = max(hp, hp_trail - hp_max / 60)       // 1 秒降完满血那么长
```

- 所以：
  - 回血、坐篝火、复活时，延迟条马上跟上血量；
  - 被打的那个 tick，第 5d 步就把 `hp_trail_wait` 从 30 减到 29。
- 30 和 60 是 `souls` 的常量。

**Boss 血条**（下方居中）：
- 敌人的新 prop `boss_bar`，接在节点 props 最后（变成 98 个）：

| prop | 默认 | 说明 |
|---|---|---|
| `boss_bar` | 0 | 整数，0 或 1。1 表示打它时画 Boss 血条 |

  - 写了必须有 `enemy_speed`，用已有的文案。
- **什么时候显示**：
  - 敌人写了 `boss_bar` 1、没死，并且角色活着、在它的 `enemy_aggro_range` 之内（水平距离）；
  - 不需要状态。
- **样子**：
  - 位置 `((W - 700) / 2, 720 - 60)`，高 10，满长 700；
  - 两层：底同血条；血量 `(0.6, 0.05, 0.05, 1)`；
  - 没有延迟条。
  - 同时有几个时，按先序往上叠，每个往上挪 30。
- Boss 场景的 Boss 写 `boss_bar` 1。

**锁定点**：
- 锁定时，在目标的锁定点（目标位置加 `(0, lock_height, 0)`）画一个圆点：半径 5，颜色 `(1, 1, 1, 0.9)`。
- 用 `HudView::project` 换成屏幕坐标，在相机后面就不画。

**画的顺序**：
1. 血条；
2. 耐力条；
3. Boss 血条；
4. 锁定点。

**文字和图标**（E6.2 起）：
- **图标**：
  - `hud_sheets()` 声明 `assets/art/souls_hud.json`，里面两个 tag：`flask`（金色小瓶子）、`soul`（淡色的魂）；
  - 每个 16×16，用 Aseprite MCP 画，`.aseprite` 源文件一起提交。
- **原素瓶**（左下角）：
  - 图标 `flask` 在 `(40, 720 - 72)`，`scale` 2（32×32）；
  - 瓶数的文字在 `(80, 720 - 63)`，`scale` 2，颜色 `(0.95, 0.92, 0.85, 1)`；
  - 瓶数是 0 时，图标的 `color` 是 `(0.35, 0.35, 0.35, 1)`，看起来是空瓶；其余时候是白色。
- **魂**（右下角）：
  - 数字右对齐，右端在 `W - 40`：文字的 `at.x = W - 40 - text_width`，`y = 720 - 63`，颜色同瓶数；
  - 图标 `soul` 在数字左边 8 个单位：`(at.x - 8 - 32, 720 - 72)`，`scale` 2；
  - 0 个魂也照常显示 `0`。
- **Boss 名字**：
  - 写了 `boss_bar` 1 的敌人，如果节点有 `display_name`，名字画在它的 Boss 血条上面：`(血条的 x, 血条的 y - 20)`，`scale` 2，颜色 `(0.9, 0.88, 0.8, 1)`；
  - 没有 `display_name` 就只画条。
  - Boss 血条每个往上挪的距离从 30 改成 **40**，给名字留地方。
  - 校验：写了 `boss_bar` 1 的敌人，`display_name` 里只能有 32 到 126 的字符，否则报 `the node "<路径>": display_name has a character the HUD font lacks: '<字符>'`。这条检查接在节点检查的最后。
- Boss 场景的 Boss 写 `display_name: "Iudex Gundyr"`。
- **画的顺序**：
  1. 血条；
  2. 耐力条；
  3. 每个 Boss 的血条，然后是它的名字；
  4. 原素瓶；
  5. 魂；
  6. 锁定点。

**已知限制**（E6.1）：
- Boss 血条没有延迟条。
- HUD 的大小跟着画面高度缩放；截图不是 720 高时，条的边缘可能落在半个像素上。

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
| `step` | `{"ticks":N}`，N ≥ 1，省略为 1 | 用保持的输入跑 N 个 tick。`{"tick":跑完之后的tick,"events":[...]}`。`events`（E5.1 起）是这 N 个 tick 里发出的全部事件，按先后排列，每个事件在 `type` 前面多一个 `tick` 字段：`{"tick":57,"type":"hit",…}` |
| `state` | 无 | 状态导出的内容（第 6 节），作为 JSON 值，不是字符串 |
| `screenshot` | `{"path":"...","size":[W,H],"debug_draw":…,"hud":…}`，`size`、`debug_draw`、`hud` 可以省略 | 把**当前状态**渲染成 PNG（自动创建父目录）。`debug_draw`（E5.2 起）是 `true`（全部类别）、类别名的数组或 `false`，省略就是 `false`，规则同 `--debug-draw`（第 16 节）；类别名写错回复 -32602。`hud`（E6.1 起）是 `true` 或 `false`，省略就是 `true`（第 18 节）。`{"path":...,"size":[W,H],"adapter":{...},"render_ms":毫秒}` |
| `record` | `{"path":"..."}` | 把从上次 `load_scene`/`reset`（或启动）以来的输入写成输入脚本，规范形式和 `--record` 相同。`{"path":...,"ticks":当前tick}` |
| `trace` | `{"path":"..."}` 开始，`{}` 停止（E5.1 起） | 开始：把轨迹（第 6 节「轨迹」）写进 `path`（自动创建父目录，已有的文件清空），立刻写一行当前 tick，这一行的 `events` 是当前 tick 发出的事件（tick 0 是 `[]`）；之后每跑一个 tick 写一行。已经在写别的文件时，先停掉那个。result `{"path":…,"tick":当前tick}`。停止：写完并关闭，result `{"path":停掉的路径或null,"lines":写了几行}`。`load_scene`、`reset` 时文件清空，从新的 tick 0 重新写；`quit` 和适配程序退出时写完关闭 |
| `scene` | 无 | 当前场景的规范格式全文（E5.3 起）：`{"source":"RON 文本","edited":是否编辑过}` |
| `node` | `{"path":"..."}` | 一个节点（E5.3 起）：`{"path":…,"ron":"这个节点的规范格式，不含 children","children":[子节点名，按顺序],"transform":运行时的局部变换}`。`ron` 是场景文件里写的，`transform` 是游戏当前写进节点的（格式同状态导出的 `nodes`），两者可能不同 |
| `edit_node` | `{"path":"...","set":"RON 字段","unset":["字段名"],"props":{"名字":数值或null}}`，后三项都可以省略 | 改一个节点（E5.3 起），见下文「运行中编辑」 |
| `add_node` | `{"parent":路径或null,"index":位置,"ron":"(name: …, …)"}`，`index` 可以省略 | 加一个节点（可以带子节点）；`parent` 为 `null` 时加在根上；`index` 是插在兄弟中的位置，省略就是最后（E5.3 起） |
| `remove_node` | `{"path":"..."}` | 删一个节点和它的整个子树（E5.3 起） |
| `edit_scene` | `{"set":"RON 字段"}` | 改顶层的 `camera`、`ambient`（E5.3 起）。`version`、`game` 不能改 |
| `save_scene` | `{"path":"..."}` | 把当前场景的规范格式写进文件（自动创建父目录）（E5.3 起）。`{"path":…}` |
| `schema` | 无 | 和 `loom schema` 的 `schema` 字段相同的内容（E5.4 起，第 17 节） |
| `quit` | 无 | `{}`，然后退出 |

### 运行中编辑（E5.3 起）

用户决定（2026-09-27）：
- **改场景后重新模拟**：编辑改的是场景文件的内容，引擎用改后的场景从 tick 0 开始，按已经录下的输入重跑到当前 tick。
  - 游戏代码不用为编辑做任何事，确定性自动成立。
  - 编辑不进录制，复现靠「改过的场景 + 录制」。
- **节点的任意字段都能改**：用 RON 片段写字段；增加节点也给一段 RON。改完走和加载场景一样的校验。
- **保存**：`save_scene` 写规范格式（第 10 节）。

**一次编辑**（`edit_node`、`add_node`、`remove_node`、`edit_scene`）：
1. 在当前的场景文件结构上做这次修改；
2. 生成规范格式，按加载场景的全部规则解析、校验，交给游戏建新的实例（游戏自己的要求也在这一步查，比如 souls 的 props）；
3. 从 tick 0 用录下的输入（上次 `load_scene`/`reset` 以来的，和 `record` 写出的相同）重跑到编辑前的 tick；
4. 用新的场景和游戏替换旧的。

- 任何一步失败：回复错误（-32602，消息说明原因），**什么都不变**。
  - 校验错误的文案是加载场景时的原因本身，不带 `load_scene` 回复里的 `invalid scene source: ` 前缀（E5.3 审查时按实现定下）。
  - 编辑操作本身的错误文案（`EditError`）：`no node at "<路径>"`、`edit_node needs set, unset or props`、`set cannot give children: use add_node and remove_node`、`<字段> is both in set and in unset`、`unset cannot remove name`、`unset cannot remove props: set props: [] instead`、`unset cannot remove children: use remove_node`、`unset: unknown field "<f>", expected one of …`、`unset lists "<f>" twice`、`prop "<名字>" is not on the node "<路径>"`、`index <N> is past the end: "<父节点>" has <M> children`（根上是 `the root has <M> children`）、`edit_scene cannot change version`（`game`、`nodes` 同理）。`set` 片段的解析错误是 `set: <行>:<列>: <ron 的错误>`，行列按片段本身算；`add_node` 的是 `ron: <ron 的错误>`。
  - 唯一的例外：重跑中途写轨迹失败时，回复 Engine 错误，会话仍是编辑前的，但轨迹已经丢了（历史要重写，原文件保不住）。只在 IO 出错时发生，接受（E5.3 审查时定）。
- 成功：result `{"tick":当前tick}`，和编辑前相同。
- **保持的输入**不变。**录制**不变：它只有输入，和场景无关。
- **轨迹**：正在写轨迹时，文件清空，从 tick 0 重写到当前 tick（历史变了）。重跑期间的事件不出现在任何回复里。
- 编辑一直保留，直到下一次 `load_scene`。`describe` 的 `scene` 仍是加载时的路径，磁盘上的文件不变，差别看 `edited`。`reset` 回到**改后场景**的 tick 0。
- 保证：编辑后 `state` 的内容，等于 `loom run --headless --scene <save_scene 写出的文件> --input <record 写出的脚本> --frames <tick> --dump-state` 的结果（tick 为 0 时就是改后场景的初始状态）。
- 代价：每次编辑都要重跑当前 tick 数那么多个 tick。无头模式下 souls 的一个 tick 是微秒级，几千 tick 之内可以忽略。

**`edit_node` 的参数**，按这个顺序应用：
- `set`：RON 字段列表，写法和场景文件里节点的字段一样，比如 `"position: (1.0, 0.0, 2.0), color: (1.0, 0.0, 0.0)"`。给出的字段整个替换原来的值（`props` 也是整个替换）。
  - 不能写 `children`（用 `add_node`、`remove_node`）。
  - 可以写 `name`（改名）；引用它的路径（顶层 `camera`、`orbit_camera.target`）不会跟着改，改名后校验不通过就报错。
  - 写了不认识的字段、同一个字段写两次，都报错。
- `unset`：要删掉的可选字段名（比如 `"mesh"`、`"look_at"`）。不能删 `name`；`props`、`children` 不能出现在这里（`props` 用 `set` 给空列表）。同一个字段不能既在 `set` 又在 `unset`。
- `props`：按名字合并。数值：有这个 prop 就原地改值，没有就加在最后；`null`：删掉，没有这个 prop 时报错。多个名字按名字的字典序处理。
- 三项都没给：报错（`edit_node needs set, unset or props`）。

**`edit_scene`**：`set` 只能写 `camera`、`ambient`，规则同 `edit_node`。

**`describe`** 多一个 `edited` 字段（E5.3 起，放在 `scene` 后面）：当前场景是否被编辑过。`load_scene` 之后是 false。

### 保证

- **确定性**：模拟不读墙钟。同样的启动参数和同样的请求序列，回复逐字节相同，只有 `render_ms` 和 `adapter` 除外。
- **`state` 和 `--dump-state` 是同一份内容**：字段顺序和每个数值的文本都相同，只是 `state` 用紧凑格式（没有空格和换行）。所以把 `--dump-state` 文件的空白全部去掉，就等于 `state` 回复里 `result` 的原文。
  - 实现上，回复要从带类型的结构体直接序列化，**不能经过 `serde_json::Value`**：`Value` 会把对象的键按字母排序，还会把 f32 转成 f64，写出 `0.2617993950843811` 这样更长的数字，逐位比较就对不上了。
- **轨迹一致**（E5.1 起）：同一个会话里 `trace` 写出的文件，和用 `record` 的脚本回放时 `loom run --trace` 写出的文件，从 `trace` 开始的那个 tick 起逐行相同。
- **能回放**：`record` 写出的脚本，用 `loom run --headless --frames <tick> --input <脚本> [--scene 同一个场景] --dump-state` 回放，得到的状态导出去掉空白后，和回放前 `state` 的 `result` 原文相同。
- **截图和 `loom run` 一致**：同一个状态，`screenshot` 和 `loom run --headless --screenshot` 的 PNG 逐字节相同。
- **例外：写轨迹失败**（E5.1 起）。`step` 中途写轨迹失败时回复错误，但已经跑过的 tick 不回滚，状态已经变了。写文件失败很少见，回滚的代价不值得。
- **`quit` 之后**关闭轨迹失败时，只写 stderr 日志、退出码 1，stdout 不再输出任何东西：回复已经发出，再写一个错误 JSON 会破坏「一行一条回复」（E5.1 审查时定，E5.2 顺手改，E5.1 的实现会再写一个错误 JSON）。
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
  - `describe`、`load_scene`、`reset`、`input`、`step`、`state`、`screenshot`、`record`，E5.1 起加 `trace`（参数 `path` 可以省略，省略就是停止）。
  - E5.2 起 `screenshot` 多一个可选参数 `debug_draw`，同 serve。
  - E6.2 起 `screenshot` 多一个可选参数 `hud`，同 serve（第 18 节）。
  - E5.3 起加 `scene`、`node`、`edit_node`、`add_node`、`remove_node`、`edit_scene`、`save_scene`。
  - E5.4 起加 `schema`。
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

### 事件（E5.1 起）

和 `souls` 一样进轨迹和 serve 的 `step` 回复（第 6 节「轨迹」）。规划者决定：只发和消息对应的一种事件，字段尽量和 `souls` 的 `hit` 相同。

| `type` | 字段 | 什么时候发 |
|---|---|---|
| `hit` | `by`、`target`、`damage`、`hp`、`result` | 玩家撞击僵尸，或者僵尸咬玩家；和那条消息在同一处发 |

- `by`、`target`：节点路径。
- `damage`：攻击力（整数，玩家是 `player_attack`，僵尸是它的 `attack`）；`hp`：之后剩下的血量（整数）。
- `result`：`"hit"` 或 `"death"`（打到 0）。
- 一条 `hit` 对应一条或两条消息（`You kill`、`bites you` 加 `You die`）。其余情况（撞墙、走路、僵尸走一步）不发事件，状态里看得出来。

## 16. 调试绘制（E5.2 起）

用户决定（2026-09-27）：
- **线框，永远画在最上层**：不做深度测试，隔着墙、木桩也看得见。
- **按类别选**：`--debug-draw` 不带值是全开，带值只开列出的类别；serve 的 `screenshot` 有同样的参数。类别名由游戏定义，写错报错。
- **窗口里按 F3 开关**：只影响画面，不进输入录制，不影响模拟。
- souls 画受击判定、攻击扇形、碰撞体、锁定（第 11 节「调试绘制」）。

**分工**：引擎只提供画线段的能力，画什么由游戏决定。
- `loom_core::debug`：
  - `DebugLine { a: Vec3, b: Vec3, color: Vec3 }`：世界坐标的一条线段，线性 RGB，不透明。
  - `DebugDraw`：收集线段的缓冲区。除了 `line`，还有几个拆成线段的形状：
    - `circle(center, radius, color)`：水平圆（XZ 平面），32 段；
    - `cylinder(base, radius, height, color)`：底圆、顶圆，加 4 条竖线（角度 0°、90°、180°、270°）；
    - `sector(center, facing, radius, lo, hi, color)`：水平扇形，角度的约定和第 11 节的 `bearing` 相同，外弧按每段不超过 360°/32 分段；
    - `wire_box(center, half, rotation, color)`：12 条棱；
    - `cross(point, size, color)`：沿 X、Y、Z 的三条线段。
  - 画形状用的三角函数用 libm，和模拟一样（这里只影响画面，不影响状态，用 libm 只是为了游戏 crate 的 lint 规则统一）。
- 游戏：
  - `debug_categories()`：这个游戏的类别名，固定顺序；
  - `debug_draw(&self, categories, &mut DebugDraw)`：只读，按当前状态画开着的类别。**不许改变任何状态**。
- `loom_render`：`Frame` 多一个 `lines: &[LineDraw]`，在网格和精灵之后、同一个离屏目标上画，线段拓扑，1 像素宽，关掉深度测试。截图、窗口都一样。`loom_engine` 把 `DebugLine` 转成 `LineDraw`。
- 画的是**当前状态**，和截图、状态导出取的同一时刻。

**开关**：
- `--debug-draw` 不带值：游戏的全部类别。带值：逗号分隔的类别名，不能为空、不能重复，要在游戏的类别里；否则按参数错误报出（退出码 2）：`unknown debug draw category "<名字>": <游戏> has <类别列表>`、`debug draw category "<名字>" is given twice`。
- 类别列表的写法和 `unknown game` 那条相同：`souls has "hurtboxes", "swings", "colliders" and "lock"`；没有类别时是 `rogue has none`。`--debug-draw=`（等号后为空）按未知类别 `""` 报（E5.2 审查时按实现定下）。
- serve 的 `debug_draw: []` 等于 `false`，不报错：JSON 数组不会有命令行那种「写了逗号漏了名字」的问题（E5.2 审查时定）。
- 窗口模式退出时，如果 F3 是在最后一帧之后才按的，先按当前的开关补画一帧，再存 `--screenshot`（E5.2 实现时定）。
- `rogue` 目前没有类别：`--debug-draw` 不带值时什么都不画；带值时都是未知类别。
- `loom view` 不跑游戏，没有 `--debug-draw`。
- **F3**（只在窗口模式）：在「命令行给的类别」（没给 `--debug-draw` 时是全部类别）和「全关」之间切换。启动时：给了 `--debug-draw` 是开，没给是关。F3 是引擎保留的键，和 Esc 一样不能绑定，不进录制，热重载后保持当前的开关。
- 窗口模式退出时的 `--screenshot` 按退出那一刻的开关画。

**保证**：
- 调试绘制不改变模拟：开不开，状态导出、轨迹、录制都逐字节相同。
- 没开调试绘制时，截图和 E5.2 之前逐字节相同（`lines` 为空时不发绘制命令）。
- 开着的截图同样确定：同一个状态、同一个后端，PNG 逐字节相同。不同后端之间可能差几个像素，和网格一样（第 3、4 节），smoke 不做跨后端比较。

## 17. 格式说明 `loom schema`（E5.4 起）

用户决定（2026-09-27）：
- **和代码同源**：结构用 `schemars` 在同一个 Rust 类型上生成 JSON Schema，`///` 注释就是说明文字；props 的名字、默认值、取值规则直接从游戏现有的参数表导出。改代码，说明就跟着变。
- 覆盖范围由规划者定：**场景文件和 props**、**状态导出和事件**，也就是写场景、读状态时要用的。输入脚本、键位表格式简单，`describe` 已经列出动作和轴；serve 的参数 MCP 已经有 schema。这两类以后有需要再加。

**输出**：`loom schema` 的 stdout 照第 6 节只有一个 JSON 对象：`{"status":"ok","schema":{…}}`。`schema` 的结构：

```json
{
  "loom_schema": 1,
  "scene": { JSON Schema },
  "games": {
    "rogue": { "props": {…}, "state_dump": { JSON Schema }, "trace_line": { JSON Schema }, "event": { JSON Schema } },
    "souls": { … }
  }
}
```

- `loom_schema`：这份说明本身的格式版本，现在是 1。
- JSON Schema 用 schemars 1.x 默认的 draft 2020-12。对象的键按上面的顺序；`games` 按游戏名排序。
- **`scene`**：场景文件（第 10 节）的结构，从 `loom_scene` 的文件结构类型生成。
  - 场景文件是 RON，这里按它的 serde 结构描述：元组是定长数组，可选字段可以不写（RON 里写成裸值），`mesh` 这类枚举是 serde 的外部标签形式。schema 的顶层 `description` 里写明这几条对应关系。
  - `deny_unknown_fields` 体现为 `additionalProperties: false`。
  - 结构以外的规则（名字的正则、`scale > 0`、`near < far` 等）写在对应字段的 `///` 注释里。能直接用 schemars 属性表达的（名字的 `pattern`、`version` 的取值）就用属性。
- **`props`**：按节点的种类分组，每组是一个对象 `{"props": [...], "requires": [...], "constraints": [...]}`（E5.4 审查时按实现定下）。`props` 的每项：

```json
{ "name": "stamina_max", "default": 100.0, "rule": { "kind": "positive" }, "unit": "stamina points", "description": "…" }
```

  - `default`：没有默认值（必须写，或者写了才开启某种功能）时是 `null`。
  - `rule` 的 `kind`：
    - `positive`（> 0）；
    - `at_least`（`min`，≥ min）；
    - `whole`（`min`，可选的 `max`：整数，含两端）；
    - `below`（`min`、`max`：`min ≤ v < max`）；
    - `arc`（0 < v ≤ 360）；
    - `any`（值本身不查，只参与 `constraints` 里的规则，例如 souls 角色的 `swing_high`）。
    - 两个游戏用到的规则都要能表示；新规则加进这个列表。
  - `unit`、`description`：英文，写在游戏的参数表里（表多一两列），和代码同一处维护。演示场景里每个 prop 的英文注释可以直接拿来用。
  - souls 的分组：
    - `character`：角色节点的 50 个；
    - `node`：其他节点的 17 个。
    - `requires`：哪些 prop 必须和哪个一起写，写成「谁: needs/need 谁」，例如 `"poise, stagger_ticks, poise_reset_ticks, revive_ticks: need hp"`；
    - `constraints`：跨 prop 的规则，写成字符串，例如 `"lock_range <= lock_break_range"`。
  - rogue 的分组：`player`、`zombie`（第 15 节），`requires`、`constraints` 都是空列表。
- 对象的 `properties` 按键的字母顺序（workspace 没开 serde_json 的 `preserve_order`），和 serde 实际输出的字段顺序不同；我们自己的结构（`loom_schema`、`scene`、`games`、每个游戏的四部分、props 的每项）按声明顺序。结果是确定的。
- `description` 来自 `///` 注释，保留了注释的换行，Rust 的写法（比如 `` `None` ``，对应 JSON 的 `null`）原样保留。
- **`state_dump`**：这个游戏的完整状态导出（第 6 节，包括顶层的 `tick`、`nodes` 等），`game` 是这个游戏的状态类型。
- **`event`**：这个游戏的事件（第 11、15 节「事件」），每种 `type` 一个分支。
- **`trace_line`**：轨迹的一行（第 6 节「轨迹」）。

**提交进仓库**：生成的结果写在 `schema/loom.schema.json`（`schema` 字段的内容，pretty 格式，末尾换行）。
- 有测试比较它和生成的结果，不同就失败。设了环境变量 `LOOM_UPDATE_SCHEMA=1` 时，测试改为重写这个文件。
- 所以格式一变，提交的 diff 里就看得见；AI 不运行程序也能读。

**保证**（都有测试）：
- 仓库里所有场景文件，按文件结构类型解析、再序列化成 JSON 之后，都符合 `scene`。
- CLI 测试里所有状态导出的期望值，都符合对应游戏的 `state_dump`；轨迹测试写出的每一行，都符合 `trace_line`。
- 每个游戏参数表里的每一项都出现在 `props` 里，默认值相同。这一条由生成方式保证，测试里抽查几项。
- 校验用 `jsonschema` crate，只作开发依赖。

**已知限制**：`constraints` 和 `requires` 是写在代码里的文字，和真正做检查的代码在同一个文件，但没有机制保证两者一致。改检查时要记得改它们（写进 TODO 的「测试覆盖的已知薄弱处」）。

## 18. HUD（E6 起）

用户决定（2026-09-27）：
- **游戏每帧给一份绘制列表**：和调试绘制一样，游戏按当前状态出一串「矩形、圆点、文字」，引擎只负责画。HUD 永远和状态一致。
- **文字用自己画的像素字体**：用 Aseprite MCP 画一张 ASCII 字形图，走第 14 节的 sprite sheet 管线，不加依赖（E6.2）。
- **默认画进截图，可以关**：截图就是玩家看到的画面；`--no-hud` 关掉。
- souls 画血条、耐力条、原素瓶、魂、Boss 血条、锁定点（第 11 节「HUD」）。

切片：
- **E6.1**：屏幕空间的矩形和圆点，`--no-hud`；souls 的血条、耐力条、Boss 血条、锁定点。
- **E6.2**：像素字体和文字；souls 的原素瓶数、魂数、Boss 名字。

**坐标**：HUD 单位。
- 画面高度永远是 **720 个 HUD 单位**，宽度是 `720 × 宽 / 高`。1280×720 的截图里，1 个 HUD 单位就是 1 个像素。
- 原点在左上角，+X 向右，+Y 向下。
- 靠右、靠下的东西由游戏用 `width`、`height` 自己算位置。

**分工**：
- `loom_core::hud`：
  - `HudItem`，每项都是屏幕上的一块：
    - `Rect { min: Vec2, size: Vec2, color: Vec4 }`：轴对齐的实心矩形；
    - `Disc { center: Vec2, radius: f32, color: Vec4 }`：实心圆点；
    - E6.2 再加文字。
  - 颜色是线性 RGB 加 alpha，每个分量都在 0 到 1 之间。
  - `Hud`：收集绘制项的缓冲区，按添加的顺序画。
  - `HudView { width, height, view_proj }`：画面大小（HUD 单位），加上相机的「视图 × 投影」矩阵。
    - `project(world: Vec3) -> Option<Vec2>`：世界坐标换成 HUD 坐标。在相机后面（裁剪空间 `w <= 0`）时返回 `None`；在画面外但在相机前面时照常返回。
- 游戏：`hud(&self, view: &HudView, hud: &mut Hud)`，只读，按当前状态画。**不许改变任何状态**。`rogue` 什么都不画。
- `loom_render`：
  - `Frame` 多一个 `hud: &[HudDraw]`，在网格、精灵之后，调试线段之前画。所以调试线框在 HUD 上面。
  - 不做深度测试，按列表顺序画，**alpha 混合**（`src_alpha, one_minus_src_alpha`，在线性空间里混合）。顺序固定，所以结果确定。
  - 圆点画成外接正方形，片元离圆心的距离大于半径就丢掉。边缘是硬边，没有抗锯齿。
  - 列表为空时不发绘制命令。
  - `loom_engine` 负责把 HUD 单位换成像素，交给渲染器。

**开关**：
- `loom run` 加 `--no-hud`：窗口、无头、截图都不画 HUD。
- serve 的 `screenshot` 多一个参数 `hud`：`true` 或 `false`，省略就是 `true`。
- 报告在 `debug_draw` 后面加 `hud`：画了是 `true`，`--no-hud` 时是 `false`。
- 窗口里没有开关 HUD 的键。
- `loom view` 不跑游戏，没有 HUD。

**保证**：
- HUD 不改变模拟：画不画，状态导出、轨迹、录制都逐字节相同。
- 列表为空时（`rogue`、`--no-hud`），截图和 E6 之前逐字节相同。
- 同一个状态、同一个后端，截图逐字节相同。

**E6.1 审查时定**：
- **GPU 测试**：`loom_render` 的 HUD 渲染测试标 `#[ignore]`，不进 `cargo test`；由 smoke 的 H11 用 `--ignored` 跑。
- **投影矩阵**：`loom_render::view_projection()` 公开，引擎算 `HudView` 时用它，保证和渲染器是同一个矩阵。
- **smoke 的 H4**：两个后端都加 `--no-hud` 比较。
- **报告的 `hud`**：反映的是开关，和 `debug_draw` 一样。游戏什么都不画时也是 `true`。
- **MCP 的 `screenshot` 工具**：E6.1 时还没有 `hud` 参数，总是带 HUD；E6.2 补上透传。

### 文字和图片（E6.2 起）

用户决定（2026-09-27）：
- **像素字体**：5×7 的字形放在 6×8 的格子里，带完整 ASCII（大小写、数字、标点），HUD 里放大 2 倍，字高 14。
- **Boss 名字来自节点的新字段** `display_name`（第 10 节）。
- **原素瓶和魂用小图标加数字**，图标和字体一样用 Aseprite 画。

**字体**：引擎自带一张字形图，属于引擎，不属于哪个游戏。
- 文件：
  - `assets/fonts/loom_5x7.aseprite`：源文件；
  - `assets/fonts/loom_5x7.json`、`.png`：导出，第 14 节的 `json-array` 格式，只有 1 帧。
  - 用 Aseprite MCP 画，PNG 和 `.aseprite` 走 LFS（`.gitattributes` 已覆盖 `assets/**`）。
- 布局：
  - 整张图 96×48，16 列 6 行，每格 6×8；
  - 字符码 `c`（32 到 126）在第 `(c - 32) % 16` 列、第 `(c - 32) / 16` 行；最后一格（127）空着；
  - 字形占格子左上角的 5×7，第 6 列和第 8 行空着，就是字间距和行间距；
  - 字形是白色不透明，其余全透明。
- **读取**：`loom run`、serve 启动时，按第 14 节的规则读一次，路径相对当前工作目录。
  - 不论画不画 HUD 都读；`loom view` 不读。
  - 另外检查图片是 96×48，否则报错 `the HUD font assets/fonts/loom_5x7.json: <原因>`，退出码同场景错误。

**新的绘制项**（`loom_core::hud::HudItem`）：
- `Text { at: Vec2, scale: f32, color: Vec4, text: String }`：
  - `at` 是第一个字的格子左上角；
  - 每个字前进 `6 × scale`，不换行；
  - 字形像素的颜色是 `color`；
  - 32 到 126 以外的字符画成 `?`。
  - `Hud::text_width(text, scale)`：`(6 × 字数 - 1) × scale`，0 个字时是 0。用来右对齐。
- `Image { sheet: &'static str, tag: &'static str, at: Vec2, scale: f32, color: Vec4 }`：
  - 画 sheet 里这个 tag 的第一帧，左上角在 `at`，每个贴图像素占 `scale` 个 HUD 单位；
  - 颜色是贴图颜色乘以 `color`，传白色就是原色。
- **图片要先声明**：
  - 游戏提供 `hud_sheets() -> &'static [(&'static str, &'static [&'static str])]`，列出 sheet 路径和用到的 tag；
  - 引擎在加载场景时按第 14 节的规则读它们、检查 tag。
  - 画的时候遇到没声明的 sheet 或 tag，这一项不画，打一条 `error` 日志；测试保证游戏不会这样用。
- 文字和图片都照 sprite 的做法：
  - 用 `textureLoad` 直接取像素；
  - alpha < 0.5 的片元丢掉，其余的颜色是 `texel.rgb × color.rgb`、alpha 是 `color.a`，再照常混合。
- 和矩形、圆点混在同一个列表里，按顺序画。
- 想让字形像素落在整数个屏幕像素上，`at` 和 `scale` 要让 `HUD 单位 × 高 / 720` 是整数。1280×720 时，`at` 取整数、`scale` 取 2 就行。

**MCP**：`loom-mcp` 的 `screenshot` 工具加 `hud` 参数，透传给 serve，规则同 serve（第 13 节）。
