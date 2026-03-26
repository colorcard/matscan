# matscan 配置（TOML）与日志说明（完整）

本文基于当前仓库代码实现（`src/config.rs`、`src/main.rs`、`src/tracing.rs` 等）整理，重点解释：

- `config.toml` 每个配置项的含义与默认行为
- 日志/输出的来源、级别过滤与落盘机制
- 一些容易踩坑但很关键的实现细节

---

## 1. 配置文件加载与解析机制

## 1.1 加载入口

程序启动后会读取命令行第一个参数作为配置路径；未传时默认使用 `config.toml`：

- 见 `src/main.rs`：`args.get(1).unwrap_or("config.toml")`
- 随后会 `canonicalize`，再 `read_to_string`，最后 `toml::from_str` 反序列化

这意味着：

- 路径错误、权限问题、TOML 语法错误都会在启动阶段直接报错退出
- 配置是一次性读取，不会热更新

## 1.2 字段严格校验（非常重要）

`Config` 及其子结构体基本都带了 `#[serde(deny_unknown_fields)]`（`src/config.rs`）。

含义：

- 写错字段名不会被“静默忽略”，而是直接反序列化失败
- 例如把 `filter_sql` 错写成旧格式字段，会直接启动失败

这对线上运行很有价值：可以尽早暴露配置拼写/结构错误。

---

## 2. 顶层配置项（`Config`）

以下字段位于 TOML 顶层。

## 2.1 `postgres_uri`（必填）

- 类型：`String`
- 作用：PostgreSQL 连接串，程序启动后用于 `Database::connect`
- 示例：`postgres://matscan:password@localhost/matscan`

## 2.2 `rate`（必填）

- 类型：`u64`
- 含义：发包速率上限（packets per second）
- 影响：扫描发送线程中节流器会按该值控制 SYN 发送速率

## 2.3 `sleep_secs`（可选，默认 10）

- 类型：`Option<u64>`
- 默认：未设置时按 `10` 秒
- 含义：每轮扫描结束后的“额外等待窗口”
- 实现细节：
  - 程序会先等待处理线程把当前队列处理完
  - 然后用 `sleep_secs - processing_time` 计算还需补多少等待时间
  - 目的是减少“慢响应”串到下一轮策略的概率（避免归因错位）

## 2.4 `source_port`（可选，默认 `61000`）

- 类型：`SourcePort`
- 支持两种 TOML 形态：
  - 单端口：`source_port = 61000`
  - 区间：`source_port = { min = 61000, max = 65535 }`
- 用途：指定扫描发包的源端口（或端口池）
- 重要运维要求：必须在本机防火墙丢弃该源端口的入站包，否则内核 TCP 栈会干扰扫描连接状态

实现细节（区间模式）：

- 代码里端口选择基于种子做取模（`src/scanner/mod.rs`）
- 实际挑选范围是 `[min, max)`（上界不参与随机挑选）

## 2.5 `scan_duration_secs`（可选，默认 300）

- 类型：`Option<u64>`
- 默认：`5 分钟`
- 含义：单轮扫描最长期限
- 发送线程达到目标包数或超时后结束

## 2.6 `ping_timeout_secs`（可选，默认 60）

- 类型：`Option<u64>`
- 默认：`60 秒`
- 含义：接收侧连接状态保留时长；超过后会清理旧连接

## 2.7 `logging_dir`（可选）

- 类型：`Option<PathBuf>`
- 含义：设置后启用文件日志（按天滚动），目录下写入 `matscan.log`
- 不设置时：不会创建 tracing 文件日志层

## 2.8 `target`（必填小节）

```toml
[target]
addr = "matscan"
port = 1337
protocol_version = 47
```

- `addr`：握手中的目标地址字符串（hostname）
- `port`：目标端口
- `protocol_version`：Minecraft 协议版本号（握手字段）

## 2.9 `scanner`（必填小节）

```toml
[scanner]
enabled = true
# strategies = ["Slash0", "Slash24a"]
```

- `enabled`：是否启用常规扫描策略
- `strategies`（可选）：
  - 不填表示使用全部内置扫描策略
  - 填了会校验名称合法性，不合法会直接 `panic`
  - 名称使用策略枚举名（如 `Slash0`、`Slash16a`、`Slash24b` 等）

## 2.10 `rescan` ~ `rescan5`（可选小节，最多 5 组）

每组结构一致，可用于配置不同节奏/过滤条件的重扫通道。

字段说明：

- `enabled: bool`：是否启用该重扫通道
- `rescan_every_secs: u64`：重扫间隔窗口下界（“至少多久没扫过”）
- `players_online_ago_max_secs: Option<u64>`：要求最近有玩家在线（按时间窗口）
- `last_ping_ago_max_secs: u64`：重扫窗口上界（默认逻辑值 2 小时）
- `limit: Option<usize>`：SQL LIMIT
- `filter_sql: String`：追加到 WHERE 子句的原始 SQL 片段
- `sort: Option<random|oldest>`：排序策略（随机/最久未扫优先）
- `padded: bool`：是否加入“填充地址块”防止有效响应过于集中

关键细节：

- `filter_sql` 是直接拼接 SQL（代码注释明确把配置视为可信输入）
- 整个 `rescanX` 小节若完全省略，会走结构体默认值（通常相当于禁用）
- 若小节存在但未写 `last_ping_ago_max_secs`，反序列化默认值为 2 小时

## 2.11 `snipe`（可选）

