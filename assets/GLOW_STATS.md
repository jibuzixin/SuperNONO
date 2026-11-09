# NONO 资产辉光统计（对照 App 侧 `nono --check`）

> 由 `_work/tools/audit_glow.py` 扫描 `assets/` 实测生成，与 `nono --check`
> 的 glow audit 同口径。**警告 0 条**。
> 列含义：`表` = 顶层 `nono.glowShapes` 条目数；`表 baked` = 其中 `baked:true`
> 的条目数；`逐层` = 带 `nono.glow` 的图层数；`baked` = 逐层里 `baked:true`
> 的图层数；`有效` = 运行时真正会发光的图层数（未烘焙且 strength>0）；
> `命中` = 图层 `nono.shape` 能命中顶层表的 shape 数。

## 汇总

| 资产组 | 动画数 | 表 | 表 baked | 逐层 glow | baked | 有效 | 命中 |
|---|---|---|---|---|---|---|---|
| 形态 1（超能） | 19 | 49 | 40 | 129 | 75 | 54 | 49 |
| 形态 2（超能） | 19 | 124 | 35 | 800 | 124 | 676 | 124 |
| 形态 3（超能） | 18 | 127 | 13 | 250 | 13 | 237 | 127 |
| 形态 4（超能） | 18 | 215 | 51 | 1491 | 88 | 1403 | 215 |
| 形态 5（超能） | 17 | 75 | 66 | 262 | 88 | 174 | 75 |
| 普通 NONO | 14 | 4 | 1 | 5 | 2 | 3 | 4 |
| 至尊年费 NONO | 17 | 0 | 0 | 0 | 0 | 0 | 0 |
| 飞行器 1 | 8 | 40 | 0 | 72 | 0 | 72 | 40 |
| 飞行器 2 | 8 | 32 | 0 | 80 | 0 | 80 | 32 |
| 飞行器 3 | 8 | 32 | 0 | 32 | 0 | 32 | 32 |
| 飞行器 4 | 8 | 32 | 0 | 32 | 0 | 32 | 32 |
| 通用飞行器 | 8 | 32 | 0 | 80 | 0 | 80 | 32 |
| **合计** | **162** | **762** | **206** | **3233** | **390** | **2843** | **762** |

## 逐动画

### 形态 1（超能）

| 类别 | 动画 | 表 | 表 baked | 逐层 glow | baked | 有效 | 命中 |
|---|---|---|---|---|---|---|---|
| walk | down | 0 | 0 | 4 | 0 | 4 | 0 |
| walk | left | 0 | 0 | 3 | 0 | 3 | 0 |
| walk | leftdown | 0 | 0 | 3 | 0 | 3 | 0 |
| walk | leftup | 0 | 0 | 4 | 0 | 4 | 0 |
| walk | right | 0 | 0 | 3 | 0 | 3 | 0 |
| walk | rightdown | 0 | 0 | 3 | 0 | 3 | 0 |
| walk | rightup | 0 | 0 | 4 | 0 | 4 | 0 |
| walk | up | 0 | 0 | 4 | 0 | 4 | 0 |
| actions | ball | 0 | 0 | 0 | 0 | 0 | 0 |
| actions | battery | 8 | 0 | 25 | 0 | 25 | 8 |
| actions | boot | 11 | 11 | 43 | 43 | 0 | 11 |
| actions | charge | 1 | 1 | 2 | 2 | 0 | 1 |
| actions | cube | 1 | 0 | 1 | 0 | 1 | 1 |
| actions | shutdown | 20 | 20 | 21 | 21 | 0 | 20 |
| actions | summon | 8 | 8 | 9 | 9 | 0 | 8 |
| expressions | angry | 0 | 0 | 0 | 0 | 0 | 0 |
| expressions | happy | 0 | 0 | 0 | 0 | 0 | 0 |
| expressions | sad | 0 | 0 | 0 | 0 | 0 | 0 |
| expressions | surprise | 0 | 0 | 0 | 0 | 0 | 0 |

### 形态 2（超能）

