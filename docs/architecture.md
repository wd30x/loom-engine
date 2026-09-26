# loom-engine 架构

本文档记录**已经定下来的决策**。实现者不要改这里的内容，有异议写进 `docs/FEEDBACK.md`，由规划者决定后更新这里。

## 1. crate 分层

```
                  loom_engine  (bin: loom)
           /          |            |         \
loom_platform    loom_render   loom_scene     (彼此不知道对方)
       |
   loom_core
```

- `loom_scene`：场景文件的解析和校验，以及运行时的节点树（第 10 节）。只依赖 glam、serde、ron、thiserror，不知道渲染和平台。
- `loom_sim`（G1 起）：游戏的模拟逻辑（现在是演示，以后是角色、相机、碰撞）。依赖 `loom_core`、`loom_scene`、glam、libm。它有**自己的 `clippy.toml`**，用 `disallowed-methods` 禁掉 std 的超越函数（第 4 节）。注意 crate 级的 `clippy.toml` 会整个替换 workspace 的那份，所以要把 workspace 里的设置也抄过来。

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

**格式稳定性**（用户决定，2026-09-26）：在出现**第一个外部使用者**之前（预计是 E3 的远程控制 / MCP），stdout、状态导出、输入脚本、场景文件的格式**可以改**，但每次都必须：写进本文档、在同一个切片里同步改测试。出现外部使用者之后，改回严格规则：字段只能加，不能删、不能改名、不能改含义。

### 命令

```
loom run [OPTIONS]
    --headless             不创建窗口；每帧恰好 1 个 tick
    --frames <N>           跑 N 帧（N ≥ 1）后退出。无头模式必须提供
    --screenshot <PATH>    退出前把最后一帧存成 PNG，会自动创建父目录
    --dump-state <PATH>    退出前把模拟状态写成 JSON（见下文「状态导出」），会自动创建父目录
    --input <PATH>         回放输入脚本（第 9 节）；这时键盘不再驱动动作，Esc 仍然可以退出
    --record <PATH>        退出前把这次运行每个 tick 的动作变化写成输入脚本（第 9 节），会自动创建父目录
    --scene <PATH>         加载场景文件（第 10 节）；不给时用内置的演示场景。窗口模式下文件变化会热重载
    --no-capture           窗口模式下不抓鼠标（不锁定、不隐藏指针），也不读鼠标移动（轴只来自按键）。
                           smoke 测试用它：不占用桌面鼠标，有人动鼠标也不会污染录制。和 --headless 同时给是参数错误
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
- `scene`：用内置演示场景时为 `null`，否则是 `--scene` 给的路径
- `reloads`：窗口模式下成功热重载的次数；`reload_errors`：热重载时场景文件解析或校验失败的次数（失败时保持旧场景继续跑）。无头模式两者都是 0
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
- 游戏状态整个放在**一个**字段下：G1.2 起是 `game`（第 11 节），替换了 E0–G1.1 的旋转立方体 `demo`。
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
- `game` 的字段见第 11 节。角度都是弧度。

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
| `camera` | `(fov_y, near, far)` | 无 | `fov_y` 是垂直视场角（度）；相机看向自己的 -Z |
| `light` | `(color, intensity)` | 无 | 方向光，照向节点的 -Z |
| `character` | `(run_speed, turn_speed)` | 无 | 角色组件（第 11 节）。`run_speed` 米/秒，`turn_speed` 度/秒 |
| `orbit_camera` | `(target, pivot_height, distance, yaw, pitch, pitch_min, pitch_max)` | 无 | 环绕相机组件（第 11 节），必须和 `camera` 在同一个节点上。`target` 是带 `character` 的节点路径；角度都是度 |
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
- `orbit_camera`：节点上必须也有 `camera`；`target` 必须指向带 `character` 的节点；`pivot_height`、`distance` 有限且 `distance > 0`；`-90 < pitch_min < pitch_max < 90`；`pitch` 在 `[pitch_min, pitch_max]` 里
- `rotation` 和 `look_at` 不能同时出现；`look_at` 的目标不能等于 `position`，方向也不能和 Y 轴平行（`right` 会退化）
- 所有数字必须是有限值；`scale` 分量 > 0；颜色分量在 0 到 1 之间（节点 `color`、`light.color`、`ambient` 都是）；`light.intensity` ≥ 0；`fov_y` 在 (0, 180) 之间；`0 < near < far`

### 游戏和场景的关系

- 不给 `--scene` 时，用 `loom_sim` 里内置的演示场景，用 `include_str!` 嵌进程序。
- G1.2 起，游戏要求场景里有一个带 `character` 的节点，并且场景的当前相机（顶层 `camera`）带 `orbit_camera`、`target` 指向这个角色；角色节点和这个相机节点都必须是**根节点**（游戏直接写它们的局部变换）。不满足就报错。游戏逻辑见第 11 节。
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

实现在 `loom_sim`，受第 4 节确定性规则约束：三角函数一律用 `libm`，角度用 `f32` 弧度。

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

// 4. 移动
position += dir * (run_speed / 60.0)

// 5. 转身：朝 dir 转，每个 tick 最多转 turn_speed 度的 1/60
if v.length_squared() > 0.0 {
    target = libm::atan2f(-dir.x, -dir.z)
    diff   = wrap(target - facing)
    max    = turn_speed.to_radians() / 60.0
    facing = wrap(facing + clamp(diff, -max, max))
}
wrap(a) = a - TAU * floor((a + PI) / TAU)     // 包到 [-π, π)，floor 是精确运算

// 6. 写回节点
角色节点：position = position，rotation = Quat::from_rotation_y(facing)
相机节点：rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch)
          position = position + Vec3(0, pivot_height, 0) + rot * Vec3(0, 0, distance)
          rotation = rot
```

其中 `run_speed`、`turn_speed`、`pivot_height`、`distance`、`pitch_min`、`pitch_max` 来自场景组件（角度在加载时转成弧度：`deg.to_radians()`）。

### 初始状态（tick 0）

- `position`：角色节点在场景里的位置（胶囊体的底部，也就是脚下）
- `facing`：由角色节点在场景里的旋转算出：`f = rotation * Vec3::NEG_Z; facing = libm::atan2f(-f.x, -f.z)`，**再把 `-0.0` 规范成 `0.0`**（没有旋转时 `-f.x` 是 `-0.0`，`atan2f(-0.0, 1.0)` 得 `-0.0`）
- `yaw`、`pitch`：来自 `orbit_camera` 的 `yaw`、`pitch`（度转弧度）
- 加载时就按第 6 步写一次相机节点，所以 tick 0 的画面已经是正确的环绕视角

### 状态导出

```json
"game": {
  "player": { "position": [x, y, z], "facing": f },
  "camera": { "yaw": f, "pitch": f }
}
```

### 已知限制（G1.2）

- **没有碰撞**（G1.3 做）：角色能穿过柱子。
- **相机没有碰撞**：`pitch_max` 太大时相机会钻到地面以下（本机实测：`pivot_height` 1.5、`distance` 4 时，超过约 22° 就会穿地）。演示场景用 20°。以后做相机碰撞（被墙或地面挡住时拉近）再放开。
- 没有加速和减速：移动速度在一个 tick 内从 0 到满。
