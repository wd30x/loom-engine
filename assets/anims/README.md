# assets/anims

动画片段：RON 文本，一个文件一个片段，放在 `assets/anims/<模型名>/<片段名>.ron`。格式和规则见 `docs/architecture.md` 第 19 节「骨骼和动画片段」：
- 关键帧的时间是 tick（1/60 秒）；
- 旋转是 YXZ 欧拉角，单位度；
- 值是**相对绑定姿势的偏移，按模型的轴**（+X 右，+Y 上，-Z 前），绕骨骼自己的位置转；
- 结构见 `loom schema` 的 `clip`。

预览用 `assets/scenes/anim_preview.ron`（左边 idle，右边 walk），拍一张胶片图（smoke H13）：

```
loom view --scene assets/scenes/anim_preview.ron --ticks 0,15,30,45 --size 480x360 --screenshot out/anim_preview.png
```

## humanoid：各骨骼的转向

`assets/models/humanoid.glb` 的骨骼（见 `assets/models/README.md`）。下表是「绕模型哪根轴、转正角还是负角」对应什么动作，写动作片段时查。

「左」「右」是骑士自己的左右：右手（持剑）在 +X。

| 骨骼 | 偏移 | 动作 |
|---|---|---|
| `upper_leg_l`、`upper_leg_r` | 绕 X 正角 | 大腿往前抬；负角往后摆 |
| `upper_leg_l` | 绕 Z 负角 | 左腿向外（左）张开 |
| `upper_leg_r` | 绕 Z 正角 | 右腿向外（右）张开 |
| `lower_leg_l`、`lower_leg_r` | 绕 X 负角 | 屈膝：小腿向后弯 |
| `foot_l`、`foot_r` | 绕 X 正角 | 脚尖翘起；负角脚尖朝下 |
| `upper_arm_l`、`upper_arm_r` | 绕 X 正角 | 手臂往前抬；负角往后摆 |
| `upper_arm_l` | 绕 Z 正角 | 左臂往里收，贴近身体；负角向外抬 |
| `upper_arm_r` | 绕 Z 负角 | 右臂往里收，贴近身体；正角向外抬 |
| `lower_arm_l`、`lower_arm_r` | 绕 X 正角 | 屈肘：前臂向前抬 |
| `spine`、`chest` | 绕 X 正角 | 上身后仰；负角前俯 |
| `spine`、`chest` | 绕 Y 正角 | 上身向左转 |
| `head` | 绕 X 负角 | 低头；正角抬头 |
| `head` | 绕 Y 正角 | 向左转头 |
| `hips` | 平移 Y 负值 | 整个人下沉（腿也跟着，是 hips 的子骨骼） |
| `hips` | 绕 Z 正角 | 整个人向左倒（连腿） |
| `weapon` | 绕 X 正角 | 剑尖往前、往上抬（绕握柄） |

**怎么核对的**（E7.2，E7.3 补拍两行）：
1. 做一个核对用的片段：每 10 tick 只有一根骨骼偏离绑定姿势，偏移的值就是上表各行，前后各 4 tick 回到 0。
2. 从正前方和正左侧各拍一张胶片图，逐帧看动作方向：
   ```
   loom view --scene <场景> --ticks 0,10,20,…,160 --size 240x400 --screenshot …
   ```
3. 核对结果：表里每一行都实拍核对过。
   - E7.2 拍了 16 个动作；`spine`、`chest` 两行里，`chest` 绕 X、`spine` 绕 X 和 Y 各拍了一次。
   - E7.3 补拍了当时推出来的两项：`upper_leg_r` 绕 Z 正角 20°（正面看右腿向外张开），`chest` 绕 Y 正角 30°（从前上方俯视，右肩转向前、左肩转向后，即向左转）。
   - 以后改了模型的骨骼方向也不影响这张表：偏移按模型的轴算，和骨骼自己的轴无关。

## humanoid 的片段

| 文件 | 长度 | 循环 | 内容 |
|---|---|---|---|
| `humanoid/idle.ron` | 120 tick（2 秒） | 是 | 站立呼吸：胸口后仰 2° 再回来，头反向 1.5° 让视线保持水平；hips 上下 6 mm；两臂往前、往里微摆（左臂 2°/3°，持剑的右臂 1°/2°）；剑尖最低约离地 0.3 m |
| `humanoid/walk.ron` | 60 tick（1 秒） | 是 | 原地走一个完整步态，节点不动：<br>• 大腿前后 ±25°；<br>• 膝盖在后摆和收腿时弯曲，最大 45°；<br>• 脚在前时接近水平，在后时脚尖朝下约 45°（蹬地）；<br>• hips 在两腿分开时下沉 5 cm，交错时回到 -1 cm，一个循环起伏两次；<br>• 两臂和腿反向摆：左臂 ±20°，持剑的右臂 ±10°。<br>tick 0 左腿在前，tick 30 右腿在前 |

