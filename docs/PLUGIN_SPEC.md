# 插件规范

> 状态：**M1 只实现契约**（`crates/nono-plugin`：manifest、权限、宿主 trait）。
> wasm 宿主（wasmi + 权限校验 + 热加载）在 M3 实现。

## 1. 为什么是 WASM

- 三平台行为一致，插件二进制不依赖系统库。
- 天然沙箱：插件拿不到文件系统、网络、窗口句柄，只能调用宿主显式暴露的函数。
- 一个坏插件 panic / 死循环不会带崩桌宠（宿主隔离 + fuel 限额）。
- 可以用 Rust / AssemblyScript / TinyGo 编写，AI 也容易生成。

明确不做：原生 `.so/.dylib` 插件（无沙箱）、Python 插件（要捆 runtime）。

## 2. 插件包

```
my-plugin/
├─ plugin.toml      # 元信息 + 权限声明
├─ main.wasm        # wasm32-wasip1 模块
└─ assets/          # 可选：插件自带素材
```

`plugin.toml`：

```toml
id = "clock.chime"
name = "整点报时"
version = "0.1.0"
author = "community"
description = "每个整点让 NONO 说一句话并播放开心表情"
api_version = 1
entry = "main.wasm"          # 可省略，默认 main.wasm
permissions = ["send_animation_event", "show_notification"]
```

`api_version` 大于宿主支持版本时直接拒绝加载（`PluginError::ApiVersion`）。

## 3. 权限白名单

权限必须在 `plugin.toml` 声明；未声明的调用由宿主拒绝并记录。

| 权限 | 能力 | 风险 |
|---|---|---|
| `send_animation_event` | 发 `PlayAnimation` / `ChangeState` 等事件 | 低（只能控制角色） |
| `read_user_text` | 读取用户输入的对话文本 | 中（可能含隐私） |
| `access_files` | 读写**插件自己目录**下的文件 | 中（路径被宿主限定） |
| `http_outbound` | 宿主代发 HTTP 请求（域名白名单可再加） | 中（数据外发） |
| `clipboard_read` / `clipboard_write` | 剪贴板 | 高（敏感） |
| `show_notification` | 系统通知 | 低 |

宿主侧在每次特权调用前执行 `PluginManifest::require(permission)`。

## 4. 宿主 ABI（M3 实现）

插件导出（wasm32-wasip1，C ABI）：

| 导出 | 说明 |
|---|---|
| `nono_alloc(len: u32) -> u32` | 宿主写入 JSON 前分配内存 |
| `nono_free(ptr: u32, len: u32)` | 释放宿主传入的内存 |
| `nono_init() -> i32` | 插件初始化，返回 0 表示成功 |
| `nono_on_event(ptr: u32, len: u32)` | 收到一条 JSON 事件 |
| `nono_tick(now_ms: u64)` | 周期性回调（可选，用于定时任务） |

宿主导入（`env` 命名空间，全部经过权限校验）：

| 函数 | 说明 |
|---|---|
| `host_emit(ptr, len)` | 发布一条 JSON 事件（需 `send_animation_event`） |
| `host_log(level, ptr, len)` | 写日志 |
| `host_notify(ptr, len)` | 系统通知（需 `show_notification`） |
| `host_read_user_text() -> ptr` | 取最近一次用户输入（需 `read_user_text`） |
| `host_http_get(url_ptr, url_len) -> ptr` | 受限 HTTP（需 `http_outbound`） |
| `host_clipboard_read()` / `host_clipboard_write(...)` | 剪贴板（需对应权限） |

返回给宿主的数据统一是「长度 + 指向 wasm 线性内存的 JSON UTF-8」，JSON 形状与
`nono_core::Event` 完全一致，因此插件作者只需要看一种协议。

## 5. 生命周期与热加载

1. 扫描插件目录，读取并校验 `plugin.toml`。
2. 校验 `api_version`、entry 文件存在、权限列表。
3. 实例化 wasm，调用 `nono_init`。
4. 事件广播时把 JSON 投递给每个插件（M3 可按订阅过滤）。
5. 卸载：丢弃实例；文件变化时重新加载（M3）。

隔离策略：

- 每个插件独立 `Store`，**不共享内存**。
- 默认不启用 WASI 文件/网络；仅通过 host function 访问。
- fuel/epoch 限额防止死循环；超时或 trap 只卸载该插件。
- 插件 panic（trap）不会传播到宿主进程。

## 6. 示例：整点报时

```text
on_init: 读取上次整点时间
on_tick(now):
  if now 跨过整点:
    emit {"type":"PlayAnimation","anim_id":"form1/expr/happy","form":1}
    notify("NONO 报时：现在是 15 点")
```

## 7. 开发与分发（M3 计划）

- 提供 `cargo generate` 模板（Rust + `wasm32-wasip1`）。
- 插件只是目录 + wasm，压缩包分发；宿主校验可选 sha256。
- 权限在加载时展示给用户确认（M4 插件市场）。
