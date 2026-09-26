# Godot 参考对照

`ref/godot/` 是 Godot **4.7.2-stable** 的只读浅克隆，已 gitignore。重新获取：

```powershell
git clone --depth 1 --branch 4.7.2-stable https://github.com/godotengine/godot.git ref/godot
```

**原则**：学它的设计，不照搬它的规模。Godot 要支持所有游戏类型、所有平台，还要带编辑器；我们只取真实使用者（现在是类魂原型，之后是 2D 示例游戏）用得到的部分。

许可：Godot 本体是 MIT。借鉴思路不需要声明；**移植具体代码**时，要在 `THIRD_PARTY_NOTICES.md` 里附上 Godot 的版权声明（见 `ref/godot/LICENSE.txt` 和 `COPYRIGHT.txt`）。`thirdparty/` 目录下的代码许可证各不相同，不要从里面拷。

## 按里程碑对照

### E0 主循环与时间

| 概念 | 位置 | 看什么 |
|---|---|---|
| 主循环的一次迭代 | `main/main.cpp` 中 `Main::iteration()`（约 4921 行） | 物理 tick 和渲染帧的关系 |
| 固定步长同步 | `main/main_timer_sync.h/.cpp` | 累加器、每帧步数上限、抖动修正 |
| 默认参数 | `main/main.cpp` 约 2252 行 | 60 Hz、每帧最多 8 步，和我们的决策一致 |
| 固定帧率 | `main/main.cpp` 里的 `--fixed-fps` | 关掉实时同步，每帧推进固定时间，这就是我们的无头模式 |
| 录制帧 | `servers/movie_writer/`，`--write-movie` | 逐帧确定性输出 PNG |
| 无头显示 | `servers/display/display_server_headless.h` | 无头模式下哪些功能是空实现 |

### E1 输入

| 概念 | 位置 | 看什么 |
|---|---|---|
| 动作映射 | `core/input/input_map.h/.cpp` | 动作名 → 多个按键事件；死区 |
| 输入事件 | `core/input/input_event.h` | 事件类型的划分 |
| 输入状态 | `core/input/input.h` | `is_action_just_pressed` 这类按帧查询的语义 |

### E2 场景与资源

| 概念 | 位置 | 看什么 |
|---|---|---|
| 节点 | `scene/main/node.h/.cpp` | 父子关系、生命周期（ready / process / physics_process）、分组 |
| 场景树 | `scene/main/scene_tree.h/.cpp` | 每帧怎么遍历节点、延迟删除 |
| 3D 变换节点 | `scene/3d/node_3d.h` | 局部变换和全局变换、脏标记 |
| 打包场景 | `scene/resources/packed_scene.h/.cpp` | 场景怎么序列化，实例化时怎么还原 |
| 文本场景格式 | `scene/resources/resource_format_text.cpp` | `.tscn` 的读写，对我们的 RON 格式很有参考价值 |
| 资源 | `core/io/resource.h`、`core/io/resource_loader.cpp` | 引用计数、按路径缓存、热重载 |

**不要照搬**：Godot 的 `Object`、`ClassDB`、`Variant` 反射体系（`core/object/`）是为编辑器和 GDScript 服务的。Rust 里用 serde 加普通结构体就够了。

### E3 远程控制

| 概念 | 位置 | 看什么 |
|---|---|---|
| 远程调试器 | `core/debugger/remote_debugger.h/.cpp` | 运行中的游戏怎么和外部通信：消息格式、暂停和单步 |
| 调试器接口 | `core/debugger/engine_debugger.h` | 各子系统怎么注册自己的调试命令 |

### 渲染和物理的 Server 架构

| 概念 | 位置 | 看什么 |
|---|---|---|
| 渲染服务器 | `servers/rendering/rendering_server.h` | 场景层通过 RID 句柄操作渲染对象，不直接持有 GPU 资源 |
| 物理服务器 | `servers/physics_3d/physics_server_3d.h` | 同样的句柄模式 |
| 句柄分配 | `core/templates/rid_owner.h` | RID 的分配和回收 |

这种"场景层只拿句柄，服务器持有真实资源"的分离，正好对应我们"`loom_render` 不知道场景和 winit"的分层。

### 游戏阶段（G1 起）

| 概念 | 位置 |
|---|---|
| 角色控制器 | `scene/3d/physics/character_body_3d.h`（`move_and_slide`） |
| 触发区域（hitbox 可参考） | `scene/3d/physics/area_3d.h` |
| 相机 | `scene/3d/camera_3d.h` |
| 动画播放 | `scene/animation/animation_player.h`、`scene/resources/animation.h` |
| 动画状态机和混合 | `scene/animation/animation_tree.h` |
| glTF 导入 | `modules/gltf/gltf_document.cpp` |
| 物理后端 | `modules/jolt_physics/`（Godot 4.4 起内置 Jolt） |
| 寻路 | `modules/navigation_3d/`、`scene/3d/navigation/navigation_agent_3d.h` |
