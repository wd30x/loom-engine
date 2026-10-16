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
| `humanoid/stagger.ron` | 30 tick | 否 | 硬直：3 被打得上身后仰、手臂甩开；8 右脚往后退半步、膝盖发软；15 稳住；22 回到戒备（剑竖在胸前），脚停在退到的位置；30 站稳。游戏还会把节点倾斜 20°，片段自己不整体倾斜 |
| `humanoid/death.ron` | 40 tick | 否 | 死亡：6 手臂松开、剑已经转到贴着身体、头开始偏向一边；16 瘫软；30、40 躺着：四肢稍微张开，头偏向一边，剑沿着身体指向脚。游戏在死亡那个 tick 就把节点放倒成仰躺或俯卧，所以片段从第一个关键帧起，四肢都只在身体所在的平面附近动，不屈膝、不弯腰（E7.5 审查时定） |
| `humanoid/drink.ron` | 60 tick | 否 | 喝原素瓶：10、20 左手把瓶子往上举；30 瓶口正对面甲的缝；38、45 仰头，瓶子倾向嘴边；52 放下；60 站立。右手的剑一直垂在身侧。瓶子是挂在 `hand_l` 上的节点，不在片段里；各关键帧瓶口的位置写在文件开头的注释里 |
| `humanoid/sit.ron` | 40 tick | 否 | 篝火坐下：10 下蹲，脚不动；20 蹲到最低，前脚掌着地；30 盘腿坐下（左小腿在右小腿前面），剑横放在腿上，左手搭在左膝；40 坐定，头微微低下。片段自己把髋部放低 0.74 m，到贴近地面，游戏不再降低节点 |

每个循环片段的每个通道在 tick 0 和 `length` 写同样的值，循环时首尾相接。

### 片段按默认参数写

souls 的动作片段按**默认参数**的长度写（`docs/architecture.md` 第 11 节「模型和动画」）。参数改了，游戏用播放速度把每一段拉长或压短，片段本身不用改：

| 片段 | 分段（tick） | 对应的默认参数 |
|---|---|---|
| `roll` | 0–32 | `roll_ticks` 32 |
| `backstep` | 0–20 | `backstep_ticks` 20 |
| `attack1`–`attack3` | 前摇 0–10、出招 10–14、后摇 14–30 | `attack_startup_ticks` 10、`attack_active_ticks` 4、`attack_recovery_ticks` 16 |
| `heavy` | 前摇 0–22、出招 22–27、后摇 27–50 | `heavy_startup_ticks` 22、`heavy_active_ticks` 5、`heavy_recovery_ticks` 23 |
| `stagger` | 0–30 | `stagger_ticks` 30 |
| `drink` | 举瓶 0–30、喝和放下 30–60 | `flask_heal_tick` 30、`flask_ticks` 60（第二段 30 tick 是两者之差） |
| `death`、`sit` | 0–40 | 不随参数伸缩，按速度 100 播完后停在最后一格 |
| `run` 系列 | 整个循环 | `RUN_CLIP_SPEED` 4 m/s |
| `sprint` | 整个循环 | `SPRINT_CLIP_SPEED` 6 m/s |

攻击片段的分界 tick（10、14；22、27）和 `drink` 的 30 上都有关键帧，每一段单独拉伸时，分界处的姿势不变。四个攻击的出招都是剑从角色右边扫到左边（命中判定是从右往左扫过的扇形），出招各帧剑尖的高度见各文件开头的注释。

### 怎么写的

- 移动片段的腿用两段骨骼的 IK 算：着地期间每 2 tick 一个关键帧，让脚踝相对地面不动（身体按片段速度前进）。按引擎的规则（偏移绕骨骼自己的位置、按模型的轴转）做正向运动学核对，着地期间脚踝漂移都不超过 0.2 cm。
- 攻击、戒备的手臂和剑也用数值 IK：给定握柄的位置和剑的方向，解出右臂和 `weapon` 的偏移；重攻击的左手解到握柄下方。
- 这些计算只是写片段时的草稿工具，没有进仓库；片段文件本身才是源头，改的时候直接改 RON。

预览（不跑游戏，原地播放）：

```
loom view --scene assets/scenes/knight_moves.ron --ticks 0,10,20,30 --size 720x480 --screenshot out/knight_moves.png
loom view --scene assets/scenes/knight_attacks.ron --ticks 6,12,20,25,40 --size 720x480 --screenshot out/knight_attacks.png
loom view --scene assets/scenes/knight_rest.ron --ticks 3,10,20,30,45,60 --size 720x400 --screenshot out/knight_rest.png
```

后两张分别是 smoke H15、H16。`knight_rest` 里硬直和死亡的骑士按游戏里被正面打中的样子摆放：向后倾斜 20°、仰面躺着并抬高 0.13 m；喝瓶的骑士左手挂着瓶子。

## boss 的片段

`assets/models/boss.glb` 用（E7.6 起，`docs/architecture.md` 第 11 节「Boss 的模型和动画」）。骨骼和 humanoid 相同，上面「各骨骼的转向」表照样适用。每个文件开头有英文注释，写了各关键帧的姿势。

