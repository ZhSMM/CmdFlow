# CmdFlow — 设计文档

> 本地命令编排工具：可视化编排 + 参数化执行 + DAG 工作流 + 定时调度
> 项目名暂定，可改。

---

## 1. 项目概述

### 1.1 定位

本机命令执行器 + 轻量工作流引擎。把日常用到的 `cmd` / `PowerShell` / `python` / `node` 脚本、HTTP 请求、AI 调用等「本地会跑的东西」集中起来，做成可配置、可参数化、可串联、可定时的任务流。

对标产品：
- **Rundeck**（功能最像，但 Rundeck 是服务端的，本项目是单机桌面版）
- **n8n**（节点更通用，本项目专注本地命令）
- **VS Code Tasks**（轻量但不能 DAG 和可视化）

### 1.2 核心能力

| 能力 | 说明 |
|---|---|
| 命令库管理 | 注册/编辑/分组/版本化命令 |
| 参数化执行 | 每个命令声明参数（类型/校验/默认值），执行时填表 |
| 多种节点 | 命令、脚本、HTTP、AI、文件、延时、条件、循环 |
| DAG 工作流 | 节点编排为 DAG，串行/并行/条件分支/数据传递 |
| 多种编辑方式 | 画布拖拽、列表依赖、YAML 文本，三种互转 |
| 实时输出 | stdout/stderr 实时流式显示，类 IDE 控制台 |
| 调度执行 | 手动触发 + Cron 定时 + OS 计划任务（应用关闭也能跑） |
| 历史与回放 | 每次执行有完整记录，可重放 |
| 安全 | 危险命令确认 + 黑名单 + 命令预览 + 可中断 + 敏感字段加密 |
| 导入导出 | YAML 格式，方便分享、备份、Git 同步 |

### 1.3 适用场景

- 开发者日常：构建脚本、清理临时文件、批量处理项目
- 运维：定时巡检、日志归档、备份
- 数据：定时抓取、ETL 流水、报表生成
- AI 应用：把 LLM 调用和其他本地处理串起来

---

## 2. 技术栈

### 2.1 总览

```
┌─────────────────────────────────────────────────┐
│  Frontend  │  React 18 + TypeScript + Vite      │
│            │  React Flow (DAG 画布)              │
│            │  TanStack Query (数据同步)          │
│            │  shadcn/ui + TailwindCSS            │
│            │  Monaco Editor (YAML)               │
│            │  xterm.js (终端输出)                │
├─────────────────────────────────────────────────┤
│  Backend   │  Tauri 2 (Rust)                     │
│  (Core)    │  SQLite (rusqlite + r2d2)           │
│            │  tokio (异步运行时)                 │
│            │  keyring crate (敏感参数加密)        │
│            │  cron crate (Cron 解析)              │
│            │  reqwest (HTTP 节点)                │
│            │  notify-rust (系统通知)              │
├─────────────────────────────────────────────────┤
│  System    │  Windows / macOS / Linux            │
│            │  WebView2 / WKWebView / WebKitGTK   │
└─────────────────────────────────────────────────┘
```

### 2.2 选型理由

| 选择 | 原因 |
|---|---|
| **Tauri 2** | 比 Electron 体积小 10x，原生 shell 能力，Rust 后端性能好 |
| **Rust 后端** | 进程管理是核心场景，Rust 在子进程、流处理、错误处理上比 Node 强 |
| **SQLite** | 单文件、零运维、跨平台，足够单机使用 |
| **React Flow** | 主流 DAG 画布库，社区活跃，可定制性强 |
| **xterm.js** | 终端输出渲染的最佳选择，支持 ANSI 颜色 |
| **shadcn/ui** | 可复制可改的组件库，不锁死风格 |

### 2.3 依赖前端 package 概览

```jsonc
{
  "dependencies": {
    "react": "^18",
    "react-dom": "^18",
    "@tanstack/react-query": "^5",
    "reactflow": "^11",
    "@monaco-editor/react": "^4",
    "@xterm/xterm": "^5",
    "zod": "^3",                 // schema 校验
    "react-hook-form": "^7",      // 参数表单
    "lucide-react": "^0.4",       // 图标
    "class-variance-authority": "^0.7"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2",
    "typescript": "^5",
    "vite": "^5",
    "tailwindcss": "^3"
  }
}
```

### 2.4 Rust crate 概览

```toml
[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-shell = "2"
tauri-plugin-fs = "2"
tauri-plugin-dialog = "2"
tauri-plugin-notification = "2"
rusqlite = { version = "0.31", features = ["bundled"] }
r2d2 = "0.8"
r2d2_sqlite = "0.24"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
keyring = "2"
cron = "0.12"
reqwest = { version = "0.12", features = ["json", "stream"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
```

---

## 3. 系统架构

### 3.1 总体架构

