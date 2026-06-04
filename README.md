# TheBrain

TheBrain 是一个本地优先的个人外置大脑桌面应用。它以类 Obsidian 的 Markdown Vault 为基础，把收集箱、AI 自动整理、文件问答、TODO、档案和长期记忆连接成一个可追溯、可回滚的个人知识工作台。

当前产品优先服务学生和个人知识工作者，第一目标用户是项目创建者本人；未来会逐步开源给更广泛的个人知识管理用户使用。

## 当前状态

项目当前处于 v0.6 第一层能力打磨阶段。Tauri 2 + React + Vite + Rust 桌面骨架已经可运行，当前实现包含：

- 本地 Vault 选择与初始化。
- `000-收集箱/` 与 `000-收集箱/收集箱-已整理.md`。
- `.thebrain/index.sqlite` 内部索引、日志和状态数据库。
- Markdown 读写、预览、导出和 YAML frontmatter 过滤。
- 收集箱导入、抽取、整理计划、移动、ledger、movement log、audit events 和回滚。
- 收集箱队列的第一层可信 worker：手动单轮消费 pending 队列、暂停/恢复、稳定等待、预算检查、MiMo 抽取与整理决策、非 mock 且可信时自动移动、失败/冲突写入内部状态。
- MiMo provider 路径、预算状态、用量占位账本和 fallback/pending 状态。
- md/txt 本地 RAG 索引、关键词检索、local semantic placeholder、引用和 Trace。
- 前端工作台：主仪表盘、收集箱、Markdown 编辑器、便利贴、个人页、项目页和设置页；其中个人页和项目页允许先作为前端概念页保留，部分统计、Agent 历史和学习曲线仍是占位数据。

v0.4 不是完整个人 Agent、真实向量数据库系统或完整多格式解析系统。当前 RAG 使用关键词通道和 local semantic placeholder 通道，不声明已经具备真实 embedding 或重排能力。PDF、DOCX、PPTX、URL 的真实解析仍是后续目标。

当前能力状态约定：

- 真实可用：已有前端入口、Tauri command、本地服务、Vault 或 SQLite 数据支撑。
- fallback：后端命令失败或运行环境不完整时的安全降级结果，必须显示原因，不代表能力已完成。
- 占位：按设计图或产品愿景保留的 UI 与样例数据，暂不接正式后端。
- 后续目标：已进入 PRD/Roadmap，但当前阶段不要求实现。

占位能力真实接入后，应删除对应占位数据、占位注释和 README 中的占位描述。

## 产品定位

TheBrain 的长期方向是：

- Obsidian 增强版：以本地 Markdown Vault 作为用户数据源。
- NotebookLM 增强版：对整理后的本地文件进行带引用的问答、总结、对比和行动建议。
- 滴答清单补充：从笔记、会议纪要、通话记录和收集箱内容中整理 TODO、日程和提醒。
- 个人第二大脑：管理学习、生活、项目、人际关系和长期档案。

第一核心场景是类 Obsidian 的收集箱与 AI 自动分类整理。快速便利贴用于低摩擦捕获灵感；文件问答、TODO、个人档案和项目工作台围绕整理后的 Vault 逐步展开。

## AI 自动整理边界

AI 可以在用户指定的 Vault 范围内自动整理并移动 `000-收集箱/` 中的文件，不要求每次移动都由用户手动确认。

自动整理必须遵守以下边界：

- 只能处理 `000-收集箱/` 下允许处理的文件。
- 不能移动 `000-收集箱/收集箱-已整理.md`。
- 不能越过 Vault 根目录，所有路径必须规范化并拒绝绝对路径、空路径和 `..` 逃逸。
- 不能覆盖已有文件；冲突必须进入可见 conflict 状态或采用明确的冲突策略。
- 每次移动都必须写入 ledger、movement log 和 audit events。
- 必须支持回滚，把文件移回收集箱并更新 ledger。
- embedding、AI prompt/response、token usage、移动历史、队列日志不写入 Markdown frontmatter，只进入 `.thebrain` 内部结构。
- TODO、日程、联系人档案等行动项可以由 AI 生成候选；是否自动写入正式系统需要在后续 PRD 中按场景定义。

当前代码已经具备移动、日志、回滚和手动触发的单并发队列消费能力；长期稳定文件监听、常驻后台 worker、批量处理策略和更完整的自动冲突处理仍是后续目标。

## 数据边界

Vault 是用户资产的唯一可信来源；`.thebrain` 是索引、日志、Trace、队列和状态存储，不应污染用户 Markdown。