| `humanoid/run.ron` | 40 tick | 是 | 往前跑，按 4 m/s 写（`RUN_CLIP_SPEED`）：一个循环两步，每步 20 tick、1.33 m，一秒 3 步。每只脚着地 8 tick（左脚 0–8，右脚 20–28），从髋前 0.27 m 落地、到髋后 0.27 m 蹬离，其余时间双脚离地。身体前倾 12°，着地中段最低（下沉 6 cm），腾空最高。手臂和腿反向摆，持剑的右臂摆得少，剑朝前下方 |
| `humanoid/sprint.ron` | 36 tick | 是 | 冲刺，按 6 m/s 写（`SPRINT_CLIP_SPEED`）：每步 18 tick、1.8 m，着地 6 tick，前后各 0.3 m。前倾 20°，手臂摆得更大，剑拖在手后方偏下 |
| `humanoid/run_back.ron` | 40 tick | 是 | 锁定时往后跑，按 4 m/s 写：步子和 `run` 一样长，脚在髋后落地（前脚掌先着地）、在髋前蹬离。上身直立，持剑戒备（剑竖在胸前，左手在旁边） |
| `humanoid/run_left.ron`、`run_right.ron` | 30 tick | 是 | 锁定时横移，按 4 m/s 写：每步 15 tick、1 m，每只脚着地 6 tick，在髋下横扫 0.4 m；朝移动方向那只脚比另一只靠前 7 cm，两腿交叉而过。上身朝前，持剑戒备，略向移动方向倾 |
| `humanoid/roll.ron` | 32 tick | 否 | 前滚翻：0 站立；2 下蹲；4 俯冲；8–20 团身，整个身体绕团身的中心向前转一圈（`root` 绕 X 转，同时平移，让转动中心保持不动；中心的高度让身体最低点贴地），这时剑横着伸向右边，沿着转轴，不会扎进地里；24 蹲着落地；28 起身；32 站立 |
| `humanoid/backstep.ron` | 20 tick | 否 | 后撤步：4 下蹲蓄力、举剑戒备；8 腾空，上身后仰，脚往前甩；12 前脚掌落地、屈膝缓冲；16 站稳；20 站立，剑往下放 |
| `humanoid/attack1.ron` | 30 tick | 否 | 第 1 刀：右上到左下的斜劈 |
| `humanoid/attack2.ron` | 30 tick | 否 | 第 2 刀：齐腰从右往左平扫 |
| `humanoid/attack3.ron` | 30 tick | 否 | 第 3 刀：从右肩上方往左下的大斜劈，左脚往前跨一步 |
| `humanoid/heavy.ron` | 50 tick | 否 | 重攻击：双手握剑（左手 10–35 握在右手下面），前摇里举到右肩后上方、身体后仰蓄力，出招时从右上往左下重劈，左脚跨出，后摇较长 |

每个循环片段的每个通道在 tick 0 和 `length` 写同样的值，循环时首尾相接。

### 片段按默认参数写

souls 的动作片段按**默认参数**的长度写（`docs/architecture.md` 第 11 节「模型和动画」）。参数改了，游戏用播放速度把每一段拉长或压短，片段本身不用改：

| 片段 | 分段（tick） | 对应的默认参数 |
|---|---|---|
| `roll` | 0–32 | `roll_ticks` 32 |
| `backstep` | 0–20 | `backstep_ticks` 20 |
| `attack1`–`attack3` | 前摇 0–10、出招 10–14、后摇 14–30 | `attack_startup_ticks` 10、`attack_active_ticks` 4、`attack_recovery_ticks` 16 |
| `heavy` | 前摇 0–22、出招 22–27、后摇 27–50 | `heavy_startup_ticks` 22、`heavy_active_ticks` 5、`heavy_recovery_ticks` 23 |
| `run` 系列 | 整个循环 | `RUN_CLIP_SPEED` 4 m/s |
| `sprint` | 整个循环 | `SPRINT_CLIP_SPEED` 6 m/s |

攻击片段的分界 tick（10、14；22、27）上都有关键帧，每一段单独拉伸时，分界处的姿势不变。四个攻击的出招都是剑从角色右边扫到左边（命中判定是从右往左扫过的扇形），出招各帧剑尖的高度见各文件开头的注释。

### 怎么写的

- 移动片段的腿用两段骨骼的 IK 算：着地期间每 2 tick 一个关键帧，让脚踝相对地面不动（身体按片段速度前进）。按引擎的规则（偏移绕骨骼自己的位置、按模型的轴转）做正向运动学核对，着地期间脚踝漂移都不超过 0.2 cm。
- 攻击、戒备的手臂和剑也用数值 IK：给定握柄的位置和剑的方向，解出右臂和 `weapon` 的偏移；重攻击的左手解到握柄下方。
- 这些计算只是写片段时的草稿工具，没有进仓库；片段文件本身才是源头，改的时候直接改 RON。

预览（不跑游戏，原地播放）：

```
loom view --scene assets/scenes/knight_moves.ron --ticks 0,10,20,30 --size 720x480 --screenshot out/knight_moves.png
loom view --scene assets/scenes/knight_attacks.ron --ticks 6,12,20,25,40 --size 720x480 --screenshot out/knight_attacks.png
```

后一张是 smoke H15。

## 从 Blender 的动画转片段

`loom clip <模型.glb> --animation <名字> --out <片段.ron> [--looping]` 把 glTF 里的一个动画按片段格式写成 RON（规则见第 19 节「glTF 动画转片段」）：
- 每个 tick 采样一次；
- 旋转保留 2 位小数，平移保留 4 位；
- 关键帧按 0.1° / 0.5 mm 的精度精简。

`tools/blender/humanoid_wave.py` 是它的测试动画：
- 在 humanoid 上加一个 2 秒的 `wave`：右臂抬起、前臂挥两下、头转向手、hips 移一点，全部是 LINEAR 关键帧；
- 只导出到 `out/`，不进仓库；
- 加 `--render <目录> <tick,…>` 时，Blender 会从 loom 场景的同一个相机位置渲染这几个 tick，方便对照：

```
blender -b --factory-startup --python tools/blender/humanoid_wave.py -- out/humanoid_wave.glb --render out/wave_blender 0,30,60
loom clip out/humanoid_wave.glb --animation wave --out out/wave.ron
```

Blender 的 LINEAR 是按四元数的各个分量分别插值，不是球面插值，所以一段大角度的转动在 loom 片段里会多出一些中间关键帧（精简时按精度自动留下）。
