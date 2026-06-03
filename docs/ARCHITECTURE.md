# TheBrain 架构

## 1. 总体形态

TheBrain 是 Tauri 桌面应用，不是传统远程 Web 前后端分离架构。

```text
React + Vite UI
  -> src/api.ts
  -> Tauri commands
  -> Rust services
  -> Vault filesystem + .thebrain/index.sqlite
```

前端负责用户界面、交互状态和 Tauri command 调用。本地 Rust 服务负责文件系统、SQLite、路径安全、AI 调用、索引、移动、回滚和日志。

## 2. 数据边界

```text
Vault/
  用户 Markdown、图片、音频、文本等文件

Vault/000-收集箱/
  低摩擦输入区，AI 自动整理的主要入口

Vault/.thebrain/
  内部索引、队列、日志、Trace、RAG、预算、移动记录

Vault/.secrets/
  可选本地凭据，gitignored
```

原则：

- Vault 文件是用户资产。
- `.thebrain` 是内部状态，不是用户笔记。
- `.secrets` 不进入 Git，不进入 RAG，不进入日志。
- 用户应能在不使用 TheBrain 的情况下继续读取 Vault 中的 Markdown 文件。

## 3. 收集箱自动整理 Pipeline

目标 pipeline：

```text
新文件进入 000-收集箱
  -> 稳定等待与去重
  -> 内容抽取
  -> AI 分类与目标路径决策
  -> 冲突检查
  -> 自动移动
  -> 更新 ledger
  -> 写 movement log / audit events
  -> 生成 TODO / 日程 / 档案候选
  -> 可回滚
```

安全边界：

- 只处理 `000-收集箱/`。
- 不移动 `收集箱-已整理.md`。
- 不越过 Vault。
- 不覆盖已有文件。
- 所有移动有日志。
- 所有移动可回滚。

## 4. Markdown 与元数据

Markdown frontmatter 只保存适合用户可见的小型元数据：

- `title`
- `tags`
- `aliases`
- `created`
- `updated`
- `source`
- `source_type`
- `status`
- `classification`
- `thebrain_id`

不写入 frontmatter 的内部数据：

- embedding
- AI prompt/response
- token usage
- 移动历史
- 队列日志
- Trace
- 访问频率明细

这些数据进入 `.thebrain/index.sqlite`。

## 5. RAG Pipeline

当前 pipeline：

```text
扫描 Vault md/txt
  -> 跳过内部目录和 ledger
  -> Markdown 结构感知分块
  -> 写 rag_documents / rag_chunks
  -> 关键词检索
  -> local semantic placeholder
  -> 去重、归一化、top-k
  -> 构造上下文
  -> MiMo 或 fallback 回答
  -> 写引用和 Trace
```

后续目标：

- 真实 embedding。
- 向量检索。
- rerank。
- 多轮会话 UI。
- 跨文件长期记忆策略。
- 更强引用定位和打开能力。

## 6. TODO、日程和档案

TheBrain 后续需要建立从内容到行动和档案的模型：

```text
会议纪要 / 通话记录 / 笔记 / 便利贴
  -> 抽取人物、地点、联系方式、任务、时间
  -> 生成候选
  -> 自动或半自动写入 TODO / 日程 / 人名档案
  -> 保留来源引用
```

关键决策尚未完成：

- 档案数据写入 Markdown、SQLite，还是两者结合。
- TODO/日程是否自动确认。
- 联系方式等敏感信息如何显示、导出和保护。

## 7. 前端与后端关系

前端可以先展示完整产品愿景和交互壳，但必须清楚标注状态：

- 已可用。
- 实验。
- 占位。
- 后续。
- fallback。

后端逐步补齐真实能力。前端不应把未实现后端能力伪装成真实成功。

## 8. 当前主要服务

- `vault.rs`：Vault 初始化和路径安全。
- `importer.rs`：导入到收集箱。
- `ai.rs`：MiMo、抽取、整理决策和 fallback。
- `movement.rs`：文件移动、ledger、audit、回滚。
- `rag.rs`：RAG 索引和问答编排。
- `retrieval.rs`：检索通道和评分。
- `chunking.rs`：Markdown 分块。
- `rag_trace.rs`：Trace 持久化。
- `index.rs`：SQLite schema。
- `budget.rs` / `usage.rs`：预算和用量。
- `sticky.rs`：便利贴。
- `candidates.rs`：TODO/日程候选。
