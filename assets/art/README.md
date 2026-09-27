# assets/art

`rogue` 和 `souls` 的美术源文件和导出结果。`.aseprite` 是二进制源文件（走 Git LFS），这份 README 是它的文本说明：**改图之前先读这里**，改完同步更新。

| 文件 | 说明 |
|---|---|
| `tiles.aseprite` | `rogue` 的瓦片源文件：16×16，indexed，5 帧，每帧一个 tag |
| `tiles.png` | 导出的 sprite sheet，80×16，横排，索引色 PNG（走 LFS） |
| `tiles.json` | 导出的数据，json-array 格式，带 `frameTags` |
| `souls_hud.aseprite` | `souls` 的 HUD 图标源文件：16×16，RGB，2 帧，每帧一个 tag（见下文「souls 的 HUD 图标」） |
| `souls_hud.png` | 导出的 sprite sheet，32×16，横排，RGBA PNG（走 LFS） |
| `souls_hud.json` | 导出的数据，json-array 格式，带 `frameTags` |

HUD 的像素字体不在这里，在 `assets/fonts/`，说明见那里的 README。

`.ase-mcp/` 是 aseprite 的 MCP（`ase-mcp`）存放 undo 快照和临时文件的私有目录，不属于美术资源。

## 瓦片（tiles）

所有瓦片都是俯视、像素风，末日废土。可读性第一：原始尺寸下也要一眼分得清。

| 帧（1 起） | tag | JSON 里的 `from`/`to` | 内容 | 背景 |
|---|---|---|---|---|
| 1 | `floor` | 0 | 地面：暗灰的水泥或泥地，基色上随机撒暗点和亮点。同色的点互不相邻（上下左右，跨瓦片边缘也算），所以铺开后没有接缝，也看不出成团的图案在重复 | 不透明 |
| 2 | `wall` | 1 | 墙顶：浅灰的砖，错缝砌法。每块砖 8×4（含灰缝），第一行是高光，灰缝更暗；另外有几个磨损的暗点。图案周期是 16，所以铺开没有接缝 | 不透明 |
| 3 | `player` | 2 | 玩家：正面偏俯视的小人。深棕头发、肤色的脸、蓝外套（中间一条浅蓝的拉链）、棕色裤子，外面一圈深色轮廓；左右对称，比较紧凑 | 透明 |
| 4 | `zombie` | 3 | 僵尸：灰绿的皮肤、红眼睛、张着嘴，头往右偏一格；左臂向左平伸到瓦片边缘，右手下垂；破烂的灰褐衣服上有血迹，两条腿一粗一细。轮廓不对称，和玩家一眼就能分开 | 透明 |
| 5 | `corpse` | 4 | 尸体（E4.4）：`zombie` 那一帧逆时针转 90 度，也就是横躺着，头在左边、脚在右边，原来平伸的左臂现在垂向下方；补了几处转过来之后断开的轮廓。手臂右边有一小滩深红的血（索引 15）。和站着的僵尸同样的配色，一眼能认出是它，但方向完全不同 | 透明 |

## 调色板

16 色，索引 0 是透明色（indexed 模式的 transparent color）。

| 索引 | 颜色 | 用途 |
|---|---|---|
| 0 | `#00000000` | 透明 |
| 1 | `#16161a` | 轮廓（人物），眼睛 |
| 2 | `#34332f` | 地面：暗点 |
| 3 | `#44423d` | 地面：基色 |
| 4 | `#524f48` | 地面：亮点 |
| 5 | `#7a7468` | 墙：灰缝、磨损 |
| 6 | `#9c9586` | 墙：砖 |
| 7 | `#bdb5a4` | 墙：砖的高光 |
| 8 | `#2c5aa8` | 玩家：外套 |
| 9 | `#5b8fe0` | 玩家：外套高光、拉链 |
| 10 | `#d9a57f` | 玩家：皮肤 |
| 11 | `#4a3222` | 玩家：头发、裤子 |
| 12 | `#56703f` | 僵尸：暗部皮肤 |
| 13 | `#86a15e` | 僵尸：皮肤 |
| 14 | `#6e6652` | 僵尸：衣服 |
| 15 | `#8a2a24` | 僵尸：眼睛、血迹 |