- `enabled`：是否启用“玩家上下线狙击通知”
- `webhook_url`：Discord webhook 地址
- `usernames`：关注用户名列表
- `anon_players`：是否监控匿名玩家突增事件

行为概述：

- 通过与上次缓存样本对比，判断“加入/离开”
- 满足条件时异步发 webhook 消息

## 2.12 `fingerprinting`（可选）

- `enabled: bool`
- 开启后会主动发送特定请求以触发服务端错误响应，用于协议实现指纹识别
- 代码注释提示：可能在服务端控制台产生错误输出

## 2.13 `debug`（可选）

- `exit_on_done: bool`：完成一轮后立即退出（调试用）
- `only_scan_addr: Option<SocketAddrV4>`：只扫描一个地址；并禁用其他策略分支与排除列表
- `simulate_rx_loss: f32`：模拟接收丢包概率
- `simulate_tx_loss: f32`：模拟发送丢包概率

---

## 3. 日志与输出：你看到的内容分别来自哪里

这个项目同时存在两套输出体系：

1. **控制台直出（`println!/eprintln!`）**
2. **`tracing` 结构化日志（`info!/warn!/error!/debug!/trace!`）**

## 3.1 控制台直出（stdout/stderr）

这是你最容易看到的运行信息，典型包括：

- 启动阶段：`Starting...`、`parsing config...`
- 扫描过程：`chosen strategy: ...`、`scanning ... targets`
- 周期统计：`packets_sent = ...`
- 结果汇总：`updated/added/revived` 彩色统计
- 某些错误走 `eprintln!`

特点：

- 与 `RUST_LOG` 无关，默认就会输出
- 部分输出含 ANSI 颜色（见 `src/terminal_colors.rs`）

## 3.2 tracing 日志初始化机制

`init_tracing` 在 `src/tracing.rs`，核心行为：

1. 总是加载 `EnvFilter::from_default_env()`（读取 `RUST_LOG`）
2. 仅当配置了 `logging_dir` 时，追加一个文件日志层：
   - 按天滚动：`matscan.log`
   - 关闭 ANSI
   - 层级上限为 `DEBUG`（`TRACE` 不会进该层）

这意味着：

- 不配置 `logging_dir`：tracing 事件不会写文件
- 配置 `logging_dir`：tracing 事件会按 `RUST_LOG` + `DEBUG` 上限共同过滤后写入文件

## 3.3 级别含义（结合本项目）

- `error`：明确错误（如后台维护任务异常）
- `warn`：可恢复异常/可疑状态（如模拟丢包、异常协议情况）
- `info`：阶段性业务信息（如收集服务器、一轮扫描汇总）
- `debug`：调试级细节（如重扫 SQL）
- `trace`：非常高频底层细节（如收包、握手细节、发包 trace）

注意：由于文件层限制为 `DEBUG`，`trace!` 即使放开 `RUST_LOG` 也不会进 `matscan.log`。

---

## 4. 常见输出语句的业务含义（速查）

- `chosen strategy: Xxx`：本轮选中的策略类别/策略名
- `get_ranges took ...`：策略查询目标范围耗时
- `scanning N targets (M ranges)`：本轮最终发送目标规模
- `excluded X targets from this scan`：应用排除规则后减少数量
- `waiting for processing to finish...`：发包结束，等待异步处理落库
- `sleeping for S seconds`：执行轮间补偿等待
- `ok finished adding to db ...`：普通扫描结果汇总（更新/新增/复活/速度）
- `ok finished rescanning ...`：重扫模式结果汇总（回复率等）
- `packets_sent = ...`：发包速率周期统计

---

## 5. 配置示例（与当前代码匹配）

```toml
postgres_uri = "postgres://matscan:replace-me@localhost/matscan"
rate = 100_000
sleep_secs = 10
source_port = { min = 61000, max = 65535 }
scan_duration_secs = 300
ping_timeout_secs = 60
logging_dir = "./logs"

[target]
addr = "matscan"
port = 25565
protocol_version = 47

[scanner]
enabled = true
# strategies = ["Slash0", "Slash24a", "Slash32c"]

[rescan]
enabled = true
rescan_every_secs = 3600
last_ping_ago_max_secs = 7200
limit = 100000
sort = "oldest"
filter_sql = "online_players > 0"
padded = false

[snipe]
enabled = false
webhook_url = ""
usernames = []
anon_players = false

[fingerprinting]
enabled = false

[debug]
exit_on_done = false
simulate_rx_loss = 0.0
simulate_tx_loss = 0.0
```

---

## 6. 必要的实现原理总结（简版）

1. **扫描发送与响应处理解耦**  
   发包线程尽量高速发送；接收与处理由独立流程完成，最后在轮次边界做同步。

2. **轮次边界的“等待 + 补偿睡眠”机制**  
   先等处理完成，再补齐 `sleep_secs`，降低慢响应污染下一轮策略统计。

3. **策略分类轮转**  
   常规扫描 / 重扫 / 指纹识别作为不同类别轮换，避免单一路径长期独占。

4. **日志分层**  
   控制台直出承担“实时可见进度”；`tracing` 负责可过滤、可落盘的结构化事件。

---

## 7. 一个实用建议

如果你希望“既看实时，又能落文件并保留较多细节”，建议同时做两件事：

1. 在配置里设置 `logging_dir`
2. 运行前设置 `RUST_LOG`（例如模块级 `info/debug`）

这样能把 `tracing` 事件写入滚动日志文件，同时保留程序本身的控制台进度输出。