| 类别 | 动画 | 表 | 表 baked | 逐层 glow | baked | 有效 | 命中 |
|---|---|---|---|---|---|---|---|
| walk | down | 1 | 0 | 10 | 0 | 10 | 1 |
| walk | left | 3 | 0 | 10 | 0 | 10 | 3 |
| walk | leftdown | 3 | 0 | 10 | 0 | 10 | 3 |
| walk | leftup | 3 | 0 | 130 | 0 | 130 | 3 |
| walk | right | 3 | 0 | 10 | 0 | 10 | 3 |
| walk | rightdown | 3 | 0 | 10 | 0 | 10 | 3 |
| walk | rightup | 3 | 0 | 130 | 0 | 130 | 3 |
| walk | up | 3 | 0 | 10 | 0 | 10 | 3 |
| actions | ball | 6 | 0 | 23 | 0 | 23 | 6 |
| actions | battery | 12 | 0 | 37 | 0 | 37 | 12 |
| actions | boot | 10 | 10 | 32 | 10 | 22 | 10 |
| actions | charge | 1 | 0 | 2 | 0 | 2 | 1 |
| actions | cube | 4 | 0 | 88 | 0 | 88 | 4 |
| actions | shutdown | 0 | 0 | 22 | 0 | 22 | 0 |
| actions | summon | 63 | 25 | 231 | 114 | 117 | 63 |
| expressions | angry | 2 | 0 | 15 | 0 | 15 | 2 |
| expressions | happy | 0 | 0 | 10 | 0 | 10 | 0 |
| expressions | sad | 2 | 0 | 10 | 0 | 10 | 2 |
| expressions | surprise | 2 | 0 | 10 | 0 | 10 | 2 |

### 形态 3（超能）

| 类别 | 动画 | 表 | 表 baked | 逐层 glow | baked | 有效 | 命中 |
|---|---|---|---|---|---|---|---|
| walk | down | 1 | 0 | 6 | 0 | 6 | 1 |
| walk | left | 1 | 0 | 6 | 0 | 6 | 1 |
| walk | leftdown | 1 | 0 | 6 | 0 | 6 | 1 |
| walk | leftup | 1 | 0 | 6 | 0 | 6 | 1 |
| walk | right | 1 | 0 | 6 | 0 | 6 | 1 |
| walk | rightdown | 1 | 0 | 6 | 0 | 6 | 1 |
| walk | rightup | 1 | 0 | 6 | 0 | 6 | 1 |
| walk | up | 1 | 0 | 6 | 0 | 6 | 1 |
| actions | ball | 5 | 0 | 18 | 0 | 18 | 5 |
| actions | battery | 9 | 0 | 21 | 0 | 21 | 9 |
| actions | boot | 0 | 0 | 0 | 0 | 0 | 0 |
| actions | cube | 74 | 0 | 85 | 0 | 85 | 74 |
| actions | shutdown | 0 | 0 | 0 | 0 | 0 | 0 |
| actions | summon | 14 | 13 | 19 | 13 | 6 | 14 |
| expressions | angry | 14 | 0 | 21 | 0 | 21 | 14 |
| expressions | dizzy | 1 | 0 | 8 | 0 | 8 | 1 |
| expressions | happy | 1 | 0 | 6 | 0 | 6 | 1 |
| expressions | play | 1 | 0 | 24 | 0 | 24 | 1 |

### 形态 4（超能）

| 类别 | 动画 | 表 | 表 baked | 逐层 glow | baked | 有效 | 命中 |
|---|---|---|---|---|---|---|---|
| walk | down | 11 | 0 | 164 | 0 | 164 | 11 |
| walk | left | 13 | 0 | 30 | 0 | 30 | 13 |
| walk | leftdown | 13 | 0 | 30 | 0 | 30 | 13 |
| walk | leftup | 13 | 0 | 164 | 0 | 164 | 13 |
| walk | right | 13 | 0 | 30 | 0 | 30 | 13 |
| walk | rightdown | 13 | 0 | 30 | 0 | 30 | 13 |
| walk | rightup | 13 | 0 | 164 | 0 | 164 | 13 |
| walk | up | 11 | 0 | 164 | 0 | 164 | 11 |
| actions | ball | 17 | 0 | 288 | 0 | 288 | 17 |
| actions | battery | 23 | 0 | 81 | 0 | 81 | 23 |
| actions | boot | 17 | 17 | 27 | 27 | 0 | 17 |
| actions | cube | 12 | 0 | 234 | 0 | 234 | 12 |
| actions | shutdown | 10 | 10 | 20 | 20 | 0 | 10 |
| actions | summon | 24 | 24 | 41 | 41 | 0 | 24 |
| expressions | angry | 1 | 0 | 2 | 0 | 2 | 1 |
| expressions | dizzy | 2 | 0 | 4 | 0 | 4 | 2 |
| expressions | play | 7 | 0 | 14 | 0 | 14 | 7 |
| expressions | smug | 2 | 0 | 4 | 0 | 4 | 2 |

### 形态 5（超能）

