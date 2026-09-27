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
| `upper_leg_r` | 绕 Z 正角 | 右腿向外（右）张开（和左腿镜像，按右手定则推出，没单独拍） |
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

**怎么核对的**（E7.2）：
1. 做一个核对用的片段：每 10 tick 只有一根骨骼偏离绑定姿势，偏移的值就是上表各行，前后各 4 tick 回到 0。
2. 从正前方和正左侧各拍一张胶片图，逐帧看动作方向：
   ```
   loom view --scene <场景> --ticks 0,10,20,…,160 --size 240x400 --screenshot …
   ```
3. 核对结果：
   - 除了 `upper_leg_r` 绕 Z 那一行，都是实拍核对过的；
   - 另外 `chest` 绕 X、`spine` 绕 X 和 Y 各拍了一次，`chest` 绕 Y 按同样的规则推出。
   - 以后改了模型的骨骼方向也不影响这张表：偏移按模型的轴算，和骨骼自己的轴无关。

## humanoid 的片段

| 文件 | 长度 | 循环 | 内容 |
|---|---|---|---|
| `humanoid/idle.ron` | 120 tick（2 秒） | 是 | 站立呼吸：胸口后仰 2° 再回来，头反向 1.5° 让视线保持水平；hips 上下 6 mm；两臂往前、往里微摆（左臂 2°/3°，持剑的右臂 1°/2°）；剑尖最低约离地 0.3 m |
| `humanoid/walk.ron` | 60 tick（1 秒） | 是 | 原地走一个完整步态，节点不动：<br>• 大腿前后 ±25°；<br>• 膝盖在后摆和收腿时弯曲，最大 45°；<br>• 脚在前时接近水平，在后时脚尖朝下约 45°（蹬地）；<br>• hips 在两腿分开时下沉 5 cm，交错时回到 -1 cm，一个循环起伏两次；<br>• 两臂和腿反向摆：左臂 ±20°，持剑的右臂 ±10°。<br>tick 0 左腿在前，tick 30 右腿在前 |

每个片段的每个通道在 tick 0 和 `length` 写同样的值，循环时首尾相接。