```
┌─────────────────────────────────────────────────────────┐
│                    Tauri Webview (UI)                    │
│   ┌──────────┬──────────┬──────────┬──────────────┐     │
│   │ Library  │  Editor  │  Runner  │   History    │     │
│   │ 命令库   │  DAG编辑 │  执行面板 │   历史回放   │     │
│   └──────────┴──────────┴──────────┴──────────────┘     │
│           │            │            │            │      │
│           └────────────┴─────┬──────┴────────────┘      │
│                              │ Tauri IPC                 │
│                              │ (invoke / event)          │
├──────────────────────────────┼──────────────────────────┤
│                    Tauri Core (Rust)                     │
│   ┌─────────┬─────────┬──────────┬─────────┬────────┐   │
│   │ Command │ DAG     │  Exec    │ Sched   │  Im/Ex │   │
│   │ Registry│ Engine  │  Engine  │ Service │  YAML  │   │
│   └─────────┴─────────┴──────────┴─────────┴────────┘   │
│       │           │          │          │                │
│       └───────────┴────┬─────┴──────────┘                │
│                        ▼                                  │
│   ┌──────────────────────────────────────────────┐       │
│   │  Storage Layer (SQLite Pool + Keyring)       │       │
│   └──────────────────────────────────────────────┘       │
└──────────────────────────────────────────────────────────┘
                         │
                         ▼
            ┌──────────────────────────┐
            │  OS: cmd / pwsh / python │
            │  / node / shell / Task   │
            │   Scheduler / launchd    │
            └──────────────────────────┘
```

### 3.2 模块职责

| 模块 | 职责 |
|---|---|
| **Command Registry** | 命令/节点的 CRUD、版本管理、分类、搜索 |
| **DAG Engine** | DAG 解析、拓扑排序、依赖图、循环检测 |
| **Exec Engine** | 子进程管理、流式输出捕获、超时控制、取消 |
| **Sched Service** | 调度规则解析、触发决策、定时任务持久化 |
| **Param System** | 参数 schema 校验、表单生成、变量插值、加密 |
| **Storage** | SQLite 连接池、迁移、查询、加密字段读写 |
| **Im/Ex** | YAML 序列化/反序列化、版本兼容、冲突合并 |
| **Event Bus** | Tauri 事件总线，把执行进度实时推给前端 |

### 3.3 进程模型

- **主进程**：Tauri 主进程，UI、业务逻辑、调度器、IPC 处理
- **执行子进程**：每个被执行的命令是独立 OS 进程，通过 `tokio::process::Command` 启动
- **调度器**：在主进程的 tokio runtime 中跑，单一调度循环
- **不引入独立 worker 进程**：单机场景，复杂 worker 进程会带来部署和通信成本

---

## 4. 数据模型

### 4.1 ER 图

```
commands (命令库)
    │ 1
    │
    │ N
versions (命令版本)
    │ 1
    │
    │ N
params (参数定义)
    
workflows (工作流/DAG)
    │ 1
    │
    │ N
nodes (DAG 节点)

nodes
    │ N
    │
    │ M
edges (节点连线/依赖)

executions (执行记录)
    │ 1
    │
    │ N
node_runs (节点级执行记录)

schedules (定时规则)
    │
    ▼
workflows (1:1 或 1:N)

blacklist (危险命令黑名单)
```

### 4.2 核心表 Schema

