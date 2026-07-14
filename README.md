# CmdFlow

> 本地命令编排工具：可视化编排 + 参数化执行 + DAG 工作流 + 定时调度

CmdFlow 是一个桌面应用，让你把日常用到的 `cmd` / `PowerShell` / `python` / `node` 脚本、HTTP 请求、AI 调用等「本地会跑的东西」集中起来，做成可配置、可参数化、可串联、可定时的任务流。

## 特性

- 🖥️ **本地优先** — 全部在你的机器上跑，数据本地 SQLite 存储
- 🧱 **命令库** — 注册/版本化/分组命令，每个命令可声明参数（12 种类型）
- 🔀 **DAG 工作流** — 9 种节点类型，画布拖拽、并行执行、条件分支、数据传递
- ⏰ **定时调度** — Cron 表达式，可注册为 OS 计划任务
- 📺 **实时输出** — xterm.js 流式显示，多节点 tab 切换
- 🔒 **安全** — 黑名单拦截 + 危险命令确认 + 敏感字段 keyring 加密
- 💾 **历史回放** — 每次执行完整记录，可一键重放
- 🎨 **主题** — 浅色 / 深色 / 跟随系统

## 技术栈

- **后端**：Rust + Tauri 2 + tokio + SQLite + reqwest + cron
- **前端**：React 18 + TypeScript + Vite + TailwindCSS + shadcn/ui
- **画布**：React Flow
- **终端**：xterm.js
- **存储**：SQLite (主存) + YAML (导入导出)

## 快速开始

```bash
# 1. 安装依赖
pnpm install

# 2. 开发模式
pnpm tauri:dev

# 3. 生产构建
pnpm tauri:build
```

第一次跑会比较慢（Rust 全量编译 + 启动 Webview）。

## 节点类型

| 类型 | 类别 | 说明 |
|---|---|---|
| 命令 (`cmd`) | 核心 | 执行一个已注册的命令 |
| 脚本 (`script`) | 核心 | 直接执行一段 python/node/bash 脚本 |
| HTTP (`http`) | IO | 发送 HTTP 请求 |
| AI (`ai`) | IO | 调用大模型（OpenAI 兼容） |
| 文件 (`file`) | IO | 文件读写操作 |
| 延时 (`delay`) | 控制 | 等待一段时间 |
| 条件 (`condition`) | 控制 | 条件分支 |
| 循环 (`loop`) | 控制 | 对集合迭代 |
| 子工作流 (`subworkflow`) | 控制 | 嵌套调用另一个工作流 |

## 项目结构

```
CmdFlow/
├── DESIGN.md                   详细设计文档
├── src/                         前端 (React + TS)
│   ├── pages/                  6 个页面：命令库/工作流/执行/历史/调度/设置
│   ├── components/
│   │   ├── layout/             主框架
│   │   ├── workflow/           DAG 画布 + 节点/属性面板
│   │   ├── forms/              命令/参数编辑器
│   │   ├── runner/             xterm 终端
│   │   └── ui/                 Button / Card / Dialog / ...
│   ├── lib/                    IPC 封装 / utils
│   └── stores/                 zustand 状态
├── src-tauri/                  后端 (Rust)
│   └── src/
│       ├── core/               业务核心（interpolation / executor / scheduler / dag）
│       ├── commands/           IPC handlers
│       ├── nodes/              9 种节点实现
│       ├── storage/            SQLite 持久化
│       └── security/           黑名单 / 加密
├── migrations/                 SQL 迁移
└── scripts/                    工具脚本
```

## 开发命令

```bash
pnpm dev              # 纯前端 dev server
pnpm tauri:dev        # 完整 Tauri 开发
pnpm tauri:build      # 打包成 .msi / .exe / .dmg
pnpm exec tsc -b      # 类型检查
pnpm exec vite build  # 前端生产构建
cd src-tauri && cargo check     # Rust 类型检查
cd src-tauri && cargo test --lib  # Rust 单元测试
```

## 路线图

- [x] **Phase 0** 脚手架（Tauri 2 + React + SQLite + 基础布局）
- [x] **Phase 1** 命令库 + 单命令执行（含 12 种参数类型 + 黑名单 + xterm）
- [x] **Phase 2** DAG 工作流（7 种节点 + 画布编辑 + 拓扑并行执行）
- [x] **Phase 3** 调度 + 历史（Cron + OS 计划任务 + 执行历史 + 重放）
- [x] **Phase 4** AI 节点 + SubWorkflow 节点 + 主题切换 + 完善设置
- [x] **Phase 5** 性能优化 + 文档 + 错误处理

## License

MIT
