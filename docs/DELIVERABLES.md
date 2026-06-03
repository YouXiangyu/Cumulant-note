# 交付物关系

TheBrain 需要把不同交付物的职责分清，避免 README、PRD、架构文档、Roadmap 和 AGENTS 互相重复或互相冲突。

## 1. 交付物地图

```text
README.md
  当前项目事实入口

AGENTS.md
  AI/agent 协作协议

docs/PRD.md
  产品目标、用户、核心场景、非目标

docs/ARCHITECTURE.md
  系统结构、数据边界、服务职责

docs/ROADMAP.md
  阶段计划、优先级、未完成目标

docs/research/
  同类项目、竞品和决策参考

docs/frontend-concepts/
  视觉概念图和 UI 方向参考
```

## 2. README.md

README 只描述当前项目事实：

- TheBrain 是什么。
- 当前版本状态。
- 当前真实能力。
- 当前产品边界。
- 当前已完成和未完成目标。
- 如何运行和验证。
- 文档入口。

README 不负责：

- 记录每次修改。
- 展开完整 PRD。
- 存放长期竞品调研。
- 详细描述每个表结构。

## 3. AGENTS.md

AGENTS 是 agent 的协作协议：

- 项目经理身份。
- 每轮任务如何确认。
- 什么时候需要问用户。
- 什么时候可以自行决定实现细节。
- README 和文档更新规则。
- subagent 使用规则。
- 安全和本地数据红线。

AGENTS 不应成为产品长文。

## 4. PRD

PRD 描述产品目标和产品边界：

- 用户是谁。
- 解决什么问题。
- 核心场景是什么。
- 哪些是当前范围。
- 哪些是非目标。
- AI 自动化边界。
- 前端愿景与后端真实能力如何分层。

## 5. Architecture

架构文档描述系统事实：

- Tauri 桌面应用分层。
- Vault 与 `.thebrain` 的职责。
- 收集箱自动整理 pipeline。
- RAG pipeline。
- Trace、audit、movement log。
- 凭据和安全。

## 6. Roadmap

Roadmap 描述阶段目标：

- v0.5 先收敛产品与文档。
- v0.6 打磨收集箱自动整理。
- v0.7 强化 RAG 和 NotebookLM 式问答。
- v0.8 扩展 TODO、日程、人名档案和个人关系管理。
- 后续再进入多格式、自动化和开源生态。

## 7. Research

Research 保存同类项目参考，不放入 README 正文。

适合保存：

- Obsidian、Logseq、Joplin、AFFiNE、Anytype、Dendron、Foam。
- Khoj、Open WebUI、AnythingLLM、DocsGPT、LlamaIndex。
- 它们的 README、docs、roadmap、数据模型、local-first 边界和 RAG 组织方式。

## 8. Frontend Concepts

`docs/frontend-concepts/` 是视觉方向参考，不是不可变硬规范。

当前概念图可作为：

- 软件风格参考。
- 前端功能入口参考。
- 设计审查素材。
- Image To Code 的候选目标图。

随着业务功能确认，概念图可以继续更新。