```sql
-- 命令库：一个 command 是一类任务（如 "git push"），可有多个版本
CREATE TABLE commands (
    id            TEXT PRIMARY KEY,        -- uuid
    name          TEXT NOT NULL UNIQUE,    -- 显示名
    description   TEXT,
    category      TEXT,                    -- 分类："git"/"deploy"/"cleanup"...
    type          TEXT NOT NULL,           -- 'cmd' | 'pwsh' | 'python' | 'node' | 'bash' | 'script'
    current_ver   INTEGER NOT NULL DEFAULT 1,
    tags          TEXT,                    -- JSON 数组
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

-- 命令版本快照（保留历史，可回滚）
CREATE TABLE command_versions (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    command_id    TEXT NOT NULL REFERENCES commands(id) ON DELETE CASCADE,
    version       INTEGER NOT NULL,
    template      TEXT NOT NULL,           -- 模板字符串，含 {{param}} 占位符
    working_dir   TEXT,                    -- 工作目录，可含占位符
    env           TEXT,                    -- JSON: {KEY: "value or {{param}}"}
    timeout_ms    INTEGER,                 -- NULL = 无超时
    shell         TEXT,                    -- NULL = 默认 shell
    note          TEXT,                    -- 版本说明
    created_at    INTEGER NOT NULL,
    UNIQUE(command_id, version)
);

-- 参数定义
CREATE TABLE params (
    id            TEXT PRIMARY KEY,
    command_ver_id INTEGER NOT NULL REFERENCES command_versions(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,           -- 参数名，对应模板中的 {{name}}
    label         TEXT NOT NULL,           -- UI 显示名
    type          TEXT NOT NULL,           -- 'text'|'textarea'|'number'|'boolean'|'select'|'multiselect'|'file'|'dir'|'password'|'json'
    required      BOOLEAN NOT NULL DEFAULT 0,
    default_value TEXT,                    -- JSON
    options       TEXT,                    -- JSON 数组（select/multiselect 用）
    validation    TEXT,                    -- JSON: {min, max, pattern, ...}
    sensitive     BOOLEAN NOT NULL DEFAULT 0,  -- 敏感字段（加密存储）
    description   TEXT,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    UNIQUE(command_ver_id, name)
);

-- 工作流（DAG）
CREATE TABLE workflows (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    description   TEXT,
    enabled       BOOLEAN NOT NULL DEFAULT 1,
    trigger_type  TEXT NOT NULL DEFAULT 'manual',  -- 'manual' | 'schedule' | 'event'
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

-- DAG 节点
CREATE TABLE nodes (
    id            TEXT PRIMARY KEY,        -- 节点实例 ID
    workflow_id   TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    type          TEXT NOT NULL,           -- 'cmd' | 'script' | 'http' | 'ai' | 'file' | 'delay' | 'condition' | 'loop' | 'subworkflow'
    command_id    TEXT REFERENCES commands(id),  -- type=cmd/script 时引用
    config        TEXT NOT NULL,           -- JSON: 节点级配置（覆盖/补充）
    position_x    REAL NOT NULL DEFAULT 0,
    position_y    REAL NOT NULL DEFAULT 0,
    sort_order    INTEGER NOT NULL DEFAULT 0
);

-- DAG 边（依赖关系）
CREATE TABLE edges (
    id            TEXT PRIMARY KEY,
    workflow_id   TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    source_node   TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    target_node   TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    source_port   TEXT DEFAULT 'out',      -- 多输出端口
    target_port   TEXT DEFAULT 'in',
    condition     TEXT                     -- JSON: {type: 'on_success'|'on_failure'|'always'|'expr', ...}
);

-- 执行记录
CREATE TABLE executions (
    id            TEXT PRIMARY KEY,
    workflow_id   TEXT NOT NULL REFERENCES workflows(id),
    trigger       TEXT NOT NULL,           -- 'manual' | 'schedule' | 'event' | 'api'
    status        TEXT NOT NULL,           -- 'pending'|'running'|'success'|'failed'|'cancelled'|'timeout'
    started_at    INTEGER,
    finished_at   INTEGER,
    duration_ms   INTEGER,
    input_params  TEXT,                    -- JSON: 运行时参数
    error         TEXT,                    -- 错误摘要
    env_snapshot  TEXT                     -- JSON: 执行时环境快照（用于回放）
);

-- 节点级执行记录
CREATE TABLE node_runs (
    id            TEXT PRIMARY KEY,
    execution_id  TEXT NOT NULL REFERENCES executions(id) ON DELETE CASCADE,
    node_id       TEXT NOT NULL,
    status        TEXT NOT NULL,
    started_at    INTEGER,
    finished_at   INTEGER,
    duration_ms   INTEGER,
    exit_code     INTEGER,
    stdout        TEXT,                    -- 完整输出
    stderr        TEXT,
    output_data   TEXT,                    -- JSON: 节点 output 供下游引用
    error         TEXT
);

-- 输出日志（流式分片，支持大输出）
CREATE TABLE run_logs (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    node_run_id   TEXT NOT NULL REFERENCES node_runs(id) ON DELETE CASCADE,
    stream        TEXT NOT NULL,           -- 'stdout' | 'stderr' | 'system'
    content       TEXT NOT NULL,
    ts            INTEGER NOT NULL
);

-- 调度规则
CREATE TABLE schedules (
    id            TEXT PRIMARY KEY,
    workflow_id   TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    cron_expr     TEXT NOT NULL,
    timezone      TEXT DEFAULT 'local',
    enabled       BOOLEAN NOT NULL DEFAULT 1,
    last_run_at   INTEGER,
    next_run_at   INTEGER,
    mode          TEXT NOT NULL DEFAULT 'in_app',  -- 'in_app' | 'os_native' | 'hybrid'
    os_task_id    TEXT,                    -- OS 计划任务 ID（注册到 Task Scheduler 时用）
    created_at    INTEGER NOT NULL
);

-- 危险命令黑名单
CREATE TABLE blacklist (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern       TEXT NOT NULL,           -- 正则或字面量
    description   TEXT,
    enabled       BOOLEAN NOT NULL DEFAULT 1
);

-- 全局设置
CREATE TABLE settings (
    key           TEXT PRIMARY KEY,
    value         TEXT NOT NULL
);

-- 索引
CREATE INDEX idx_nodes_workflow ON nodes(workflow_id);
CREATE INDEX idx_edges_workflow ON edges(workflow_id);
CREATE INDEX idx_node_runs_exec ON node_runs(execution_id);
CREATE INDEX idx_executions_workflow ON executions(workflow_id);
CREATE INDEX idx_run_logs_node_run ON run_logs(node_run_id);
```

### 4.3 YAML 格式（导入导出）

```yaml
version: 1
kind: workflow
metadata:
  name: 部署到测试环境
  description: 拉代码 → 构建 → 上传 → 重启
  tags: [deploy, test]
  trigger:
    type: manual  # manual | schedule
    schedule:
      cron: "0 2 * * *"
      timezone: Asia/Shanghai

nodes:
  - id: pull
    type: cmd
    command_ref: git-pull   # 引用已注册的命令
    overrides:              # 节点级覆盖
      timeout_ms: 60000
    position: { x: 0, y: 0 }

  - id: build
    type: cmd
    command_ref: npm-build
    position: { x: 200, y: 0 }

  - id: upload
    type: script
    config:
      language: python
      code: |
        import shutil
        shutil.make_archive("dist", "zip", "build")
        # upload to s3 ...
    position: { x: 400, y: 0 }

  - id: notify
    type: ai
    config:
      provider: openai
      model: gpt-4o-mini
      prompt: "部署完成，请用一句话总结：{{build.stdout}}"
    position: { x: 600, y: 0 }

edges:
  - from: pull
    to: build
    condition: { type: on_success }
  - from: build
    to: upload
    condition: { type: on_success }
  - from: upload
    to: notify
    condition: { type: always }
```