## 怎么画出来的

用 aseprite 的 MCP，两次调用就能从零重建 `tiles.aseprite`：

1. `create_sprite`：`file: "tiles.aseprite"`，`width: 16`，`height: 16`，`color_mode: "indexed"`，`palette` 是上表 16 色，按索引顺序排列。已有文件时加 `overwrite: true`。
2. `run_lua`：`file: "tiles.aseprite"`，代码见下文。

像素数据是写死在 Lua 里的字符串，一行一个字符串，一个十六进制字符对应一个调色板索引，`.` 表示索引 0，所以重建的结果完全确定。其中：
- 地面的数据是用固定种子（4101）的随机数生成的：每个像素 12% 概率是暗点、10% 概率是亮点；然后按从左到右、从上到下的顺序，去掉和左边或上边同色的点（跨边缘也算）；
- 墙按砖的规则生成；
- 两个人物是手画的字符图；
- 尸体是僵尸的字符图逆时针转 90 度，再手工补轮廓、加血迹。

要改图，直接改下面的字符串，重跑这两步，再导出。

注意：在已有的源文件末尾用 `spr:newEmptyFrame()` 加帧时，Aseprite 会把最后一个 tag 自动延长到新帧上（E4.4 加尸体帧时遇到过：`zombie` 从 4–4 变成了 4–5），要手动把它改回来。下面的 Lua 是先建好全部帧再加 tag，从零重建不受影响。

```lua
-- One string per row, one hex digit per palette index; '.' is index 0.
local tiles = {
  { name = "floor", rows = {
    "3333333334333342",
    "3333333433333333",
    "3332323334233333",
    "3333343233332333",
    "3333333343333334",
    "3333342333333323",
    "2433323333333333",
    "3343333243323434",
    "3333333323333333",
    "3334323333333333",
    "3333342334333434",
    "4234334333332332",
    "3333333333333323",
    "3233333333433332",
    "3323343233232333",
    "3333332333343323",
  } },
  { name = "wall", rows = {
    "7777777577777775",
    "6656666566666665",
    "6666666566666665",
    "5555555555555555",
    "7775777777757777",
    "6665666666656666",
    "6665666666556666",
    "5555555555555555",
    "7777777577777775",
    "6666656566666665",
    "6666666566666665",
    "5555555555555555",
    "7775777777757777",
    "6665666666656566",
    "6665666665656666",
    "5555555555555555",
  } },
  { name = "player", rows = {
    "................",
    ".....111111.....",
    "....1bbbbbb1....",
    "....1baaaab1....",
    "....1a1aa1a1....",
    "....1aaaaaa1....",
    "...1198888911...",
    "..198888888891..",
    "..198889988891..",
    "..198889988891..",
    "..1a18899881a1..",
    "...11bbbbbb11...",
    "....1bb11bb1....",
    ".....11..11.....",
    "................",
    "................",
  } },
  { name = "zombie", rows = {
    "................",
    "......111111....",
    ".....1cccccc1...",
    ".....1dddddc1...",
    ".....1dfddfd1...",
    ".....1dd11dd1...",
    "......1cccc1....",
    ".11111eeeeee1...",
    "1dddceefeeeee11.",
    ".1111eeeeeeedc1.",
    ".....1eeefe1cc1.",
    ".....1ec1eee111.",
    ".....1ee1.1e1...",
    ".....1cc1.1c1...",
    ".....11...11....",
    "................",
  } },
  { name = "corpse", rows = {
    "................",
    "........1111....",
    "........1cc1....",
    "..1111.1edc111..",
    ".1ccdd1eee1eec1.",
    ".1cdfdceeeee111.",
    ".1cdd1ceeefe1...",
    ".1cdd1ceeee111..",
    ".1cdfdcefeecec1.",
    ".1cddd1eeeeeec1.",
    "..1111.1ee11111.",
    ".......1c1.ff...",
    ".......1d1ffff..",
    ".......1d1fffff.",
    ".......1d1.fff..",
    "........1.......",
  } },
}

local spr = app.sprite
local layer = spr.layers[1]
-- Frames after the first start empty; every frame gets a sprite-sized cel at (0, 0).
while #spr.frames < #tiles do spr:newEmptyFrame() end
for i, tile in ipairs(tiles) do
  local cel = layer:cel(i) or spr:newCel(layer, i)
  local img = Image(spr.width, spr.height, spr.colorMode)
  for y, row in ipairs(tile.rows) do
    for x = 1, #row do
      local ch = row:sub(x, x)
      img:drawPixel(x - 1, y - 1, ch == "." and 0 or tonumber(ch, 16))
    end
  end
  cel.image = img
  cel.position = Point(0, 0)
  local tag = spr:newTag(i, i)
  tag.name = tile.name
end
```

