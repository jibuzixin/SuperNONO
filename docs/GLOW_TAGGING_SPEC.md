# NONO 素材辉光标注规范（给素材提取/标注工具）

> 目的：让 **`nono.glow`（逐层）** 和 **`nono.glowShapes`（顶层表）** 两条信息都能真正驱动运行时辉光，
> 所有形态统一。下面第 5 节是这次要改的具体清单，第 6 节是验收标准。

## 1. 运行时现状（App 已实现）

App 在加载每个 Lottie 时生成辉光通道（`crates/nono-render/src/glow.rs`），规则：

1. 逐层 `layer.nono.glow` 是**第一优先级**；有就按它的参数发光。
2. 没有逐层 `nono.glow` 时，用 `layer.nono.shape` 去顶层 `nono.glowShapes` 里查兜底（本次已补上，commit `773a416`）。
3. `baked: true` 的条目不发光——语义是「源素材已经画成描边堆叠/已烘焙」，再叠一次会过曝。
4. 发光颜色取条目里的 `color`（0–255）；如果恰好是纯红 `[255,0,0]`，运行时当前会换成青色
   `[50,254,254]`（形态 3 的源素材色彩变换兼容）。**新素材请直接写最终显示色，不要依赖这个替换。**
5. 逐层 `nono.glow` 的字段：`color`、`blur`/`blurY`、`strength`、`alpha`、`inner`、`compositeSource`、
   `passes`、`pulse`、`baked`。运行时目前用到 `color`、`blur`、`strength`、`baked`，其余字段保留备用。
6. 运行时辉光强度参数（`GlowSettings`）：`sigma_scale=0.6`（模糊半径 = 素材 blur × 0.6）、
   `alpha_scale=1.0`、单动画像素预算 200k、模糊质量 80。若要整体更亮/更糊，改这里即可。

## 2. 这次全形态审计发现的问题

对当前 `assets/`（形态 1–5）逐动画统计（`table` = 顶层 `glowShapes` 条目数，`manual` = 逐层 `nono.glow` 层数，
`match` = 图层的 `nono.shape` 能命中顶层表的 shape 数）：

| 形态 | 动画数 | table | table baked | manual | match |
|---|---|---|---|---|---|
| 1 | 19 | 49 | 40 | 127 | 49 |
| 2 | 19 | 189 | 35 | 657 | 124 |
| 3 | 18 | 135 | 13 | 250 | 127 |
| 4 | 18 | 220 | 51 | 1491 | **8** |
| 5 | 17 | 75 | 66 | 64 | **2** |

三类问题：

1. **id 对不上（最严重）**：形态 4/5 的 `nono.shape` 与 `glowShapes` 的 key 是两套编号，兜底查不到。
   例：形态 5 down，表里是 `18`，图层 `nono.shape` 是 `63`(body)/`3`(glow)；
   形态 5 boot，表里 `2–7`，图层是 `76/74/43/42/41`。
   形态 4 除极少数外全部对不上（`nono4_down` 表 `1,10,11,…` vs 图层 `27,67`）。
2. **整段没有表**：形态 1 的 8 个走路 + 4 个表情 + 玩球；形态 2 的关机；形态 3 的开机/关机；
   形态 5 的 8 个走路 + 4 个表情 + 玩球。这些动画只有（或没有）逐层手标，`glowShapes` 缺失。
3. **baked 比例高**：形态 1 表 40/49 baked，形态 5 表 66/75 baked，形态 4 表 51/220。
   baked 的条目运行时按设计不发光；如果其中有些其实没烘焙、应该由运行时发光，需要改回 `baked:false`。

## 3. 需求（用户原话）

> `nono.glow` 和 `nono.glowShapes` 都要发光；具体怎么方便、如何优雅由你决定。

**推荐方案（App 侧已按此实现）**：

- **逐层 `nono.glow` 是运行时唯一入口**：工具在导出时把源 SWF 的 GLOW 解析结果**落到对应图层**上，
  每个有 GLOW 的图层写一条 `nono.glow`（参数 + baked）。
- **顶层 `nono.glowShapes` 保留为源事实/参考**：按 source shape id 的全表，App 仍然支持它作为兜底；
  但不要再依赖它去驱动（因为图层和表的 id 体系不保证一致）。
- 逐层 `nono.glow` 与 `glowShapes` 的 `baked` 语义必须一致、准确：真的烘焙进画面才写 `true`。

## 4. 数据契约（工具要产出的字段）

### 4.1 逐层（Lottie `layers[i]`）

```json
"nono": {
  "role": "accent",
  "shape": "18",                    // 源 shape id（见 4.3）
  "glow": {
    "color": [50, 254, 254],        // 最终显示色，0–255
    "blur": 10.0,
    "blurY": 10.0,
    "strength": 1.0,                // >0 才生效
    "alpha": 255,
    "inner": false,
    "compositeSource": true,
    "passes": 1,
    "pulse": true,
    "baked": false
  }
}
```

- 没有 GLOW 的图层**不要写** `glow`（缺省 = 不发光）。
- 一个图层源上有多个 GLOW 时，取实际叠加后最接近的参数；或拆成多条合成通道（保持 `passes` 语义）。
- 逐层 `glow` 的 `baked=true` 时，App 跳过该图层。

