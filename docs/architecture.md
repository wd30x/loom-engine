# loom-engine 架构

本文档记录**已经定下来的决策**。实现者不要改这里的内容，有异议写进 `docs/FEEDBACK.md`，由规划者决定后更新这里。

## 1. crate 分层

```
            loom_engine  (bin: loom)
           /     |      \
loom_platform  loom_render   (彼此不知道对方)
       |
   loom_core
```

- `loom_core`：纯逻辑。固定步长时钟、tick 计数、输入（按键、动作映射、采样、脚本、录制，见第 9 节）。不依赖 winit 和 wgpu，所以可以在任何环境下做单元测试。
- `loom_platform`：只有它能碰 winit。它把 OS 事件翻译成 `loom_core` 的输入类型，并把窗口句柄交给调用方。
- `loom_render`：只有它能碰 wgpu。**它不知道 winit 的存在**：创建 surface 时接收的是任何实现了 `Into<wgpu::SurfaceTarget<'static>>` 的东西（比如 `Arc<winit::window::Window>`），由 `loom_engine` 传进来。
- `loom_engine`：组装层。解析命令行，按窗口模式或无头模式把上面三个 crate 接起来。

以后新增的 crate（场景、远程控制等）也照这个思路来：**各自只依赖自己必须用到的东西**，由 `loom_engine` 统一组装。

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
- 执行方式：等出现专门的模拟 crate（G1）时，给它加一个 crate 级别的 `clippy.toml`，用 `disallowed-methods` 禁掉这些函数（见 TODO G1.0）。

**确定性的范围**（E0-4）：
- **图像**：只保证同一台机器、同一驱动下逐字节相同。实测同一块 GPU 上 Vulkan 和 Dx12 的截图也逐字节相同，但换 GPU 或换驱动不作保证。PNG 只用于本机回归和人（AI）眼检查。
- **模拟状态**：必须在任何机器上都逐位相同。跨机器的回归测试一律比较模拟状态的导出（E1 起），不比较图片。

## 5. 约定

| 项目 | 约定 |
|---|---|
| 坐标系 | 右手系，**+Y 向上**，**-Z 向前**，+X 向右（与 glTF 一致） |
| 单位 | 长度用米，角度用弧度，时间用秒 |
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

JSON 里的字段名一旦定下来就不要改名。要加新字段可以，旧字段不能删、不能改含义。

### 命令

```
loom run [OPTIONS]
    --headless             不创建窗口；每帧恰好 1 个 tick
    --frames <N>           跑 N 帧（N ≥ 1）后退出。无头模式必须提供
    --screenshot <PATH>    退出前把最后一帧存成 PNG，会自动创建父目录
    --dump-state <PATH>    退出前把模拟状态写成 JSON（见下文「状态导出」），会自动创建父目录
    --input <PATH>         回放输入脚本（第 9 节）；这时键盘不再驱动动作，Esc 仍然可以退出
    --record <PATH>        退出前把这次运行每个 tick 的动作变化写成输入脚本（第 9 节），会自动创建父目录
    --size <WxH>           渲染分辨率，默认 1280x720；窗口模式下只决定初始窗口尺寸（第 2 节）

loom info                  输出 GPU adapter 信息，不渲染
```

无头模式下，第 k 帧（从 1 开始数）跑第 k 个 tick；截图和状态导出取的都是第 N 个 tick 跑完之后的状态，所以 `--frames 60` 时 `sim_time = 1.0`。

### 输出格式

`loom run` 成功：
```json
{"status":"ok","mode":"headless","frames":60,"ticks":60,"sim_time":1.0,"size":[1280,720],"screenshot":"out/e0.png","state":null,"record":null,"input":null,"adapter":{"name":"...","backend":"Vulkan","driver":"..."}}
```
- `mode`：`"headless"` 或 `"window"`
- `screenshot`：没有截图时为 `null`
- `state`：没有导出状态时为 `null`，否则是 `--dump-state` 给的路径
- `record`：没有录制时为 `null`，否则是 `--record` 给的路径
- `input`：没有回放时为 `null`，否则是 `{"path":…,"events":脚本里的事件数,"applied":实际生效的事件数}`。脚本比运行长时 `applied < events`
- `adapter`：没有创建 GPU 设备时为 `null`（无头模式且没有 `--screenshot`）
- 窗口模式提前退出（Esc、关窗）时，`frames` 和 `ticks` 是实际跑到的数

`loom info` 成功：
```json
{"status":"ok","adapter":{"name":"...","backend":"Vulkan","driver":"...","device_type":"DiscreteGpu"}}
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
- 游戏状态整个放在**一个**以游戏命名的字段下。现在是 E0 的旋转立方体，字段名是 `demo`。
- 字段名的稳定性规则和 stdout 一样：可以加，不能删、不能改名、不能改含义。

```json
{
  "tick": 60,
  "sim_time": 1.0,
  "actions": [],
  "demo": {
    "yaw": 1.0,
    "position": [
      0.0,
      0.0,
      0.0
    ]
  }
}
```

`actions` 是最后一个 tick 生效的动作集合（按名字排序），见第 9 节。

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
- 目前的范围：`KeyA`–`KeyZ`、`Digit0`–`Digit9`、四个方向键、`Space`、`Enter`、`Tab`、`Backspace`、左右 `Shift`/`Control`/`Alt`。其他按键平台层直接忽略。
- **`Escape` 不是 `Key`**：它保留给引擎用来退出，不能绑定。
- 鼠标和手柄不在 E1 范围，等 G1.3。

### 动作映射 `ActionMap`

RON 文件，是一个列表（**不用 map**：实测 ron 遇到重复的 map 键会静默保留最后一个）：

```ron
[
    (action: "move_forward", keys: [KeyW, ArrowUp]),
    (action: "move_left",    keys: [KeyA, ArrowLeft]),
]
```

- 动作名必须匹配 `^[a-z][a-z0-9_]*$`
- 同一个动作不能出现两次；同一个动作的 `keys` 里不能有重复按键
- `keys` 可以为空（动作只由脚本驱动）；一个按键可以绑定到多个动作
- 结构体一律 `#[serde(deny_unknown_fields)]`：实测 ron 默认会静默忽略拼错的字段名

### 按 tick 采样

一个 tick 的动作集合在这个 tick 开始时确定一次。`InputCollector` 记住两样东西：当前按住的键，和「上次采样之后按下过的键」。

- 某个动作在这个 tick 生效，当且仅当它绑定的任意一个键**正按住**，或者**上次采样之后按下过**。采样后清空「按下过」。
- 所以两次 tick 之间按下又松开的短按，**恰好生效一个 tick**，不会丢。窗口模式一帧可能跑 0 个或多个 tick，这条规则让结果只取决于按键事件落在哪两个 tick 之间，不取决于帧率。
- 已经按住的键再收到按下事件（按键重复），不算新的一次按下。
- 窗口失去焦点时，释放所有按住的键，防止松手时窗口收不到事件而卡键。

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