---

## 5. 节点系统

### 5.1 节点分类

#### 5.1.1 命令节点 (`cmd`)

直接执行一个已注册的命令实例。

```yaml
type: cmd
config:
  command_ref: git-push       # 引用 commands 表
  param_overrides:            # 覆盖默认参数
    branch: main
  cwd: "{{workspace}}/repo"   # 节点级工作目录
  env:
    GIT_AUTHOR_NAME: "bot"
  timeout_ms: 30000
```

#### 5.1.2 脚本节点 (`script`)

直接执行一段脚本，无需先注册到命令库。适合一次性逻辑。

```yaml
type: script
config:
  language: python            # python | node | bash | pwsh
  code: |
    import os
    files = os.listdir("{{input_dir}}")
    print(f"Found {len(files)} files")
  timeout_ms: 10000
```

#### 5.1.3 HTTP 节点 (`http`)

调用一个 HTTP API，常用于触发 webhook、调外部服务。

```yaml
type: http
config:
  method: POST
  url: https://api.example.com/notify
  headers:
    Authorization: "Bearer {{api_token}}"
  body:
    type: json
    content:
      msg: "{{prev.stdout}}"
  timeout_ms: 10000
  retry: { count: 3, backoff: exponential }
```

#### 5.1.4 AI 节点 (`ai`)

调用大模型做转换/总结/分类。

```yaml
type: ai
config:
  provider: openai            # openai | anthropic | ollama | 自定义 endpoint
  model: gpt-4o-mini
  system: "你是一个日志分析助手"
  prompt: "请总结以下错误：\n{{build.stderr}}"
  temperature: 0.3
  output_format: text         # text | json
  api_key_ref: openai_key     # 引用 keyring 中存储的 key
```

#### 5.1.5 文件节点 (`file`)

文件 IO 操作：读、写、复制、移动、删除、监听。

```yaml
type: file
config:
  action: copy                # read|write|copy|move|delete|watch
  src: "{{workspace}}/build.zip"
  dst: "{{workspace}}/dist/"
```

#### 5.1.6 延时节点 (`delay`)

等待一段时间，可用于限流、重试间隔。

```yaml
type: delay
config:
  duration_ms: 5000
```

#### 5.1.7 条件节点 (`condition`)

分支判断，根据表达式结果走不同分支。

```yaml
type: condition
config:
  expression: "{{build.exit_code}} == 0"
  true_branch: deploy-prod
  false_branch: rollback
```

表达式语言：精简版 JS 表达式（如 `n8n` 的 `$node["build"].json.exitCode === 0`），用 `rhai` 或自定义 mini-DSL 解析。

#### 5.1.8 循环节点 (`loop`)

对数组或数字范围迭代执行子图。

```yaml
type: loop
config:
  over: "{{pull.branches}}"   # 数组或数字
  iterator_var: branch
  body:                       # 子图（节点列表 + 边）
    nodes:
      - { id: deploy, type: cmd, command_ref: deploy-branch }
    edges: []
```

#### 5.1.9 子工作流节点 (`subworkflow`)

嵌套调用另一个工作流。

```yaml
type: subworkflow
config:
  workflow_ref: cleanup-tmp
  pass_params: [input_dir]
```

### 5.2 节点抽象接口（Rust）

```rust
#[async_trait]
pub trait Node: Send + Sync {
    type Input: DeserializeOwned;
    type Output: Serialize;
    type Config: DeserializeOwned;

    fn type_id(&self) -> &'static str;
    fn schema(&self) -> NodeSchema;  // 参数 + UI 提示
    async fn validate(&self, cfg: &Self::Config) -> Result<(), NodeError>;
    async fn run(&self, ctx: NodeContext, cfg: Self::Config, input: Self::Input)
        -> Result<NodeRunOutcome<Self::Output>, NodeError>;
}

pub struct NodeRunOutcome<T> {
    pub output: T,
    pub branches: HashMap<String, ()>,  // 条件节点用，标记走哪个分支
}

pub struct NodeContext {
    pub run_id: Uuid,
    pub workspace: PathBuf,
    pub vars: Arc<RwLock<HashMap<String, Value>>>,
    pub emit: EventSink,    // 流式事件回调
    pub cancel: CancellationToken,
}
```

### 5.3 节点注册表

```rust
pub struct NodeRegistry {
    nodes: HashMap<&'static str, Box<dyn NodeFactory>>,
}

impl NodeRegistry {
    pub fn default_registry() -> Self {
        let mut r = Self::new();
        r.register(CmdNode::factory());
        r.register(ScriptNode::factory());
        r.register(HttpNode::factory());
        r.register(AiNode::factory());
        r.register(FileNode::factory());
        r.register(DelayNode::factory());
        r.register(ConditionNode::factory());
        r.register(LoopNode::factory());
        r.register(SubWorkflowNode::factory());
        r
    }
}
```

---

## 6. DAG 引擎

### 6.1 核心数据结构