TheBrain 默认采用 Markdown/YAML-first：用户可读、可迁移、可长期保留的事实优先写入 Markdown、YAML frontmatter、目录结构、wikilink 和可读 ledger；SQLite 只承担索引、embedding、队列、预算、移动日志、审计、Trace、缓存和查询加速等 Markdown 不适合承载的内部状态。

```text
Vault/
  000-收集箱/
    收集箱-已整理.md
  .thebrain/
    index.sqlite
  .secrets/
    mimo_api_key.txt   # 可选，gitignored，不提交
```

`.thebrain/index.sqlite` 当前包含：`vault_meta`、`file_index`、`ai_usage`、`audit_events`、`listener_state`、`queue_items`、`dedupe_records`、`budget_settings`、`budget_ledger`、`movement_log`、`action_candidates`、`sticky_notes`、`rag_documents`、`rag_chunks`、`rag_index_runs`、`rag_queries`、`rag_trace_runs`、`rag_trace_nodes`、`rag_conversations`、`rag_messages`。

## 凭据与安全

真实 API Key 只允许存在于以下位置：

- 本机环境变量 `MIMO_API_KEY`
- gitignored 的 `Vault/.secrets/mimo_api_key.txt`
- 开发环境当前工作目录下 gitignored 的 `.secrets/mimo_api_key.txt`
- 用户显式传入的运行时参数

真实 key 不进入 README、代码、测试、日志、SQLite seed、前端错误信息或提交内容。仓库中的 `.env.local.example` 只能使用占位符。

## 支持文件类型

当前真实处理类型：

- 文本：`md`、`markdown`、`txt`
- 音频：`mp3`、`wav`、`m4a`、`aac`
- 图片：`png`、`jpg`、`jpeg`

当前 RAG 索引只扫描 Vault 内的 `md`、`markdown`、`txt`，并跳过 `.thebrain/`、`.secrets/`、`.git/`、`node_modules/`、`target/`、`dist/` 和 `000-收集箱/收集箱-已整理.md`。

PDF、DOCX、PPTX、URL 导入和复杂多格式解析可以在 UI 中作为未来能力展示，但必须明确标注为实验、占位或后续能力，避免让用户误解为已经完整可用。

## 当前架构

TheBrain 不是传统远程前后端分离 Web 应用，而是 Tauri 桌面应用。它分为前端 UI 层和本地后端服务层，最终打包在同一个桌面应用里。

前端层：

- `src/App.tsx`：桌面工作台 shell，包含 Vault、收集箱、Markdown、AI 整理、RAG 问答、TODO/日程、便利贴、个人页、项目页和设置页。
- `src/api.ts`：封装 Tauri commands，暴露受控导入、MiMo 状态、抽取、整理计划、worker、移动、回滚、冲突和 RAG 命令。
- `src/styles.css`：视觉 token、左侧导航、页面网格、收集箱、RAG 面板、Markdown 三栏编辑器和便利贴布局。

本地后端服务层：

- `src-tauri/src/commands.rs`：暴露 Vault、Markdown、收集箱、ledger、导入、MiMo、队列、worker、预算、移动/回滚、候选、便利贴、快捷键、冲突和 RAG commands。
- `src-tauri/src/services/vault.rs`：Vault 初始化和路径规范化。
- `src-tauri/src/services/importer.rs`：受控复制/移动外部文件到收集箱。
- `src-tauri/src/services/ai.rs`：MiMo 状态、文本本地抽取、音频/图片 MiMo 抽取、结构化整理 JSON 解析和 fallback。
- `src-tauri/src/services/worker.rs`：收集箱队列的第一层可信消费器，负责 claim pending item、稳定等待、预算检查、调用 MiMo、可信移动、失败/冲突/audit 记录和手动 drain。
- `src-tauri/src/services/movement.rs`：收集箱文件移动、ledger、movement log、audit events 和回滚。
- `src-tauri/src/services/rag.rs`、`retrieval.rs`、`chunking.rs`、`rag_trace.rs`：本地 RAG 索引、检索、分块、引用和 Trace。
- `src-tauri/src/services/index.rs`：`.thebrain/index.sqlite` schema。

MiMo 是 AI provider，负责抽取、理解、整理建议和回答生成；后台 worker 是本地调度器，负责从队列取任务、稳定等待、预算检查、调用 MiMo、执行移动、写日志、重试和回滚。当前已接入第一层手动 drain worker：它只处理 `000-收集箱/` 中的普通文件，不处理 ledger、`.thebrain` 或 `.secrets`；只有抽取和整理决策均为 `ok` 且 `is_mock=false` 时才会移动文件。长期常驻 worker、完整去重策略和冲突问答仍是后续目标。