## 怎么重新导出

aseprite 的 MCP `export_sheet`：

```
file: "tiles.aseprite"
png: "tiles.png"
data: "tiles.json"
data_format: "json-array"
sheet_type: "horizontal"
list_tags: true
```

导出结果是确定的：同一个源文件导出两次，PNG 和 JSON 逐字节相同。aseprite 写出的 JSON 用 CRLF 换行，仓库按 `.gitattributes` 规范成 LF，引擎读的时候不依赖换行符。导出后检查 `tiles.json` 应该满足：
- `frames` 有 5 项，都是 16×16，`trimmed` 都是 false；
- `meta.frameTags` 依次是 `floor`、`wall`、`player`、`zombie`、`corpse`，`from == to`，分别是 0 到 4；
- `meta.format` 是 `I8`；
- `meta.size` 是 80×16。

## souls 的 HUD 图标（souls_hud）

`souls` 的 HUD 在屏幕下方画这两个图标（`docs/architecture.md` 第 11 节「HUD」的「文字和图标」）。游戏在 `hud_sheets()` 里声明它们，引擎在场景加载时读进来，并检查两个 tag 都存在。HUD 按 `scale` 2 画，所以屏幕上是 32×32 个 HUD 单位。

| 帧（1 起） | tag | JSON 里的 `from`/`to` | 内容 | 背景 |
|---|---|---|---|---|
| 1 | `flask` | 0 | 原素瓶：深棕轮廓；瓶口是棕色软木塞，下面一道横边；细瓶颈往下展开成圆肚。瓶身是金色玻璃，左边一道高光，右边一道暗金；下半部装着橙色的原素。颜色和场景里瓶子的 `(0.9, 0.7, 0.2)` 相近。瓶数为 0 时，游戏把它乘上 `(0.35, 0.35, 0.35)` 画成暗色，不另画一帧 | 透明 |
| 2 | `soul` | 1 | 魂：一团往上收尖的光，形状像水滴。从外到内依次是蓝边、浅蓝、淡蓝白，中心是纯白 | 透明 |

两帧都是左右大致对称。在原始 16×16 下，在浅灰地面和深色天空上都分得清。

### 颜色

RGB 模式，所以没有调色板。只用到下面这些不透明色，其余像素全透明（alpha 0）。字母是下文 Lua 字符图里的记号：

| 字母 | 颜色 | 用途 |
|---|---|---|
| `a` | `#3b2412` | 瓶：轮廓 |
| `b` | `#9a6a36` | 瓶：软木塞 |
| `c` | `#fff6cc` | 瓶：玻璃高光 |
| `d` | `#f3da7c` | 瓶：金色玻璃 |
| `e` | `#b8862e` | 瓶：暗金 |
| `f` | `#ff9f2e` | 瓶：原素 |
| `g` | `#4f8fd0` | 魂：外边 |
| `h` | `#8cc8f0` | 魂：浅蓝 |
| `i` | `#cdeeff` | 魂：淡蓝白 |
| `j` | `#ffffff` | 魂：中心 |

### 怎么画出来的

用 aseprite 的 MCP，两次调用就能从零重建 `souls_hud.aseprite`：