```rust
pub struct Dag {
    pub nodes: HashMap<NodeId, DagNode>,
    pub edges: Vec<DagEdge>,
}

pub struct DagNode {
    pub id: NodeId,
    pub node_type: String,
    pub config: serde_json::Value,
    pub position: (f32, f32),
}

pub struct DagEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub condition: EdgeCondition,
}
```

### 6.2 引擎能力

| 能力 | 实现 |
|---|---|
| 拓扑排序 | Kahn 算法，得到合法执行顺序 |
| 环检测 | DFS 三色标记，发现环立即报错 |
| 并行层划分 | 拓扑序后，相同「深度」的可并行 |
| 依赖分析 | 哪些节点可以并行（无共同祖先依赖） |
| 增量校验 | 编辑时实时检测环、孤立节点、必填参数 |

### 6.3 执行模型

```
                ┌──── Ready Queue ────┐
                │   N1    N3          │
                └─────────────────────┘
                       │
                       ▼ (tokio::spawn)
   ┌──────────── Worker Pool (并发 N=4) ────────────┐
   │  Worker1: N1  → 完成 → 把下游 N4 加入 Ready    │
   │  Worker2: N3  → 完成 → 把下游 N5 加入 Ready    │
   └────────────────────────────────────────────────┘
```

- **就绪队列**：依赖全部满足的节点
- **Worker pool**：默认 4 并发（可配置），用 `tokio::sync::Semaphore` 限流
- **完成回调**：节点完成后把下游节点加入就绪队列（条件分支节点会标记走哪条分支）
- **错误传播**：默认「任一节点失败即停止」（可配置为「继续执行」或「仅关键节点失败停止」）
- **取消**：通过 `CancellationToken` 传播，UI 取消 → token.cancel() → 所有运行中节点收到信号

### 6.4 数据传递

节点间通过「命名变量」传递数据：

```yaml
# 上游节点 output
output_data:
  result: "success"
  count: 42
  items: [a, b, c]

# 下游节点引用
prompt: "处理 {{build.count}} 个项目，列表：{{build.items}}"
```

存储方式：每个 execution 维护一个 `vars: HashMap<NodeId, Value>`，节点完成后写入。下游通过 `{{nodeId.field}}` 引用。

### 6.5 条件边语法

```yaml
# 简单条件
- from: build
  to: deploy
  condition: { type: on_success }    # 上游 exit 0 才触发

- from: build
  to: rollback
  condition: { type: on_failure }    # 上游非 0 触发

- from: build
  to: notify
  condition: { type: always }        # 无论成败都触发

# 表达式条件
- from: check
  to: deploy-prod
  condition:
    type: expr
    expr: '{{check.exit_code}} == 0 && {{check.branch}} == "main"'
```

---

## 7. 参数系统

### 7.1 参数类型

| 类型 | UI | 存储 | 校验 |
|---|---|---|---|
| `text` | 单行输入框 | 字符串 | pattern (regex) |
| `textarea` | 多行文本框 | 字符串 | maxLength |
| `number` | 数字输入 | 数字 | min/max/step |
| `boolean` | 开关 | bool | - |
| `select` | 下拉单选 | 字符串 | enum |
| `multiselect` | 多选下拉 | 字符串数组 | enum |
| `file` | 文件选择器 | 路径字符串 | exists |
| `dir` | 目录选择器 | 路径字符串 | exists |
| `password` | 密码框 | 加密字符串 | - |
| `json` | JSON 编辑器 | 任意 | JSON Schema |
| `datetime` | 日期时间选择 | ISO 字符串 | - |
| `cron` | Cron 表达式 | 字符串 | cron 解析 |

### 7.2 变量插值

模板字符串中 `{{name}}` 在执行时替换。支持的引用语法：

```
{{paramName}}                    # 命令参数
{{nodeId.field}}                 # 上游节点 output 字段
{{nodeId.stdout}}                # 上游 stdout 全文
{{nodeId.exit_code}}             # 上游退出码
{{nodeId.stderr}}                # 上游 stderr 全文
{{env.VAR_NAME}}                 # 环境变量
{{now}}                          # 当前时间 ISO
{{now|format:"YYYY-MM-DD"}}     # 格式化时间（可选）
{{$secret.name}}                 # keyring 加密 secret
```

实现：用 `handlebars` 风格的自定义插值器，支持嵌套字段访问。

### 7.3 敏感参数

- UI 上 `password` 类型输入框，遮罩显示
- 存储：`value` 写入 SQLite 前用 OS keyring 加密（Windows DPAPI / macOS Keychain / Linux Secret Service）
- 运行时：解密后注入子进程环境变量或命令行，**不写日志、不持久化到执行历史 input_params**
- 用户可在设置里管理 keyring 条目

### 7.4 表单生成

执行工作流时，前端根据节点的参数定义自动生成表单：

```
┌─ 工作流：部署到测试环境 ─────────────────────┐
│                                              │
│  节点 1: git pull                            │
│    仓库路径 [/path/to/repo        ] [选择]   │
│    分支   [main                  ] [▼]       │
│                                              │
│  节点 2: build                               │
│    环境   (●) dev  ( ) prod                  │
│    附加参数 [...                  ]           │
│                                              │
│  节点 3: upload                              │
│    API Token [***********        ]           │
│                                              │
│           [取消]  [预览]  [▶ 执行]            │
└──────────────────────────────────────────────┘
```

