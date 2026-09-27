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

每个片段的每个通道在 tick 0 和 `length` 写同样的值，循环时首尾相接。

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
