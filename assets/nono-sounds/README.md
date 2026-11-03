# 超能 NONO · 音效资产

从原资源里提取出来的**真实音效**（MP3），与动画资产**分开存放**。
动画在 `../nono-form1-*/`，音效在这里，通过 `manifest.json` 的绑定关系关联。

> **一句话结论**：形态 1 的 7 个动作里，**6 个有内嵌音效**（开机、关机、玩球、吃电池、召唤、玩魔方），
> **充电没有**；**4 个表情全部没有**；**八方向走路没有**。
> 这不是"没找到"，而是原 SWF 里根本没有内嵌 `sou` 类 —— 清单里如实标注为
> `missing-from-source` / `none`，**没有编造任何音频文件**。

## 目录结构

```
nono-sounds/
├── README.md          本文件
├── manifest.json      音效清单：每个文件 + 每个动画的播放规则
└── sounds/            29 个 MP3
    ├── action_boot.mp3        形态 1（主用）
    ├── action_boot_s2.mp3     形态 2（变体）
    └── ...
```

命名规则：

| 后缀 | 含义 |
|---|---|
| 无后缀 | **形态 1**（主用，`super/action/<id>_1.swf` 内嵌的那一条） |
| `_s2` ~ `_s5` | 形态 2 ~ 5 的**同动作变体**（`super/action/<id>_<N>.swf`），归档备用 |

## 形态 1 的音效（主用）

| 动作 | 音效文件 | 时长 | 采样率/声道 | 体积 | 循环 |
|---|---|---|---|---|---|
| 开机 boot | `sounds/action_boot.mp3` | 5.668 s | 11025 Hz / 单声道 | 11 KB | 1 次 |
| 关机 shutdown | `sounds/action_shutdown.mp3` | 2.080 s | 11025 Hz / 单声道 | 4.1 KB | 1 次 |
| 玩球 ball | `sounds/action_ball.mp3` | 9.152 s | 11025 Hz / 单声道 | 18 KB | 1 次 |
| 吃电池 battery | `sounds/action_battery.mp3` | 8.788 s | 11025 Hz / 单声道 | 17 KB | 1 次 |
| 召唤 summon | `sounds/action_summon.mp3` | 5.460 s | 11025 Hz / 单声道 | 11 KB | 1 次 |
| 玩魔方 cube | `sounds/action_cube.mp3` | 12.012 s | 11025 Hz / 单声道 | 23 KB | 1 次 |

全部是 **MP3 / MPEG 2.5 Layer III / 11025 Hz / 单声道 / 16 kbps** —— 这是原 SWF 的编码，
没有转码，**逐字节原样提取**。

形态 2~5 的变体是 **44.1 kHz 立体声**（召唤的形态 2 是 11 kHz 双声道），
是同一只宠物在不同形态下的音效版本，长度也不同。

## 什么时候播、怎么播

动画 manifest 的每个条目都带 `audio` 段（音效 manifest 的 `bindings[]` 是同一份数据的另一种视图）：

```json
"audio": {
  "soundId": "action_boot",
  "file": "nono-sounds/sounds/action_boot.mp3",
  "status": "ok",
  "trigger": "动作开始播放时（与动画 goToAndPlay(0) 同时，客户端在 gotoAndPlay 之后调用 play）",
  "loops": 1,
  "loopNote": "播 1 次",
  "stop": "下一个动作/表情开始前停止（客户端 clearActionMC() 里 _sc.stop()）",
  "playAs3": "_sc = _sound.play(0, 1);   // Sound 对象来自 SWF 内嵌的 sou 类",
  "playWeb": "const a = new Audio(url); a.loop = false; a.play();",
  "source": "SWF 内嵌 sou 类（super/action/2_1.swf）"
}
```

### 客户端的真实规则

依据 `客户端源码/NonoModel.as` 与 `客户端源码/MachXMLInfo.as`：

| 场景 | 客户端代码 | 循环次数来源 |
|---|---|---|
| 动作 | `gotoAndPlay(1)` **之后立刻** `_sc = _sound.play(0, getActionSouLoops(id))` | 配置表 `<action>` 段的 `souLoops` |
| 表情 | `gotoAndPlay(1)` 之后立刻 `_sc = _sound.play(0, 1)` | 写死 1（配置表 `<exp>` 段**没有** `souLoops`） |
| 走路 | `gotoAndStop(direction)` —— **完全不涉及 Sound** | — |
| 停止 | 下一个动作/表情开始前 `clearActionMC()` 里 `_sc.stop()` | — |

配置表 `配置表/NoNo配置表_MachXMLInfo.xml` 里的 `souLoops`：