---

## 8. 执行引擎

### 8.1 子进程管理

```rust
pub struct ProcessRunner {
    pub async fn run(&self, spec: RunSpec, ctx: NodeContext)
        -> Result<NodeRunOutcome<Value>, NodeError>
    {
        let mut cmd = match spec.shell {
            Shell::Cmd => Command::new("cmd"),
            Shell::PowerShell => Command::new("pwsh"),
            Shell::Bash => Command::new("bash"),
            _ => Command::new(&spec.executable),
        };
        cmd.args(spec.args)
           .current_dir(spec.cwd)
           .envs(spec.env)
           .stdout(Stdio::piped())
           .stderr(Stdio::piped())
           .stdin(Stdio::null());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        // 启动流读取任务
        let stdout_task = tokio::spawn(read_stream(stdout, ctx.emit.stdout_sink()));
        let stderr_task = tokio::spawn(read_stream(stderr, ctx.emit.stderr_sink()));

        // 等待 + 超时
        let result = tokio::time::timeout(
            spec.timeout,
            child.wait_with_cancel(&ctx.cancel),
        ).await;

        // ...
    }
}
```

### 8.2 流式事件协议

后端通过 Tauri event 把日志实时推到前端：

```rust
#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunEvent {
    ExecutionStarted { execution_id: Uuid, workflow_id: Uuid },
    NodeStarted { execution_id: Uuid, node_id: String },
    NodeLog { execution_id: Uuid, node_id: String,
              stream: String, chunk: String, ts: i64 },
    NodeFinished { execution_id: Uuid, node_id: String,
                   exit_code: i32, output: serde_json::Value },
    ExecutionFinished { execution_id: Uuid, status: String },
}
```

前端订阅 `run-event-{execution_id}` 频道，xterm.js 渲染。

### 8.3 取消与超时

- **超时**：`tokio::time::timeout`，到时先 SIGTERM，等待 5s 未退出 SIGKILL
- **取消**：UI 点取消 → 后端 `CancellationToken::cancel()` → 节点收到信号优雅退出 → 子进程 SIGTERM
- **Windows 进程组**：用 `CREATE_NEW_PROCESS_GROUP` 创建，便于批量 kill

---

## 9. 调度系统

### 9.1 调度模型

```
┌─────────── 应用内置调度器 ──────────────┐
│                                         │
│  Tick (1s) → 查询 next_run_at 任务     │
│           → 命中时间 → 触发执行         │
│           → 更新 last_run_at, next_run │
│                                         │
│  并行：与 OS 计划任务互斥（同 ID 不会  │
│        双触发）                        │
└─────────────────────────────────────────┘
                    ↕
┌─────────── OS 计划任务 ─────────────────┐
│                                         │
│  Windows: Task Scheduler                │
│  macOS:   launchd                       │
│  Linux:   systemd timer                 │
│                                         │
│  通过 `cmdflow run <workflow_id>` 触发 │
│  应用关闭时也能跑（仅触发，执行仍需     │
│  调起应用）                             │
└─────────────────────────────────────────┘
```

### 9.2 调度模式

每条 `schedules` 记录有 `mode` 字段：
- `in_app`：只在应用内调度，应用关闭就不跑
- `os_native`：注册到 OS 计划任务，应用关闭也能触发
- `hybrid`：应用内调度为主，注册 OS 任务做兜底

### 9.3 OS 计划任务注册

Windows：通过 `schtasks /create` 注册，任务动作是 `cmdflow.exe run --workflow <id> --scheduled`。

macOS：写 plist 到 `~/Library/LaunchAgents/`。

Linux：写 systemd user unit。

### 9.4 时区与夏令时

- Cron 表达式可配时区
- 计算 next_run_at 时考虑时区
- 内部统一用 UTC 存储，显示时转本地

---

## 10. 安全策略

### 10.1 防护层

```
┌─ Layer 1: 黑名单 ─────────────────────────────┐
│  内置危险模式:                                 │
│    rm -rf /, rm -rf ~                          │
│    format c:                                   │
│    del /f /s /q C:\                            │
│    dd if=... of=/dev/...                       │
│    :(){:|:&};:  (fork bomb)                    │
│                                                 │
│  用户可加自定义 pattern                         │
│  命中 → 阻止执行，必须显式 override             │
└─────────────────────────────────────────────────┘
                     ↓
┌─ Layer 2: 危险命令确认 ────────────────────────┐
│  启发式：包含 rm/del/fmt/drop/... 关键字        │
│  → 执行前弹窗：显示最终命令、用户确认           │
└─────────────────────────────────────────────────┘
                     ↓
┌─ Layer 3: 预览 ────────────────────────────────┐
│  执行前显示：最终命令行、工作目录、环境变量     │
│  用户可复制、保存、确认                         │
└─────────────────────────────────────────────────┘
                     ↓
┌─ Layer 4: 可中断 ──────────────────────────────┐
│  任何执行中的命令可一键取消                     │
└─────────────────────────────────────────────────┘
```

### 10.2 敏感参数保护

- `password` 类型自动用 keyring 加密
- 运行时解密，不写日志
- 执行历史 input_params 字段对敏感值做掩码（`abc***xyz`）

