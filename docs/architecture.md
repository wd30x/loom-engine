# loom-engine 架构

本文档记录**已经定下来的决策**。实现者不要改这里的内容，有异议写到任务文档的「实现记录」里。

## 1. crate 分层

```
            loom_engine  (bin: loom)
           /     |      \
loom_platform  loom_render   (彼此不知道对方)
       |
   loom_core
```

- `loom_core`：纯逻辑。固定步长时钟、tick 计数、输入状态。不依赖 winit 和 wgpu，所以可以在任何环境下做单元测试。
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

两种模式**共用同一套模拟代码和绘制代码**，只有帧推进方式和最后一步（呈现还是读回）不一样。

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

## 5. 约定

| 项目 | 约定 |
|---|---|
| 坐标系 | 右手系，**+Y 向上**，**-Z 向前**，+X 向右（与 glTF 一致） |
| 单位 | 长度用米，角度用弧度，时间用秒 |
| 投影 | `glam::Mat4::perspective_rh`（深度范围 0..1，与 wgpu 一致） |
| 颜色 | 离屏目标是 sRGB 格式，shader 里按线性空间计算；PNG 保存的就是 sRGB 编码后的字节 |
| 数学库 | 只用 `glam`，不引入别的数学库 |

## 6. 命令行输出协议

`loom` 是给 AI 和脚本调用的，所以输出格式是固定的：

- **stdout**：每次运行**恰好输出一个 JSON 对象**，最后带一个换行。成功时 `"status": "ok"`，失败时是 `"status": "error"` 加 `"error"` 字段。
- **stderr**：给人看的日志（`env_logger`，用 `RUST_LOG` 控制级别）。
- **退出码**：`0` 成功，`1` 运行时出错，`2` 命令行参数错误（clap 默认行为）。

JSON 里的字段名一旦定下来就不要改名。要加新字段可以，旧字段不能删、不能改含义。

## 7. 错误处理

- 库 crate：用 `thiserror` 定义各自的错误枚举。
- `loom_engine`：用 `anyhow` 汇总错误，在 `main` 里转成错误 JSON 和退出码 1。
- 不因为可恢复的情况 panic，比如窗口最小化后尺寸为 0，或者 surface 丢失（Lost/Outdated）。
