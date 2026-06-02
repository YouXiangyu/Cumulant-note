# TheBrain

TheBrain 是一个本地优先的外置大脑桌面应用。当前目标是提供类似 Obsidian 的本地 Vault 体验，并在 `000-收集箱` 上形成“导入 -> 抽取 -> 整理计划 -> 用户确认移动 -> 记录/回滚”的可验证闭环。

## 当前状态

项目当前处于 v0.4 本地优先 RAG / 长期记忆问答 MVP 阶段。Tauri 2 + React + Vite TypeScript 桌面骨架已经可运行，后端具备 Vault 初始化、Markdown 读写、收集箱展示、SQLite 内部索引、受控导入、MiMo 抽取路径、结构化整理计划、TODO/日程候选、用户确认移动、ledger 记录、movement log、audit events、回滚，以及本地 md/txt RAG 索引、检索、引用和 Trace。

v0.4 不是完整个人 Agent 或真实向量数据库系统。真实 AI 只接入 MiMo provider；无 key、预算暂停/耗尽、网络失败、API 失败、结构化解析失败或 RAG 上下文不足时返回明确 fallback/pending 状态，不伪造真实成功。RAG 当前使用本地关键词通道和 local semantic placeholder 通道，不声明已具备真实 embedding。

前端视觉与交互以 `docs/frontend-concepts/` 下概念图为基准，当前包含主仪表盘、收集箱、Markdown 编辑器、便利贴、个人页、项目工作台、拖拽导入层和设置页。

## 凭据与安全

真实 API Key 只允许存在于以下位置：

- 本机环境变量 `MIMO_API_KEY`
- gitignored 的 `Vault/.secrets/mimo_api_key.txt`
- 开发环境当前工作目录下 gitignored 的 `.secrets/mimo_api_key.txt`
- 用户显式传入的运行时参数

真实 key 不进入 README、代码、测试、日志、SQLite seed、前端错误信息或提交内容。仓库中的 `.env.local.example` 只能使用占位符。

## 支持文件类型

v0.4 当前真实处理类型：

- 文本：`md`、`markdown`、`txt`
- 音频：`mp3`、`wav`、`m4a`、`aac`
- 图片：`png`、`jpg`、`jpeg`

RAG 索引当前只扫描 Vault 内的 `md`、`markdown`、`txt`，并跳过 `.thebrain/`、`.secrets/`、`.git/`、`node_modules/`、`target/`、`dist/` 和 `000-收集箱/收集箱-已整理.md`。

PDF、DOCX、PPTX、URL 导入和复杂多格式解析仍是后续目标，当前只在 UI 中作为后续能力提示。

## 当前架构

前端层：

- `src/App.tsx`：桌面工作台 shell，包含 Vault、收集箱、Markdown、AI 整理、RAG 问答、TODO/日程、便利贴、个人页、项目页和设置页。
- `src/api.ts`：封装 Tauri commands，提供 fallback 状态，暴露受控导入、MiMo 状态、抽取、整理计划、移动、回滚、冲突和 RAG 命令。
- `src/styles.css`：当前视觉 token、左侧导航、页面网格、收集箱操作状态、RAG 面板、Markdown 三栏编辑器和便利贴布局。

Tauri/Rust 层：