| 类别 | 动画 | 表 | 表 baked | 逐层 glow | baked | 有效 | 命中 |
|---|---|---|---|---|---|---|---|
| walk | down | 1 | 0 | 14 | 0 | 14 | 1 |
| walk | left | 1 | 0 | 15 | 0 | 15 | 1 |
| walk | leftdown | 1 | 0 | 15 | 0 | 15 | 1 |
| walk | leftup | 1 | 0 | 15 | 0 | 15 | 1 |
| walk | right | 1 | 0 | 15 | 0 | 15 | 1 |
| walk | rightdown | 1 | 0 | 15 | 0 | 15 | 1 |
| walk | rightup | 1 | 0 | 15 | 0 | 15 | 1 |
| walk | up | 1 | 0 | 15 | 0 | 15 | 1 |
| actions | ball | 0 | 0 | 13 | 0 | 13 | 0 |
| actions | boot | 43 | 43 | 46 | 43 | 3 | 43 |
| actions | cube | 1 | 0 | 9 | 0 | 9 | 1 |
| actions | shutdown | 1 | 1 | 4 | 1 | 3 | 1 |
| actions | summon | 22 | 22 | 45 | 44 | 1 | 22 |
| expressions | angry | 0 | 0 | 3 | 0 | 3 | 0 |
| expressions | happy | 0 | 0 | 13 | 0 | 13 | 0 |
| expressions | sad | 0 | 0 | 5 | 0 | 5 | 0 |
| expressions | surprise | 0 | 0 | 5 | 0 | 5 | 0 |

### 普通 NONO

| 类别 | 动画 | 表 | 表 baked | 逐层 glow | baked | 有效 | 命中 |
|---|---|---|---|---|---|---|---|
| walk | down | 0 | 0 | 0 | 0 | 0 | 0 |
| walk | left | 0 | 0 | 0 | 0 | 0 | 0 |
| walk | leftdown | 0 | 0 | 0 | 0 | 0 | 0 |
| walk | leftup | 1 | 0 | 1 | 0 | 1 | 1 |
| walk | right | 0 | 0 | 0 | 0 | 0 | 0 |
| walk | rightdown | 0 | 0 | 0 | 0 | 0 | 0 |
| walk | rightup | 1 | 0 | 1 | 0 | 1 | 1 |
| walk | up | 1 | 0 | 1 | 0 | 1 | 1 |
| actions | boot | 1 | 1 | 2 | 2 | 0 | 1 |
| actions | charge | 0 | 0 | 0 | 0 | 0 | 0 |
| expressions | angry | 0 | 0 | 0 | 0 | 0 | 0 |
| expressions | happy | 0 | 0 | 0 | 0 | 0 | 0 |
| expressions | sad | 0 | 0 | 0 | 0 | 0 | 0 |
| expressions | surprise | 0 | 0 | 0 | 0 | 0 | 0 |

### 至尊年费 NONO

| 类别 | 动画 | 表 | 表 baked | 逐层 glow | baked | 有效 | 命中 |
|---|---|---|---|---|---|---|---|
| walk | down | 0 | 0 | 0 | 0 | 0 | 0 |
| walk | left | 0 | 0 | 0 | 0 | 0 | 0 |
| walk | leftdown | 0 | 0 | 0 | 0 | 0 | 0 |
| walk | leftup | 0 | 0 | 0 | 0 | 0 | 0 |
| walk | right | 0 | 0 | 0 | 0 | 0 | 0 |
| walk | rightdown | 0 | 0 | 0 | 0 | 0 | 0 |
| walk | rightup | 0 | 0 | 0 | 0 | 0 | 0 |
| walk | up | 0 | 0 | 0 | 0 | 0 | 0 |
| actions | ball | 0 | 0 | 0 | 0 | 0 | 0 |
| actions | boot | 0 | 0 | 0 | 0 | 0 | 0 |
| actions | cube | 0 | 0 | 0 | 0 | 0 | 0 |
| actions | shutdown | 0 | 0 | 0 | 0 | 0 | 0 |
| actions | summon | 0 | 0 | 0 | 0 | 0 | 0 |
| expressions | angry | 0 | 0 | 0 | 0 | 0 | 0 |
| expressions | happy | 0 | 0 | 0 | 0 | 0 | 0 |
| expressions | sad | 0 | 0 | 0 | 0 | 0 | 0 |
| expressions | surprise | 0 | 0 | 0 | 0 | 0 | 0 |

### 飞行器 1

| 类别 | 动画 | 表 | 表 baked | 逐层 glow | baked | 有效 | 命中 |
|---|---|---|---|---|---|---|---|
| walk | fly1_down | 5 | 0 | 9 | 0 | 9 | 5 |
| walk | fly1_left | 5 | 0 | 9 | 0 | 9 | 5 |
| walk | fly1_leftdown | 5 | 0 | 9 | 0 | 9 | 5 |
| walk | fly1_leftup | 5 | 0 | 9 | 0 | 9 | 5 |
| walk | fly1_right | 5 | 0 | 9 | 0 | 9 | 5 |
| walk | fly1_rightdown | 5 | 0 | 9 | 0 | 9 | 5 |
| walk | fly1_rightup | 5 | 0 | 9 | 0 | 9 | 5 |
| walk | fly1_up | 5 | 0 | 9 | 0 | 9 | 5 |