| id | 动作 | souLoops | 含义 |
|---|---|---|---|
| 1 | 充电 | 999999 | 无限循环（但**充电没有音效文件**，宿主需自行注入） |
| 2 | 开机 | 1 | 播 1 次 |
| 3 | 关机 | 1 | 播 1 次 |
| 4 | 玩球 | 1 | 播 1 次 |
| 5 | 吃电池 | 1 | 播 1 次 |
| 6 | 召唤 | 1 | 播 1 次 |
| 7 | 玩魔方 | 1 | 播 1 次 |

`souLoops <= 0` 表示不播。

### Web / H5 侧接法

```js
const manifest = await fetch('assets/nono-sounds/manifest.json').then(r => r.json());
const binding  = manifest.bindings.find(b => b.animation.endsWith('nono1_action_boot.json'));
const audio    = new Audio('assets/' + binding.audio.file);
audio.loop = binding.audio.loops > 1;      // charge 之外都是 false
audio.play();                              // 与 lottie 的 autoplay 同时
```

`bindings[]` 里每个条目还带 `variants[]`，列出形态 2~5 的同名音效，
方便换形态时直接替换（时长不同，**不要**假定能无缝接续）。

### 没有音效的部分怎么办

- **充电**（`status: "missing-from-source"`）：原 SWF 里没有 `sou` 类。
  客户端仍会 `play(0, 999999)`，**声音需宿主另行注入**。
- **4 个表情**（`status: "missing-from-source"`）：所有 5 个形态的表情 SWF 都没有 `sou` 类。
  客户端仍会 `play(0, 1)`，声音同样需宿主注入。
- **走路**（`status: "none"`）：客户端的走路分支本来就不碰 Sound。

这三类是**资源本身的缺口**，本目录不提供占位音频 —— 放一个假音效进去会让
"没有音效"和"有音效"在排查时无法区分。

## 提取方法与可复现步骤

音效藏在 SWF 的 `DefineSound` 标签里，宿主端通过 AS3 类拿到：

```actionscript
// 从 SWF 里反编译出来的客户端音频封装
public dynamic class sou extends Sound {
    [Embed(source="/_assets/1_sou.mp3")]
}
```

宿主把它作为 `param1[1]` 传给模型对象，所以**每个动作 SWF 自带一条自己的声音**。
SWF 里的 `SoundStreamHead2` 全是空占位，真正的声音都在 `DefineSound`（MP3）里。

```bash
# 1) 提取动作 SWF 里的内嵌音效（FFDec -export sound）
bash _work/tools/extract_sounds_final.sh      # → _work/sounds_final/*.mp3

# 2) 复制到资产目录并生成清单
cp _work/sounds_final/*.mp3 assets/nono-sounds/sounds/
_work/venv/bin/python _work/tools/build_sounds_manifest.py

# 3) 校验（每个文件的时长/采样率/声道）
for f in assets/nono-sounds/sounds/*.mp3; do
  ffprobe -v error -show_entries format=duration \
          -show_entries stream=codec_name,sample_rate,channels -of csv=p=0 "$f"
done
```

`manifest.json` 里每个音效都带 `sha256`，可直接校验文件未被改动。

### 排查工具

| 工具 | 作用 |
|---|---|
| `_work/tools/extract_sounds_final.sh` | FFDec 批量提取 35 个动作 SWF 的 `sou` |
| `_work/tools/extract_sounds_xml.sh` | 导出 SWF 的 XML（音效调查的**权威依据**） |
| `_work/tools/sound_inventory.py` | 扫 XML 列出所有 `DefineSound` |
| `_work/tools/scan_swf_sound.py` | 自研扫描器，直接读 SWF 字节（辅助用，FFDec 才是 ground truth） |

## 已知坑

| 坑 | 说明 |
|---|---|
| **`_s` 不是形态后缀的唯一线索** | `action_shutdown` 本身就含 `_s`。拆变体名必须用正则 `^(action_[a-z]+)_s(\d+)$`，用 `rpartition('_s')` 会把 `action_shutdown` 拆成 `action` + `hutdown` 而崩。 |
| **吃电池的形态 5 没有音效** | `super/action/5_5.swf` 里没有 `DefineSound`，所以 `action_battery` 只有 `_s2`~`_s4` 三个变体（其它动作都有 4 个）。这是原件如此，不是提取失败。 |
| **充电的 5 个形态全都没有音效** | `super/action/1_N.swf` 全部无 `DefineSound`。 |
| **`SoundStreamHead2` 是空占位** | 别被它骗了 —— 真正的音频在 `DefineSound` 里。 |
| **FFDec `-swf2xml` 参数顺序** | **输入在前、输出在后**：`ffdec -swf2xml <输入> <输出>`。 |
| **FFDec 必须 headless** | `-Djava.awt.headless=true`，并把 `-Djava.io.tmpdir` 指到可写目录（本项目用 `_work/tmp/`，**不能用 `/tmp`**，沙箱会拒）。 |
| **临时目录不能是 `mktemp`** | 沙箱拒绝 `/var/folders`，所有中间文件必须落在 `_work/` 下。 |