## 使用方式

1. 运行 `npm install` 安装前端依赖。
2. 运行 `npm run tauri dev` 启动桌面应用，或运行 `npm run build` 做前端构建验证。
3. 在应用中选择 Vault 并初始化。
4. 可选：设置 `MIMO_API_KEY`，或把 key 放入 `Vault/.secrets/mimo_api_key.txt` / 项目根目录 `.secrets/mimo_api_key.txt`。
5. 在收集箱中导入文件或用便利贴快速捕获内容。
6. 让 AI 对收集箱文件进行抽取、分类、移动、记录和回滚。
7. 重建 RAG 索引后，对本地 Vault 提问，并查看引用和 Trace。

## 已完成目标

- 初始化并维护可运行的 Tauri 2 + React + Vite 应用骨架。
- Vault 选择、结构幂等初始化和 `.thebrain/index.sqlite` 创建。
- Markdown 编辑、预览、保存、导出和 YAML frontmatter 过滤。
- `000-收集箱` 文件展示与 ledger 相对 wikilink 历史映射。
- 受控导入命令 `import_to_inbox`，支持 copy/move、冲突不覆盖、audit/queue 记录。
- MiMo 状态命令 `get_mimo_status`，不泄露 key。
- 抽取命令 `extract_with_mimo`，支持文本本地抽取和音频/图片 MiMo 调用路径。
- 单文件计划命令 `plan_inbox_item`，返回 extraction、structured plan、候选和预算状态。
- 第一层 worker 命令 `get_worker_status`、`run_inbox_worker`、`pause_inbox_worker`、`resume_inbox_worker`，支持手动消费队列、暂停恢复、失败/冲突状态展示。
- 自动/手动移动基础命令，更新 ledger、movement log、audit events。
- 回滚命令 `rollback_move`，不覆盖已有文件。
- TODO/日程候选创建、确认、忽略。
- 预算暂停/耗尽状态和 `ai_usage` 用量占位记录。
- 冲突事件列表与解决记录命令。
- 设置页展示 MiMo key 状态，只显示 key 来源和脱敏状态。
- 全局快捷键插件依赖和注册命令已接入。
- RAG 文档、分块、索引运行、查询、Trace、会话和消息基础表。
- RAG 索引、状态、问答、引用和 Trace 基础命令。

## 未完成目标

- 长期常驻 worker、长期文件监听稳定性、批量处理策略和更完整的自动整理策略。
- 收集箱冲突问答窗口：遇到分类、命名或目标路径冲突时询问用户，并把用户回答保存为可复用整理规则。
- 真实 MiMo API 的长时间、多文件类型、带成本实机联调。
- MiMo 返回 token/cost 的真实精确统计。
- 完整冲突解决 UI，例如自动重命名建议、差异预览和批量处理。
- 真实 embedding、向量检索、重排模型、跨会话长期记忆策略和复杂个人统计。
- RAG 多轮会话 UI 与历史列表。
- PDF、DOCX、PPTX、URL 的真实解析。
- 人名档案、联系人信息、会议纪要、通话记录和个人关系管理数据模型。
- TODO/日程系统从候选到正式行动系统的完整工作流。
- 便利贴独立多窗口池、窗口回收和更完整的托盘体验。
- 个人页和项目工作台中的统计、Agent 历史、学习曲线从占位数据升级为真实数据。
- `src/App.tsx` 页面与状态继续拆分，把个人页、项目页、RAG、收集箱、便利贴等不相关模块拆成更清晰的组件和服务边界。
- 端到端 GUI 自动化测试和真实 Windows 实机长时间验证。

## 文档入口

- [PRD](docs/PRD.md)：产品定位、用户、核心场景、范围和非目标。
- [交付物关系](docs/DELIVERABLES.md)：README、AGENTS、PRD、架构、路线图、调研文档的职责分工。
- [架构](docs/ARCHITECTURE.md)：Vault、`.thebrain`、AI 整理、RAG 和 Tauri 分层。
- [路线图](docs/ROADMAP.md)：v0.5 及后续阶段目标。
- [同类项目调研](docs/research/comparable-projects.md)：Obsidian、Logseq、AFFiNE、Anytype、Joplin、Khoj、AnythingLLM 等项目参考。

## Test Plan

当前验证门槛：

- `npm install`
- `npm run build`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`

当前 Rust 单元测试覆盖 Vault 初始化、路径安全、Markdown frontmatter、ledger、SQLite schema、队列 claim/retry、预算、audit、导入、MiMo fallback、worker 暂停与失败不移动、移动/回滚、TODO/日程候选、便利贴和 RAG 基础能力。