- `src-tauri/src/lib.rs`：注册 Tauri 插件与 commands。
- `src-tauri/src/commands.rs`：暴露 Vault、Markdown、收集箱、ledger、导入、MiMo、队列、预算、移动/回滚、候选、便利贴、快捷键、冲突和 RAG commands。
- `src-tauri/src/services/vault.rs`：Vault 结构初始化和路径规范化。
- `src-tauri/src/services/importer.rs`：受控复制/移动外部文件到 `000-收集箱`，不覆盖已有文件，写入 audit/queue。
- `src-tauri/src/services/ai.rs`：MiMo 状态、md/txt 本地抽取、音频/图片 MiMo 抽取、结构化整理 JSON 解析和 fallback。
- `src-tauri/src/services/chunking.rs`：Markdown 结构感知分块，保留标题路径、行号、摘要和 token 估算。
- `src-tauri/src/services/retrieval.rs`：多通道检索骨架，包含关键词通道、local semantic placeholder 通道、去重、归一化和 top-k。
- `src-tauri/src/services/rag.rs`：RAG 索引重建、增量跳过、上下文格式化、问答编排、引用生成和查询记录。
- `src-tauri/src/services/rag_trace.rs`：RAG Trace run/node 持久化与最近 Trace 查询。
- `src-tauri/src/services/movement.rs`：只从收集箱移动文件，更新 ledger、movement log、audit events，并支持回滚。
- `src-tauri/src/services/candidates.rs`：TODO/日程候选，AI 只生成候选，用户确认后变更状态。
- `src-tauri/src/services/budget.rs` 和 `usage.rs`：预算状态、暂停/耗尽、用量占位账本和 `ai_usage` 汇总。
- `src-tauri/src/services/audit.rs`：SQLite audit events 读写。
- `src-tauri/src/services/index.rs`：`.thebrain/index.sqlite` schema。

Vault 结构：

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

## 使用方式

1. 运行 `npm install` 安装前端依赖。
2. 运行 `npm run tauri dev` 启动桌面应用，或运行 `npm run build` 做前端构建验证。
3. 在应用中选择 Vault 并初始化。
4. 可选：设置 `MIMO_API_KEY`，或把 key 放入 `Vault/.secrets/mimo_api_key.txt` / 项目根目录 `.secrets/mimo_api_key.txt`。
5. 在收集箱页使用“导入文件”，选择复制或显式移动；导入结果会写入 audit 和 queue。
6. 选择一个收集箱文件，点击“生成计划”。md/txt 会本地读取内容；音频/图片在有 key 且预算允许时调用 MiMo。
7. 检查目标路径、置信度、reason 和候选 TODO/日程；点击“运行整理”后才会移动文件。
8. 移动成功后会更新 `收集箱-已整理.md`、`movement_log` 和 `audit_events`；可用回滚恢复，不覆盖已有文件。
9. 在主仪表盘点击“重建索引”创建或更新 RAG 索引，然后在对话框中提问；回答会展示本地引用和 Trace。

## 行为边界

- AI 自动整理只能处理 `000-收集箱/` 下的文件。
- `000-收集箱/收集箱-已整理.md` 不能被 AI 移动。
- 所有 Vault 相对路径都会拒绝绝对路径、空路径和 `..` 逃逸。
- 导入、移动、回滚都不会覆盖已有文件；冲突进入可见 conflict 状态。
- `收集箱-已整理.md` 使用相对 wikilink，例如 `[[../100-School/笔记.md]]`。
- embedding、AI prompt/response、token usage、移动历史、队列日志不写入 Markdown frontmatter，只进入 `.thebrain` 内部结构。
- MiMo 整理输出必须解析为受控 JSON；解析失败只返回 fallback/pending，不会用自然语言驱动移动。
- RAG 不索引 `.thebrain` 内部数据或 `.secrets` 凭据目录；RAG 查询记录、引用和 Trace 只进入 `.thebrain/index.sqlite`。
- 当前 RAG local semantic placeholder 是本地词元重叠和标题加权，不是真实 embedding。
- 默认不会后台自动移动文件，必须由用户点击执行。

## 已完成目标

