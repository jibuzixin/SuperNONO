# 超能 NONO 桌宠

把赛尔号的超能 NONO 放到桌面上：透明置顶、可拖拽、会自己走动、能播放原版动作与音效。
用 Rust + winit + ThorVG 实现，常驻内存约 **20 MB**，待机不重绘。

> 当前进度：**M1（桌宠核心）已完成**，macOS 可运行。插件与 Agent 的协议/接口已就位，
> 具体运行时分别在 M3 / M2 实现。

![NONO 召唤](docs/screenshots/form1-summon.png)
![形态 3 头晕](docs/screenshots/form3-dizzy.png)

## 功能（M1）

- 无边框、透明、始终置顶的宠物窗口，不占 Dock（macOS Accessory 策略）。
- 形态 1 / 2 / 3 的全部素材：八方向走路、动作（形态 3 少「充电」）、表情、动作音效。
- 按 `petAnchor` 对齐，切换动画时宠物在屏幕上不跳位。
- 拖拽移动；单击做表情；右键做随机动作；托盘菜单切换动作 / 表情 / 形态 / 大小 /
  音效 / 自由走动 / 隐藏 / 退出。
- 屏幕边缘硬限制：按宠物轮廓计算安全区，拖到屏幕边也拖不出去，轮廓不会被裁。
- 静止时原地循环播放走路动画（和原客户端一致），托盘可关掉以省电。
- 待机时偶尔自己走一段、换个朝向或做个表情（可关闭）。
- 窗口跟随动画画布动态缩放；播放动作时窗口点击穿透，不挡桌面。
- 配置持久化：形态、缩放、音量、漫游开关、屏幕位置。
- 全 JSON 事件总线，`--trace` 可看到所有事件。
- 送一个 `--render-frame` / `--render-icon` 小工具，方便导出素材帧和图标。

## 快速开始

前置：Rust 1.80+（本机用 rustup 安装），macOS 需要 Xcode Command Line Tools（提供 C++ 编译器与 libclang）。

```bash
rustup default stable
export PATH="$HOME/.cargo/bin:$PATH"   # 如果 cargo 还不在 PATH 里
cargo run --release -p nono-app
```

首次编译会从源码构建 ThorVG，约 1 分钟。

国内网络可先设置镜像：

```bash
export RUSTUP_DIST_SERVER=https://rsproxy.cn
export RUSTUP_UPDATE_ROOT=https://rsproxy.cn/rustup
```

常用参数：

```bash
cargo run --release -p nono-app -- --check        # 校验素材与 catalog
cargo run --release -p nono-app -- --list         # 列出动作/表情
cargo run --release -p nono-app -- --trace        # 打印事件与调试日志
cargo run --release -p nono-app -- --play action:1:cube   # 启动即播放某个动画
cargo run --release -p nono-app -- --render-frame 1 action cube 100 3 /tmp/cube.png
cargo run --release -p nono-app -- --render-icon assets/icon/nono-tray.png 64
```

`NONO_ASSETS` 可覆盖素材目录，`NONO_CONFIG` 可覆盖配置文件路径（便携/测试用）。

## 交互一览

| 操作 | 行为 |
|---|---|
| 左键单击 | 随机表情（60% 概率优先「开心」） |
| 左键拖拽 | 移动 NONO，松手后记住位置 |
| 右键单击 | 随机动作 |
| 托盘菜单 | 动作 / 表情 / 形态 / 大小 / 音效 / 自由走动 / 隐藏 / 退出 |

配置与窗口位置保存在 `~/Library/Application Support/NonoPet/config.toml`。

## 工程结构

```
assets/                  原始 Lottie 素材 + catalog.json + 托盘图标
crates/nono-core/        事件总线、状态机、行为调度、素材目录、配置
crates/nono-render/      ThorVG 软件光栅 + Lottie 播放（共享 picture、内存回收）
crates/nono-platform/    透明呈现（macOS CALayer）、托盘、NSSound、窗口
crates/nono-app/         主程序与 CLI
crates/nono-plugin/      插件契约（manifest / 权限 / 宿主 trait，M3 落地）
crates/nono-agent/       Agent 协议 + Adapter trait + Mock（M2 落地）
tools/build_catalog.py   把各目录 manifest 归一化成 catalog.json
docs/                    架构、素材契约、插件规范、Agent 协议
```

更细的设计见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)。

## 性能（本机 macOS 15.7 / 5K 屏 / release）

| 场景 | 数据 |
|---|---|
| 待机内存（footprint） | ~24 MB |
| 待机 CPU（静止播放走路） | 约 4%（1.5 ms/帧 × 25fps），托盘可关 |
| 关闭「静止时播放走路」后 | 不重绘、无定时器，≈0 |
| 最大动作（形态 2 玩球，2631×1440 物理像素） | ~68 MB，帧耗时 ~18 ms（25 fps） |
| 大动作结束回到待机 | ~25 MB（主动归还分配器页） |
| 走路窗口 | 99×144 pt，帧耗时 ~1.5–1.8 ms |

## 测试

```bash
cargo test --workspace
python3 tools/build_catalog.py --check
```

测试覆盖：素材目录完整性、petAnchor 对齐数学、走路/动作/表情状态迁移、
`auto_end=0` 停末帧、行为调度、事件 JSON 形状、ThorVG 帧推进与切换。

## 路线图

- **M2**：气泡 UI + Agent 对话（剪贴板触发；Mock/Ollama/OpenAI 兼容适配器；情绪→表情映射）。
- **M3**：wasmi 插件宿主（权限白名单、fuel 限额、热加载）+ 示例插件（整点报时）。
- **M4**：用户素材导入（图片帧 / 视频转精灵帧）、插件与素材市场、Windows/Linux 呈现后端。

## 已知限制

- 仅 macOS 有透明呈现后端；Windows/Linux 需要各自的 `Presenter` 实现。
- 点透是按状态整体开关的，不是逐像素命中；大动作播放期间窗口整体不接收点击。
- 超大动作窗口会裁剪到主屏可用区域（菜单栏/Dock 之外），保证角色不被窗口钳位带偏；
  多显示器时暂按主屏计算。
- 素材目前全是 Lottie；用户上传 / 视频转精灵帧的流水线尚未开工。
- 图标由 `nono --render-icon` 生成，尚未打包成 `.app` 与 `.icns`。

## 素材与许可

`assets/` 中的动画与音效来自项目提供的《超能 NONO》资源包，版权归原权利方所有，
仅供学习与个人桌面使用。代码部分为 MIT。
