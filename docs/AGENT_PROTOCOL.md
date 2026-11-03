# Agent 协议与适配器

> 状态：M1 定义协议 + `MockAdapter`；M2 实现 Ollama / OpenAI 兼容 / Claude /
> 本地 CLI agent，并把回复流式渲染到气泡。

## 1. 设计原则

- **主程序只认识网关**，不知道背后是 Ollama、Claude 还是自研 agent。
- 重任务（HTTP、流式解析、子进程）**不进 UI 线程**；M2 用后台线程 + 事件总线。
- 没有配置任何 Agent 时，Mock 适配器保证整条链路可用（用于开发和演示）。

## 2. 协议

请求：

```json
{ "role": "user", "text": "你好", "context_id": "abc" }
```

响应：

```json
{
  "reply_text": "哈喽！",
  "emotion": "happy",
  "suggest_anim": "talk",
  "done": true
}
```

- `context_id` 可选，用于多轮对话；网关负责保存上下文。
- `emotion` 是提示，映射到当前形态的表情（见 §4）。
- `suggest_anim` 预留给动作（当前动作集里没有 talk，M2 可映射到 ball 等）。
- `done=false` 表示后面还有流式分片（M2）。

对应 Rust 类型：`nono_agent::{AgentRequest, AgentReply, AgentError}`，serde 字段与上面
完全一致（有单测锁定 JSON 形状）。

## 3. Adapter

```rust
pub trait AgentAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn send(&self, request: &AgentRequest) -> Result<AgentReply, AgentError>;
}
```

| Adapter | 说明 | 阶段 |
|---|---|---|
| `MockAdapter` | 固定/模板回复，无网络 | M1 ✅ |
| `OllamaAdapter` | `POST /api/chat`，NDJSON 流式 | M2 |
| `OpenAiCompatAdapter` | OpenAI / DeepSeek / 自建兼容服务 | M2 |
| `ClaudeAdapter` | Anthropic Messages API | M2 |
| `CliAgentAdapter` | 启动本地 `claude` / `codex` 等 CLI 子进程 | M2 |

配置（M2 写入 `config.toml`）：

```toml
[agent]
provider = "ollama"            # mock | ollama | openai | claude | cli
base_url = "http://127.0.0.1:11434"
model = "qwen2.5:7b"
api_key_env = "OPENAI_API_KEY" # 密钥只从环境变量读，不落盘
system_prompt = "你是超能 NONO，一只来自赛尔号的机器人伙伴，回答简短、俏皮。"
timeout_secs = 30

[agent.emotions]               # 情绪 → 形态 1/2 表情
happy = "happy"
sad = "sad"
angry = "angry"
surprised = "surprise"

[agent.emotions.form3]         # 形态 3 的表情名不同
happy = "happy"
sad = "dizzy"
angry = "angry"
surprised = "play"
```

## 4. 情绪 → 表情

形态 1 / 2：`happy / angry / surprise / sad`。
形态 3：`happy / angry / dizzy / play`（原资源没有「悲哀/惊讶」这两个语义，按配置映射）。

映射缺失时回退到 `happy`；表情不存在则只显示文字，不报错。

## 5. 流式与气泡（M2）

1. 用户触发对话（托盘「和 NONO 说话」→ 读剪贴板；或后续的输入框）。
2. 主程序发 `AgentRequest` 事件，网关在后台线程请求适配器。
3. 适配器逐段产出 `AgentReply { done: false }`，网关转成 `AgentReply` 事件。
4. UI 收到事件：更新气泡文本、把 `emotion` 映射成表情、播放一次。
5. `done=true` 时结束；`AgentError` 显示错误气泡。
6. 超时/取消：新请求取消旧请求（`context_id` 切换）。

气泡本身是第二个透明置顶窗口，文字用系统字体光栅化（M2 选型 `cosmic-text` 或
`fontdue`），点击穿透，若干秒后自动淡出。

## 6. 隐私与安全

- API Key 只从环境变量读取，不写入 `config.toml` 或日志。
- 默认 `provider = "mock"`，不联网；用户显式配置后才会把文本发出去。
- HTTP 请求在子线程，超时/错误只影响当次对话。
- CLI agent 以子进程运行，工作目录限定在用户数据目录（M2）。

## 7. M2 完成标准

- 通过托盘「读取剪贴板并向 NONO 提问」完成一次真实对话（Ollama 或 OpenAI 兼容）。
- 气泡正确显示流式文本，情绪映射到表情，结束时回到 Idle。
- 断网/超时/错误都有可见反馈，不崩溃。
- 全程 UI 线程不阻塞（拖动/动画仍流畅）。
