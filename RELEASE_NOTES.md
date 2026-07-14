# CmdFlow v0.1.0

本地命令编排工具首发版。

## 特性

- **本地优先** — 全部在机器上跑，SQLite 本地存储
- **命令库** — 12 种参数类型 + 版本管理 + 黑名单
- **DAG 工作流** — 9 种节点 + 画布拖拽 + 并行执行
- **定时调度** — Cron + 应用内调度 + OS 计划任务
- **实时输出** — xterm.js 流式显示
- **安全** — 黑名单拦截 + 危险命令确认 + 敏感字段加密
- **历史回放** — 一键重放任意执行
- **AI 节点** — OpenAI 兼容（gpt-4o, claude via base_url, ollama）
- **主题** — 浅色 / 深色 / 跟随系统

## 节点类型 (9)

`cmd` `script` `http` `ai` `file` `delay` `condition` `loop` `subworkflow`

## 安装

下载对应版本：
- **MSI** (推荐): Windows Installer，适合企业部署
- **EXE** (NSIS): 单文件安装器，适合个人

系统要求: Windows 10/11 + WebView2 Runtime

## 技术栈

Tauri 2 · Rust · React 18 · TypeScript · SQLite · ReactFlow · xterm.js

## 文档

- [设计文档](https://github.com/ZhSMM/CmdFlow/blob/main/DESIGN.md)
- [用户指南](https://github.com/ZhSMM/CmdFlow/blob/main/docs/USER_GUIDE.md)
- [README](https://github.com/ZhSMM/CmdFlow#readme)

## 测试

14/14 单元测试通过 (interpolation / blacklist / scheduler)