### 飞行器 2

| 类别 | 动画 | 表 | 表 baked | 逐层 glow | baked | 有效 | 命中 |
|---|---|---|---|---|---|---|---|
| walk | fly2_down | 4 | 0 | 10 | 0 | 10 | 4 |
| walk | fly2_left | 4 | 0 | 10 | 0 | 10 | 4 |
| walk | fly2_leftdown | 4 | 0 | 10 | 0 | 10 | 4 |
| walk | fly2_leftup | 4 | 0 | 10 | 0 | 10 | 4 |
| walk | fly2_right | 4 | 0 | 10 | 0 | 10 | 4 |
| walk | fly2_rightdown | 4 | 0 | 10 | 0 | 10 | 4 |
| walk | fly2_rightup | 4 | 0 | 10 | 0 | 10 | 4 |
| walk | fly2_up | 4 | 0 | 10 | 0 | 10 | 4 |

### 飞行器 3

| 类别 | 动画 | 表 | 表 baked | 逐层 glow | baked | 有效 | 命中 |
|---|---|---|---|---|---|---|---|
| walk | fly3_down | 4 | 0 | 4 | 0 | 4 | 4 |
| walk | fly3_left | 4 | 0 | 4 | 0 | 4 | 4 |
| walk | fly3_leftdown | 4 | 0 | 4 | 0 | 4 | 4 |
| walk | fly3_leftup | 4 | 0 | 4 | 0 | 4 | 4 |
| walk | fly3_right | 4 | 0 | 4 | 0 | 4 | 4 |
| walk | fly3_rightdown | 4 | 0 | 4 | 0 | 4 | 4 |
| walk | fly3_rightup | 4 | 0 | 4 | 0 | 4 | 4 |
| walk | fly3_up | 4 | 0 | 4 | 0 | 4 | 4 |

### 飞行器 4

| 类别 | 动画 | 表 | 表 baked | 逐层 glow | baked | 有效 | 命中 |
|---|---|---|---|---|---|---|---|
| walk | fly4_down | 4 | 0 | 4 | 0 | 4 | 4 |
| walk | fly4_left | 4 | 0 | 4 | 0 | 4 | 4 |
| walk | fly4_leftdown | 4 | 0 | 4 | 0 | 4 | 4 |
| walk | fly4_leftup | 4 | 0 | 4 | 0 | 4 | 4 |
| walk | fly4_right | 4 | 0 | 4 | 0 | 4 | 4 |
| walk | fly4_rightdown | 4 | 0 | 4 | 0 | 4 | 4 |
| walk | fly4_rightup | 4 | 0 | 4 | 0 | 4 | 4 |
| walk | fly4_up | 4 | 0 | 4 | 0 | 4 | 4 |

### 通用飞行器

| 类别 | 动画 | 表 | 表 baked | 逐层 glow | baked | 有效 | 命中 |
|---|---|---|---|---|---|---|---|
| walk | flymachine1_down | 4 | 0 | 10 | 0 | 10 | 4 |
| walk | flymachine1_left | 4 | 0 | 10 | 0 | 10 | 4 |
| walk | flymachine1_leftdown | 4 | 0 | 10 | 0 | 10 | 4 |
| walk | flymachine1_leftup | 4 | 0 | 10 | 0 | 10 | 4 |
| walk | flymachine1_right | 4 | 0 | 10 | 0 | 10 | 4 |
| walk | flymachine1_rightdown | 4 | 0 | 10 | 0 | 10 | 4 |
| walk | flymachine1_rightup | 4 | 0 | 10 | 0 | 10 | 4 |
| walk | flymachine1_up | 4 | 0 | 10 | 0 | 10 | 4 |

## 口径说明

- `baked:true` = 这份素材里光晕**已经被画进画面**（转换器生成的
  `glowN` / `glowmorphN` 同心描边副本层），运行时按规范跳过，
  不再叠加，避免过曝 —— 这类资产的运行时辉光 pass 数**本来就是 0**。
- `有效` 为 0 且 `逐层 glow > 0` 的资产属于「全部烘焙」，不是缺陷。
- `表` 为 0 而 `逐层 glow > 0` 的资产，其辉光来自**人工标注**
  （源 SWF 里没有 `GLOWFILTER`），走逐层入口发光，不依赖顶层表。
- `表` 与 `逐层 glow` 都为 0 的资产：源 SWF 里没有 `GLOWFILTER`，
  或该方向时间轴没用到带辉光的形状 —— 都不是漏标。