| 文件 | 长度 | 循环 | 内容 |
|---|---|---|---|
| `boss/idle.ron` | 120 tick | 是 | 站立：两脚分开、左脚在前、膝盖微屈，戟竖在右手里，戟尾离地；呼吸：tick 60 胸口抬 1.5°、hips 下沉 12 mm |
| `boss/walk.ron` | 48 tick | 是 | 走近，按 2.5 m/s 写（`WALK_CLIP_SPEED`）：一个循环两步，每步 1 m。每只脚着地 55% 的时间，从髋前 0.55 m 到髋后 0.55 m，每 2 tick 一个关键帧让脚贴地不动；抬腿最高 16 cm。身体前倾，左臂和腿反向摆，右手的戟竖着轻摆 |
| `boss/sweep.ron` | 68 tick | 否 | 横扫：前摇里双手举戟、刃转到右后方；出招 30–38 从右往左平扫过身前，左脚跨入；后摇收回 |
| `boss/cleave.ron` | 89 tick | 否 | 下劈：前摇里把戟高举到脑后、身体后仰；出招 45–49 从头顶劈到身前，左脚跨入、弯腰；49 时戟尖贴近前方地面 |
| `boss/lunge.ron` | 80 tick | 否 | 突进：前摇里下蹲、双手把戟平端如枪，右脚往后蓄力；出招 35–45 刺出，腿成长弓步，枪尖从右扫到左。原地写，游戏移动节点 |
| `boss/combo.ron` | 75 tick | 否 | 连击第 1 刀：25 举到右肩上方，26–30 从右肩斜劈到左下 |
| `boss/combo_even.ron` | 62 tick | 否 | 连击偶数刀：从左下开始，12 蓄在左下，13–17 往右上撩 |
| `boss/combo_odd.ron` | 62 tick | 否 | 连击第 3 刀以后的奇数刀：从右上开始，12 蓄到右肩上方，13–17 斜劈到左下，同 `combo` |
| `boss/spin.ron` | 102 tick | 否 | 回旋斩（E7.7 起，只在二阶段出）：22 平端起戟、往右转；50 蓄力：身体转向右边、再往右拧，双手平端的戟伸向身后；51–62 每 tick 一个关键帧，整个身体靠 `root` 原地向左转一整圈，每格 30°，戟伸在右边，从身后经右、前、左扫回身后；75 余势再转 50°；90 回正、放低戟；102 站立 |
| `boss/roar.ron` | 70 tick | 否 | 咆哮（E7.7 起，进入二阶段时）：14 蓄力：下蹲、前俯、低头，戟收到身前、左拳在胸前；17、19 挺身；20 冲击波：直起后仰、仰头，两臂向上张开成 V 字，戟高举在右边（戟尖约 4.2 m）；32–54 保持咆哮、微微摇晃；62 放下手臂；70 站立 |
| `boss/stagger.ron` | 30 tick | 否 | 硬直：3 被打得后仰、手臂甩开；8 右脚退半步；22 回到戒备；30 站稳 |
| `boss/death.ron` | 40 tick | 否 | 死亡：四肢只在身体平面里动，戟平放在右腿旁、指向脚；30、40 躺着，四肢稍微张开，头偏向一边。游戏把 `body` 抬高 `enemy_dead_lift`（`boss.ron` 写 0.3 m，戟离身体平面最远 0.16 m） |

**按默认参数写**：

| 片段 | 分段（tick） | 对应的默认参数 |
|---|---|---|
| `sweep` | 前摇 0–30、出招 30–38、后摇 38–68 | `sweep_startup_ticks` 30、`sweep_active_ticks` 8、`sweep_recovery_ticks` 30 |
| `cleave` | 0–45、45–49、49–89 | `cleave_*` 45、4、40 |
| `lunge` | 0–35、35–45、45–80 | `lunge_*` 35、10、35 |
| `combo` | 0–25、25–30、30–75 | `combo_*` 25、5、45 |
| `combo_even`、`combo_odd` | 刀前的间隔 0–12、出招 12–17、后摇 17–62 | `combo_gap_ticks` 12，`combo_active_ticks` 5、`combo_recovery_ticks` 45 |
| `spin` | 0–50、50–62、62–102 | `spin_*` 50、12、40 |
| `roar` | 冲击波前 0–20、冲击波后 20–70 | `roar_blast_tick` 20、`roar_ticks` 70 |
| `stagger` | 0–30 | `stagger_ticks` 30 |
| `death` | 0–40 | 不随参数伸缩 |
| `walk` | 整个循环 | `WALK_CLIP_SPEED` 2.5 m/s |

- 分界 tick 上都有关键帧；出招期间每 tick 一个关键帧。招式的出招都从右往左扫（`combo_even` 从左往右，`spin` 是向左转一整圈），和命中判定的扫掠方向一致。
- 连击首尾相接：`combo` 第 30 格 = `combo_even` 第 0 格，`combo_even` 第 17 格 = `combo_odd` 第 0 格，`combo_odd` 第 12 格 = `combo` 第 25 格。

**怎么写的**：同 humanoid，用没进仓库的草稿工具（按引擎的偏移规则做正向运动学、两段骨骼 IK）算出关键帧，片段文件本身是源头。

预览（smoke H17，左到右：sweep、cleave、lunge、combo 第 1 刀；H18，左到右：roar、spin，节点是二阶段的暗红）：

```
loom view --scene assets/scenes/boss_moves.ron --ticks 20,30,35,40,47,60 --size 800x420 --screenshot out/boss_moves.png
loom view --scene assets/scenes/boss_phase2_moves.ron --ticks 14,20,40,50,53,56,59,90 --size 600x420 --screenshot out/boss_phase2_moves.png
```

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