### 10.3 不做的事（MVP）

- 不做多用户/鉴权（单机单用户）
- 不做远程执行
- 不暴露网络端口（除 Tauri 内部 IPC）

---

## 11. UI 设计

### 11.1 主窗口布局

```
┌────────────────────────────────────────────────────────────────┐
│  ☰  CmdFlow                                       [⚙] [─][□][×] │
├──────┬─────────────────────────────────────────────────────────┤
│      │                                                          │
│ 📚   │                                                          │
│ 命令库│                                                          │
│      │              主内容区（视图切换）                          │
│ 🔀   │                                                          │
│ 工作流│                                                          │
│      │                                                          │
│ ▶    │                                                          │
│ 执行  │                                                          │
│      │                                                          │
│ ⏰   │                                                          │
│ 调度  │                                                          │
│      │                                                          │
│ 📜   │                                                          │
│ 历史  │                                                          │
│      │                                                          │
└──────┴─────────────────────────────────────────────────────────┘
```

### 11.2 视图

#### 命令库视图
- 左侧分类树 / 标签筛选
- 右侧命令卡片列表
- 点开 → 命令详情 + 参数配置
- 「新建命令」按钮 → 引导式创建向导

#### 工作流视图（DAG 编辑器）
- React Flow 画布
- 左侧节点面板（拖拽到画布）
- 右侧属性面板（选中节点/边时显示配置）
- 顶部工具栏：保存、运行、验证、导入、导出、视图切换
- 视图切换按钮：[画布] [列表] [YAML]（三种模式互相同步）

#### 执行视图
- 工作流参数表单（自动生成）
- 「预览」按钮显示最终命令
- 「▶ 执行」开始
- 底部 xterm.js 终端：实时输出，节点切换分 tab
- 节点状态指示：pending/running/success/failed
- 「⏹ 取消」按钮

#### 历史视图
- 表格：时间、工作流、状态、耗时、触发方式
- 点开 → 详情：每个节点的输入/输出/日志
- 「重放」按钮：用相同参数再跑一次

#### 调度视图
- 日历视图 + 列表视图
- 添加调度：选工作流 + Cron 表达式 + 模式
- 启用/禁用开关

### 11.3 主题

- 浅色 / 深色 / 跟随系统
- shadcn/ui 默认主题，可定制

---

## 12. 模块划分与目录

### 12.1 目录结构

```
CmdFlow/
├── DESIGN.md                       # 本文档
├── README.md
├── package.json
├── pnpm-lock.yaml
├── vite.config.ts
├── tsconfig.json
├── tailwind.config.js
├── index.html
├── src/                            # 前端
│   ├── main.tsx
│   ├── App.tsx
│   ├── components/
│   │   ├── ui/                     # shadcn 基础组件
│   │   ├── layout/                 # 主框架
│   │   ├── library/                # 命令库
│   │   ├── workflow/               # DAG 编辑器
│   │   │   ├── CanvasView.tsx
│   │   │   ├── ListView.tsx
│   │   │   ├── YamlView.tsx
│   │   │   ├── NodePanel.tsx
│   │   │   ├── PropertyPanel.tsx
│   │   │   └── nodeTypes/
│   │   ├── runner/                 # 执行面板
│   │   ├── history/
│   │   ├── schedule/
│   │   └── settings/
│   ├── hooks/                      # React Query hooks
│   ├── lib/
│   │   ├── tauri.ts                # IPC 封装
│   │   ├── interpolation.ts        # {{}} 插值
│   │   └── validation.ts
│   ├── stores/                     # zustand 状态
│   ├── types/                      # 与 Rust 共享的类型
│   └── styles/
├── src-tauri/                      # Rust 后端
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   └── src/
│       ├── main.rs
│       ├── lib.rs
│       ├── commands/               # Tauri command handlers
│       │   ├── mod.rs
│       │   ├── library.rs
│       │   ├── workflow.rs
│       │   ├── execution.rs
│       │   ├── schedule.rs
│       │   ├── history.rs
│       │   └── settings.rs
│       ├── core/
│       │   ├── mod.rs
│       │   ├── dag.rs              # DAG 解析
│       │   ├── executor.rs         # 执行引擎
│       │   ├── scheduler.rs        # 调度器
│       │   ├── interpolation.rs    # 变量插值
│       │   └── events.rs           # 事件总线
│       ├── nodes/                  # 节点实现
│       │   ├── mod.rs
│       │   ├── registry.rs
│       │   ├── cmd.rs
│       │   ├── script.rs
│       │   ├── http.rs
│       │   ├── ai.rs
│       │   ├── file.rs
│       │   ├── delay.rs
│       │   ├── condition.rs
│       │   ├── loop_node.rs
│       │   └── subworkflow.rs
│       ├── storage/
│       │   ├── mod.rs
│       │   ├── db.rs               # SQLite 连接池
│       │   ├── migrations.rs
│       │   └── models.rs           # 类型 + query
│       ├── security/
│       │   ├── mod.rs
│       │   ├── blacklist.rs
│       │   └── secrets.rs          # keyring
│       ├── scheduler_os/           # OS 计划任务集成
│       │   ├── mod.rs
│       │   ├── windows.rs
│       │   ├── macos.rs
│       │   └── linux.rs
│       └── import_export/
│           ├── mod.rs
│           └── yaml.rs
├── migrations/                     # SQL 迁移
│   ├── 0001_init.sql
│   └── ...
└── docs/
    ├── architecture.md
    ├── node-dev.md                 # 自定义节点开发指南
    └── user-guide.md
```