### 4.2 顶层（Lottie `nono`）

```json
"glowShapes": {
  "18": { "color": [50,254,254], "blur": 10.0, "blurY": 10.0, "strength": 1.0,
          "alpha": 255, "inner": false, "compositeSource": true, "passes": 1,
          "pulse": true, "baked": false }
}
```

- 键 = 源 shape id；值 = 与逐层 `glow` 同结构。
- 表里可以有图层未引用的条目，但**不允许**图层引用了表里不存在的 key 还指望发光。

### 4.3 `nono.shape` 的语义（这次必须统一）

- `nono.shape` 必须写**源 SWF 的 shape/character id**，并且 **`glowShapes` 的 key 必须用同一套 id**。
  这样即使逐层 `glow` 漏写，App 的兜底也能命中。
- 形态 1–3 目前基本一致（match 高）；形态 4/5 必须按源 shape 重新标，不能写 Lottie 内部 shape 编号。

## 5. 每个形态要改什么（本次统一）

### 形态 1
- 8 个走路 + 4 个表情 + 玩球：补 `glowShapes`（如果源有 GLOW），并补逐层 `nono.glow`；
- 其余已有表的动画：核对 `baked`，把误标 baked 的改回 false；
- 保留用户手标的 accent 发光（可作为逐层 `glow` 的来源）。

### 形态 2
- 关机：补表/逐层；
- happy：表 `35/58/61` 与图层 shape 对不上，按源 shape 统一；
- 其余：核对 baked（35/189）。

### 形态 3
- 开机/关机：补表/逐层；
- 源 GLOW 的红色：直接写最终青色 `[50,254,254]`，不要写 `[255,0,0]`。

### 形态 4（重点）
- 走路 8 向、4 个表情、玩球/关机/召唤/魔方：`nono.shape` 与表全部/大部对不上，按源 shape 重标；
- 在逐层写 `nono.glow`；
- 表 220 条中 51 条 baked，逐条核对。

### 形态 5（重点）
- 8 个走路（表 1 条 `18`，图层 `3/63`…）、4 个表情 + 玩球（无表）：补表 + 逐层；
- cube/summon 表与图层对不上；boot 表 2–7 与 `76/74/43/42/41` 对不上；
- 保留用户手标（down 1 层、boot 4 层等）。

### 通用
- 全量重写标签时，先清掉旧 `nono` 再写，避免残留；
- `nono.dye` 不要动（这次只改 glow 相关字段）；
- 动画 JSON 的视觉内容、图层顺序、`nono` 以外的字段不要改；
- 改完把版本升到 **1.2.0**（新功能/标签大改），在 `assets/CHANGELOG.md` 记一条。

## 6. 验收标准

1. `python3 tools/build_catalog.py --check` 通过；
2. `nono --check` 输出的 **glow audit 警告为 0**：
   - 每个有 GLOW 的动画：`glowShapes` 与逐层 `nono.glow` 至少一条有效（且能点亮）；
   - 没有出现「表里有 shape 但图层 shape 一个都对不上」；
   - 没有出现「有表/有手标但运行时 pass 为 0」；
3. 运行时 `glow_passes()`：每个有 GLOW 的动画 > 0（App 侧可加 `--check` 打印）；
4. 实际播放：形态 1–5 的走路/开机/玩球/召唤/魔方/表情，该发光的部位都能看到辉光，且没有过曝；
5. `nono.dye` 的染色结果不变；6. 现有测试全绿。

## 7. 当前数据示例（形态 5，供对照）

`nono5_down.json`（现状）：
- 顶层 `glowShapes`: `{"18": {"color":[0,255,255], "blur":10, "strength":1, "baked":false}}`
- 图层：`shape=63`（body，无 glow）、`shape=3`（role=glow，逐层 glow 手标）
- 问题：`18` 没有落到任何图层；`3` 与 `18` 是两套 id。

`nono5_action_boot.json`（现状）：
- 顶层 `glowShapes` 键 `2–7`（其中 `2/3` baked=true）
- 图层：`76/74`（role=glow，逐层手标）、`43/42/41`（body）
- 问题：表与图层 id 对不上；baked 比例需核对。

期望（示例，不要求逐字一致）：
```json
// 顶层
"glowShapes": { "18": { "color": [50,254,254], "blur": 10, "strength": 1.0, "baked": false } }
// 真正承载该 GLOW 的图层
"nono": { "role": "glow", "shape": "18",
          "glow": { "color": [50,254,254], "blur": 10, "strength": 1.0, "baked": false } }
```

## 8. 不要动的东西

- `nono.dye` / 染色相关标签（本次只改 glow）；
- 动画图层、图形、时间轴、音频引用；
- `manifest.json` 的几何字段（canvas/petAnchor/frames）；
- 现有的 `nono.roles` 角色表（可以新增，但不要删）。

## 9. 交付

- 更新后的 `assets/` 全量 JSON（或只给改动的文件 + manifest）；
- `assets/asset-version.json` → `1.2.0`，`assets/CHANGELOG.md` 记录；
- 最好附一份「每个动画的 glowShapes 数 / 逐层 glow 数 / baked 数」的统计，方便我这边对照 `--check`。