1. `create_sprite`：`file: "souls_hud.aseprite"`，`width: 16`，`height: 16`，`color_mode: "rgb"`，不传 `palette`。已有文件时加 `overwrite: true`。
2. `run_lua`：`file: "souls_hud.aseprite"`，代码见下文。

像素写成字符串，一行一个，一个字母对应上表的一个颜色，`.` 是透明。两帧都是手画的字符图。E6.2 核对过：按这两步重建，再按下文导出，得到的 PNG 和 JSON 与仓库里的逐字节相同。

```lua
-- One string per row, one letter per color; '.' is transparent.
local pc = app.pixelColor
local colors = {
  a = pc.rgba(59, 36, 18, 255),    -- flask: outline
  b = pc.rgba(154, 106, 54, 255),  -- flask: cork
  c = pc.rgba(255, 246, 204, 255), -- flask: glass highlight
  d = pc.rgba(243, 218, 124, 255), -- flask: gold glass
  e = pc.rgba(184, 134, 46, 255),  -- flask: dark gold
  f = pc.rgba(255, 159, 46, 255),  -- flask: estus
  g = pc.rgba(79, 143, 208, 255),  -- soul: edge
  h = pc.rgba(140, 200, 240, 255), -- soul: blue
  i = pc.rgba(205, 238, 255, 255), -- soul: pale
  j = pc.rgba(255, 255, 255, 255), -- soul: core
}
local icons = {
  { name = "flask", rows = {
    "......aaaa......",
    "......abba......",
    ".....aaaaaa.....",
    "......acda......",
    "......acda......",
    ".....acddea.....",
    "....acddddea....",
    "...accdddddea...",
    "..acffffffffea..",
    "..acffffffffea..",
    "..adffffffffea..",
    "..adffffffffea..",
    "...adffffffea...",
    "....addddeea....",
    ".....aaaaaa.....",
    "................",
  } },
  { name = "soul", rows = {
    "................",
    ".......g........",
    "......gh........",
    "......ghg.......",
    ".....ghihg......",
    ".....ghiihg.....",
    "....ghiiiihg....",
    "...ghiijjiihg...",
    "..ghiijjjjiihg..",
    "..ghijjjjjjihg..",
    "..ghijjjjjjihg..",
    "..ghiijjjjiihg..",
    "...ghiijjiihg...",
    "....ghhiihhg....",
    ".....gghhgg.....",
    "......gggg......",
  } },
}

local spr = app.sprite
local layer = spr.layers[1]
while #spr.frames < #icons do spr:newEmptyFrame() end
for i, icon in ipairs(icons) do
  local cel = layer:cel(i) or spr:newCel(layer, i)
  local img = Image(spr.width, spr.height, spr.colorMode)
  for y, row in ipairs(icon.rows) do
    for x = 1, #row do
      local ch = row:sub(x, x)
      if ch ~= "." then img:drawPixel(x - 1, y - 1, colors[ch]) end
    end
  end
  cel.image = img
  cel.position = Point(0, 0)
  local tag = spr:newTag(i, i)
  tag.name = icon.name
end
```

和瓦片一样，先建好全部帧再加 tag，所以不会遇到 tag 被自动延长的问题。

### 怎么重新导出

aseprite 的 MCP `export_sheet`：

```
file: "souls_hud.aseprite"
png: "souls_hud.png"
data: "souls_hud.json"
data_format: "json-array"
sheet_type: "horizontal"
list_tags: true
```

`sheet_type` 和 `list_tags` 用的都是默认值，写出来是为了明确。导出后检查 `souls_hud.json`：
- `frames` 有 2 项，都是 16×16，`trimmed` 都是 false，`frame` 依次是 `(0, 0)` 和 `(16, 0)`；
- `meta.frameTags` 依次是 `flask`（0–0）和 `soul`（1–1）；
- `meta.image` 是相对文件名 `"souls_hud.png"`，不能是绝对路径；
- `meta.format` 是 `RGBA8888`；
- `meta.size` 是 32×16。

改了 tag 名，`games/souls/src/hud.rs` 里的 `FLASK_ICON`、`SOUL_ICON` 要一起改，否则场景加载时会报 `HUD sheet assets/art/souls_hud.json has no tag …`。
