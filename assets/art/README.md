# assets/art

`rogue` 的美术源文件和导出结果。`.aseprite` 是二进制源文件（走 Git LFS），这份 README 是它的文本说明：**改图之前先读这里**，改完同步更新。

| 文件 | 说明 |
|---|---|
| `tiles.aseprite` | 源文件：16×16，indexed，4 帧，每帧一个 tag |
| `tiles.png` | 导出的 sprite sheet，64×16，横排，索引色 PNG（走 LFS） |
| `tiles.json` | 导出的数据，json-array 格式，带 `frameTags` |

`.ase-mcp/` 是 aseprite 的 MCP（`ase-mcp`）存放 undo 快照和临时文件的私有目录，不属于美术资源。

## 瓦片

所有瓦片都是俯视、像素风，末日废土。可读性第一：原始尺寸下也要一眼分得清。

| 帧（1 起） | tag | JSON 里的 `from`/`to` | 内容 | 背景 |
|---|---|---|---|---|
| 1 | `floor` | 0 | 地面：暗灰的水泥或泥地，基色上随机撒暗点和亮点。同色的点互不相邻（上下左右，跨瓦片边缘也算），所以铺开后没有接缝，也看不出成团的图案在重复 | 不透明 |
| 2 | `wall` | 1 | 墙顶：浅灰的砖，错缝砌法。每块砖 8×4（含灰缝），第一行是高光，灰缝更暗；另外有几个磨损的暗点。图案周期是 16，所以铺开没有接缝 | 不透明 |
| 3 | `player` | 2 | 玩家：正面偏俯视的小人。深棕头发、肤色的脸、蓝外套（中间一条浅蓝的拉链）、棕色裤子，外面一圈深色轮廓；左右对称，比较紧凑 | 透明 |
| 4 | `zombie` | 3 | 僵尸：灰绿的皮肤、红眼睛、张着嘴，头往右偏一格；左臂向左平伸到瓦片边缘，右手下垂；破烂的灰褐衣服上有血迹，两条腿一粗一细。轮廓不对称，和玩家一眼就能分开 | 透明 |

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
- 两个人物是手画的字符图。

要改图，直接改下面的字符串，重跑这两步，再导出。

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
}

local spr = app.sprite
local layer = spr.layers[1]
-- Frames 2..4 start empty; every frame gets a sprite-sized cel at (0, 0).
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

导出结果是确定的：同一个源文件导出两次，PNG 和 JSON 逐字节相同。导出后检查 `tiles.json` 应该满足：
- `frames` 有 4 项，都是 16×16，`trimmed` 都是 false；
- `meta.frameTags` 依次是 `floor`、`wall`、`player`、`zombie`，`from == to`，分别是 0 到 3；
- `meta.format` 是 `I8`；
- `meta.size` 是 64×16。