---

## 13. 实施计划

### 13.1 阶段划分

#### Phase 0 — 脚手架 (1 周)
- [ ] 初始化 Tauri 项目 + Vite + React + TS
- [ ] 配置 Tailwind + shadcn/ui
- [ ] SQLite 连接池 + 迁移框架
- [ ] Tauri command 注册样板
- [ ] 基础主框架（侧边栏 + 路由）

#### Phase 1 — 命令库 + 单命令执行 (1.5 周)
- [ ] 命令 CRUD（增删改查 + 版本管理）
- [ ] 参数定义（全部类型）
- [ ] 执行引擎 MVP（单命令，无 DAG）
- [ ] xterm.js 流式输出
- [ ] 黑名单 + 危险命令确认
- [ ] 命令库 UI（列表 + 编辑器）

#### Phase 2 — DAG 工作流 (2 周)
- [ ] DAG 模型 + 拓扑排序 + 环检测
- [ ] React Flow 画布编辑器
- [ ] 节点面板 + 属性面板
- [ ] 工作流执行引擎（多节点、并行、取消）
- [ ] 列表式/YAML 编辑模式
- [ ] 互转同步机制

#### Phase 3 — 调度 + 历史 (1 周)
- [ ] 应用内置调度器
- [ ] OS 计划任务集成（Windows 优先）
- [ ] 执行历史持久化
- [ ] 历史回放

#### Phase 4 — 高级节点 + 完善 (1.5 周)
- [ ] HTTP 节点
- [ ] AI 节点
- [ ] 条件/循环节点
- [ ] 文件/延时节点
- [ ] 子工作流
- [ ] 导入导出 YAML
- [ ] 主题 + 设置页
- [ ] 打包发布

#### Phase 5 — 打磨 (持续)
- [ ] 性能优化
- [ ] 错误处理完善
- [ ] 国际化（先中后英）
- [ ] 用户文档
- [ ] 自动更新

**总 MVP（Phase 0-1）预估 2.5 周**
**核心完整版（Phase 0-3）预估 5.5 周**
**全部完成（Phase 0-4）预估 7 周**

### 13.2 风险点

| 风险 | 应对 |
|---|---|
| DAG 画布 + 三视图同步复杂 | 先做画布，列表/YAML 后期加；同步通过单一 JSON state |
| Tauri 2 生态相对新 | 关键路径先做 PoC 验证 |
| OS 计划任务跨平台 | 先 Windows，macOS/Linux 后置 |
| AI 节点 API 差异 | 抽象 provider 接口，先实现 OpenAI 兼容 |
| 大输出卡 UI | xterm.js 性能好，必要时用虚拟滚动 |

---

## 14. 关键设计决策

| 决策 | 选择 | 替代方案 | 理由 |
|---|---|---|---|
| 后端语言 | Rust | Node.js | 子进程/流处理/性能更好 |
| 前端框架 | React | Vue/Svelte | React Flow 生态最好 |
| DAG 画布 | React Flow | 自研/draw.io | 成熟、可定制、社区大 |
| 本地存储 | SQLite | JSON/LevelDB | 单文件、可靠、跨平台 |
| 调度器 | 应用内 + OS | 仅 OS | 应用内体验好，OS 兜底 |
| 敏感参数 | OS keyring | 自实现加密 | 跨平台、零配置、安全 |
| 节点配置 | JSON + UI schema | 纯 YAML | 既能 UI 改又能 YAML 改 |
| 命令执行 | `tokio::process` | 独立 worker 进程 | 简单够用 |
| 流式输出 | Tauri event | WebSocket | Tauri 内部通道足够 |

---

## 15. 后续可扩展

- 插件系统：用户写自定义节点（Rust 或 WASM）
- 远程执行：可选地连接远程 agent
- AI 节点：内置 function calling
- 模板市场：社区共享工作流模板
- 团队协作：可选地接入 git 同步、评论

---

## 附录 A：示例工作流

### A.1 日常开发

```
[git pull] → [npm install] → [npm test] → [git push]
```

### A.2 部署流水线

```
[pull] ─┬─→ [build] ─→ [test] ─→ [upload] ─→ [notify slack]
        │
        └─→ [lint] ─→ [report]
```

### A.3 数据备份

```
[compress logs] → [upload to s3] → [delete old] → [notify email]
                 ↓ 失败
                 [alert admin]
```

---

## 附录 B：术语表

| 术语 | 含义 |
|---|---|
| Command | 命令库中注册的一个可执行单元 |
| Node | DAG 中的一个执行点 |
| Workflow | 由节点和边组成的 DAG |
| Execution | 工作流的一次执行 |
| Node Run | 执行中单个节点的运行实例 |
| Trigger | 触发执行的方式（manual/schedule/event） |
| Param | 命令或节点的参数 |
| Variable | 节点间传递的命名数据 |

---

**文档版本**：v1.0
**最后更新**：2026-07-13