- 初始化并维护可运行的 Tauri 2 + React + Vite 应用骨架。
- Vault 选择、结构幂等初始化和 `.thebrain/index.sqlite` 创建。
- Markdown 编辑、预览、保存、导出和 YAML frontmatter 过滤。
- `000-收集箱` 文件展示与 ledger 相对 wikilink 历史映射。
- 受控导入命令 `import_to_inbox`，支持 copy/move、冲突不覆盖、audit/queue 记录。
- MiMo 状态命令 `get_mimo_status`，不泄露 key。
- 抽取命令 `extract_with_mimo`，支持文本本地抽取和音频/图片 MiMo 调用路径。
- 单文件计划命令 `plan_inbox_item`，返回 extraction、structured plan、候选和预算状态。
- 结构化整理解析：`sourceRelativePath`、`targetRelativePath`、`confidence`、`reason`、`tags`、`summary`、`todoCandidates`、`scheduleCandidates`。
- 用户确认移动命令 `run_ai_organize` / `move_inbox_item`，更新 ledger、movement log、audit events。
- 回滚命令 `rollback_move`，不覆盖已有文件。
- TODO/日程候选创建、确认、忽略。
- 预算暂停/耗尽状态和 `ai_usage` 用量占位记录。
- 冲突事件列表与解决记录命令。
- 前端收集箱真实导入、抽取状态、计划状态、fallback 标记、移动和回滚入口。
- 设置页展示 MiMo key 状态，只显示 key 来源和脱敏状态。
- 全局快捷键插件依赖和注册命令已接入。
- `.thebrain/index.sqlite` schema v3，新增 RAG 文档、分块、索引运行、查询、Trace、会话和消息基础表。
- RAG 索引命令 `rebuild_rag_index`，支持扫描 Vault 内 md/txt、跳过内部目录与 ledger、按内容 hash 和 mtime 增量跳过未变更文件。
- RAG 状态命令 `get_rag_index_status`，返回 schema、文档数、chunk 数和最近索引运行。
- RAG 问答命令 `ask_rag`，包含关键词通道、local semantic placeholder 通道、去重/归一化/top-k、上下文格式化、MiMo 回答入口、fallback 和引用。
- RAG Trace 命令 `get_latest_rag_trace`，可查看 query、retrieval、postprocess、context、llm 节点。
- 前端主仪表盘新增 RAG 索引状态、重建入口、回答展示、引用卡片和 Trace 列表。

## 未完成目标

- 真实 MiMo API 的长时间、多文件类型、带成本实机联调。
- MiMo 返回 token/cost 的真实精确统计。
- 后台自动队列消费、文件监听长期稳定性和批量处理策略。
- 完整冲突解决 UI，例如自动重命名建议、差异预览和批量处理。
- 真实 AI 自动移动决策；当前仍要求用户点击确认移动。
- 真实 embedding、向量检索、重排模型、跨会话长期记忆策略和复杂个人统计。
- RAG 多轮会话 UI 与历史列表；当前仅保留 `rag_conversations` / `rag_messages` 基础表。
- PDF、DOCX、PPTX、URL 的真实解析。
- 便利贴独立多窗口池、窗口回收和更完整的托盘体验。
- 个人页和项目工作台中的统计、Agent 历史、学习曲线仍有前端派生或占位数据。
- 端到端 GUI 自动化测试和真实 Windows 实机长时间验证。

## Test Plan

当前验证门槛：

- `npm install`
- `npm run build`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`

当前 Rust 单元测试覆盖：

- Vault 初始化幂等，不覆盖已有 ledger。
- 路径拒绝绝对路径、`..` 和 Vault 逃逸。
- Markdown frontmatter 保存、预览隐藏和导出过滤。
- ledger 相对 wikilink 解析。
- SQLite schema 创建。
- 队列监听状态、持久化队列和 dedupe key 去重。
- 预算 limits、暂停和耗尽状态。
- audit events 持久化。
- 受控导入 copy、冲突不覆盖和不支持类型返回。
- md/txt 本地抽取、音频无 key fallback。
- MiMo 整理 JSON 解析和越界 target 拒绝。
- AI 移动只允许收集箱内文件，拒绝 ledger 和非收集箱文件。
- 移动和回滚拒绝覆盖已有文件。
- TODO/日程候选确认/拒绝。
- 便利贴持久化和自动保存到收集箱 Markdown。
- RAG schema 创建、Markdown 分块、md/txt 索引、跳过 ledger、关键词/local semantic placeholder 检索、无网络 mock 问答引用和 Trace。

## 协作规则与要点

- 每次开发前先阅读当前代码与文档。
- 不在需求不明确时直接实现。
- 将不确定的决策点交给用户确认。
- README 只维护当前项目事实，不作为流水账式变更日志。
- 当架构、目标、需求或当前状态变化时，同步更新 README。
