import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  AlertTriangle,
  ArchiveRestore,
  Bot,
  CalendarClock,
  Check,
  CircleDollarSign,
  ClipboardCheck,
  Download,
  FileText,
  FolderOpen,
  HardDrive,
  Inbox,
  LayoutDashboard,
  ListChecks,
  MoveRight,
  PauseCircle,
  PlayCircle,
  Plus,
  RefreshCw,
  RotateCcw,
  Save,
  Settings,
  ShieldAlert,
  StickyNote,
  Trash2,
  Wallet,
} from "lucide-react";
import DOMPurify from "dompurify";
import { marked } from "marked";
import {
  AppSettings,
  BudgetStatus,
  commands,
  ConflictItem,
  ConflictResolutionAction,
  defaultAppSettings,
  InboxItem,
  LedgerItem,
  MarkdownDocument,
  QueueStatus,
  selectVault,
  StickyNote as StickyNoteRecord,
  TodoScheduleCandidate,
  UsageSummary,
  VaultInitResult,
  VaultTreeNode,
} from "./api";

type ViewId = "dashboard" | "markdown" | "organize" | "todo" | "notes" | "settings";

interface AppError {
  title: string;
  detail: string;
  recovery: string;
}

const defaultRelativePath = "000-收集箱/新笔记.md";
const settingsStorageKey = "thebrain.settings";
const notesStorageKey = "thebrain.stickyNotes";

const navItems: { id: ViewId; label: string; icon: ReactNode }[] = [
  { id: "dashboard", label: "总览", icon: <LayoutDashboard size={16} aria-hidden="true" /> },
  { id: "markdown", label: "文档", icon: <FileText size={16} aria-hidden="true" /> },
  { id: "organize", label: "AI 整理", icon: <Bot size={16} aria-hidden="true" /> },
  { id: "todo", label: "TODO / 日程", icon: <ListChecks size={16} aria-hidden="true" /> },
  { id: "notes", label: "便利贴", icon: <StickyNote size={16} aria-hidden="true" /> },
  { id: "settings", label: "设置", icon: <Settings size={16} aria-hidden="true" /> },
];

function safeJson(value: unknown): string {
  return JSON.stringify(value ?? {}, null, 2);
}

function readStorage<T>(key: string, fallback: T): T {
  try {
    const value = localStorage.getItem(key);
    return value ? (JSON.parse(value) as T) : fallback;
  } catch {
    return fallback;
  }
}

function flattenTree(nodes: VaultTreeNode[]): VaultTreeNode[] {
  return nodes.flatMap((node) => [node, ...flattenTree(node.children)]);
}

function formatDate(value?: string | number): string {
  if (!value) return "未记录";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "未记录";
  return date.toLocaleString("zh-CN", { hour12: false });
}

function formatBudget(cents: number): string {
  return `${(cents / 100).toFixed(2)} 元`;
}

function percentage(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function hasFallback(value: unknown): boolean {
  if (Array.isArray(value)) {
    return value.some((item) => hasFallback(item));
  }
  return Boolean(
    value &&
      typeof value === "object" &&
      "isFallback" in value &&
      (value as { isFallback?: boolean }).isFallback,
  );
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return JSON.stringify(error);
}

function createStickyNote(path: string): StickyNoteRecord {
  const now = new Date().toISOString();
  return {
    id: `note-${Date.now()}`,
    title: "临时记录",
    content: "",
    autosavePath: path,
    pinned: true,
    updatedAt: now,
  };
}

function actionLabel(action: ConflictResolutionAction): string {
  const labels: Record<ConflictResolutionAction, string> = {
    keep_existing: "保留现有",
    overwrite: "覆盖",
    rename: "重命名",
    skip: "跳过",
  };
  return labels[action];
}

function queueStateLabel(state?: QueueStatus["state"]): string {
  const labels: Record<QueueStatus["state"], string> = {
    idle: "空闲",
    running: "运行中",
    paused: "已暂停",
    cooldown: "冷却中",
    error: "异常",
  };
  return state ? labels[state] : "未连接";
}

function viewTitle(view: ViewId): string {
  const item = navItems.find((entry) => entry.id === view);
  return item?.label ?? "总览";
}

export default function App() {
  const [activeView, setActiveView] = useState<ViewId>("dashboard");
  const [vaultPath, setVaultPath] = useState(() => localStorage.getItem("thebrain.vault") ?? "");
  const [relativePath, setRelativePath] = useState(defaultRelativePath);
  const [content, setContent] = useState("# 新笔记\n");
  const [frontmatterText, setFrontmatterText] = useState(
    '{\n  "title": "新笔记",\n  "status": "draft"\n}',
  );
  const [document, setDocument] = useState<MarkdownDocument | null>(null);
  const [initResult, setInitResult] = useState<VaultInitResult | null>(null);
  const [tree, setTree] = useState<VaultTreeNode[]>([]);
  const [inbox, setInbox] = useState<InboxItem[]>([]);
  const [ledger, setLedger] = useState<LedgerItem[]>([]);
  const [usage, setUsage] = useState<UsageSummary | null>(null);
  const [exported, setExported] = useState("");
  const [status, setStatus] = useState("等待选择 Vault");
  const [appError, setAppError] = useState<AppError | null>(null);
  const [settings, setSettings] = useState<AppSettings>(() =>
    readStorage<AppSettings>(settingsStorageKey, defaultAppSettings),
  );
  const [queueStatus, setQueueStatus] = useState<QueueStatus | null>(null);
  const [budgetStatus, setBudgetStatus] = useState<BudgetStatus | null>(null);
  const [conflicts, setConflicts] = useState<ConflictItem[]>([]);
  const [todoCandidates, setTodoCandidates] = useState<TodoScheduleCandidate[]>([]);
  const [organizePlan, setOrganizePlan] = useState<Awaited<
    ReturnType<typeof commands.planAiOrganize>
  > | null>(null);
  const [organizeResult, setOrganizeResult] = useState<Awaited<
    ReturnType<typeof commands.runAiOrganize>
  > | null>(null);
  const [selectedInboxPath, setSelectedInboxPath] = useState("");
  const [targetMovePath, setTargetMovePath] = useState("");
  const [overwriteMove, setOverwriteMove] = useState(false);
  const [rollbackAuditId, setRollbackAuditId] = useState("");
  const [stickyNotes, setStickyNotes] = useState<StickyNoteRecord[]>(() =>
    readStorage<StickyNoteRecord[]>(notesStorageKey, [
      createStickyNote(defaultAppSettings.stickyNotesPath),
    ]),
  );
  const [activeNoteId, setActiveNoteId] = useState("");
  const [dirtyNoteId, setDirtyNoteId] = useState<string | null>(null);

  const previewHtml = useMemo(() => {
    const markdown = document?.previewMarkdown ?? content;
    const unsafeHtml = marked.parse(markdown, { async: false }) as string;
    return DOMPurify.sanitize(unsafeHtml, { USE_PROFILES: { html: true } });
  }, [content, document]);

  const markdownFiles = useMemo(
    () =>
      flattenTree(tree).filter(
        (node) => !node.isDir && /\.(md|markdown)$/i.test(node.relativePath),
      ),
    [tree],
  );

  const activeNote = useMemo(
    () => stickyNotes.find((note) => note.id === activeNoteId) ?? stickyNotes[0] ?? null,
    [activeNoteId, stickyNotes],
  );

  const queueIsPaused = !queueStatus || queueStatus.state === "paused";
  const budgetRatio =
    budgetStatus && budgetStatus.monthlyLimitCents > 0
      ? Math.min(1, budgetStatus.spentCents / budgetStatus.monthlyLimitCents)
      : 0;

  useEffect(() => {
    localStorage.setItem(settingsStorageKey, JSON.stringify(settings));
  }, [settings]);

  useEffect(() => {
    localStorage.setItem(notesStorageKey, JSON.stringify(stickyNotes));
  }, [stickyNotes]);

  useEffect(() => {
    if (!activeNoteId && stickyNotes[0]) {
      setActiveNoteId(stickyNotes[0].id);
    }
  }, [activeNoteId, stickyNotes]);

  useEffect(() => {
    if (!dirtyNoteId) return;
    const dirtyNote = stickyNotes.find((note) => note.id === dirtyNoteId);
    if (!dirtyNote) return;
    const delay = Math.max(800, settings.autoSaveIntervalSeconds * 1000);
    const timeoutId = window.setTimeout(() => {
      void persistStickyNote(dirtyNote, true);
    }, delay);
    return () => window.clearTimeout(timeoutId);
  }, [dirtyNoteId, settings.autoSaveIntervalSeconds, stickyNotes, vaultPath]);

  useEffect(() => {
    if (vaultPath) {
      void refresh(vaultPath);
    } else {
      void refreshOperations("");
    }
  }, []);

  async function run<T>(
    action: () => Promise<T>,
    message: string,
    successMessage = "完成",
  ): Promise<T | null> {
    setAppError(null);
    setStatus(message);
    try {
      const result = await action();
      setStatus(hasFallback(result) ? `${successMessage}（前端占位）` : successMessage);
      return result;
    } catch (error) {
      setStatus("失败");
      setAppError({
        title: `${message}失败`,
        detail: errorMessage(error),
        recovery: "检查 Vault 路径、文件占用状态或后端命令注册；前端不会自动覆盖本地文件。",
      });
      return null;
    }
  }

  async function refreshOperations(nextVaultPath = vaultPath) {
    const [nextSettings, nextQueue, nextBudget, nextConflicts, nextTodoCandidates, nextNotes] =
      await Promise.all([
        commands.getAppSettings(nextVaultPath),
        commands.getQueueStatus(nextVaultPath),
        commands.getBudgetStatus(nextVaultPath),
        commands.listConflicts(nextVaultPath),
        commands.listTodoScheduleCandidates(nextVaultPath),
        commands.listStickyNotes(nextVaultPath),
      ]);

    setSettings((current) =>
      nextSettings.isFallback ? { ...current, isFallback: true } : { ...current, ...nextSettings },
    );
    setQueueStatus(nextQueue);
    setBudgetStatus(nextBudget);
    setConflicts(nextConflicts);
    setTodoCandidates(nextTodoCandidates);
    if (nextNotes.length > 0) {
      setStickyNotes(nextNotes);
    }
  }

  async function chooseVault() {
    const selected = await run(() => selectVault(), "选择 Vault", "Vault 已选择");
    if (selected) {
      setVaultPath(selected);
      localStorage.setItem("thebrain.vault", selected);
      await refresh(selected);
    }
  }

  async function refresh(nextVaultPath = vaultPath) {
    if (!nextVaultPath) {
      await refreshOperations("");
      return;
    }

    const result = await run(
      async () => {
        const [nextTree, nextInbox, nextLedger, nextUsage] = await Promise.all([
          commands.listVaultTree(nextVaultPath),
          commands.listInbox(nextVaultPath),
          commands.parseInboxLedger(nextVaultPath),
          commands.getAiUsage(nextVaultPath),
        ]);
        return { nextTree, nextInbox, nextLedger, nextUsage };
      },
      "刷新 Vault",
      "Vault 已刷新",
    );
    if (!result) return;
    setTree(result.nextTree);
    setInbox(result.nextInbox);
    setLedger(result.nextLedger);
    setUsage(result.nextUsage);
    await refreshOperations(nextVaultPath);
  }

  async function initVault() {
    if (!vaultPath) return;
    const result = await run(() => commands.initVault(vaultPath), "初始化 Vault", "Vault 已初始化");
    if (!result) return;
    setInitResult(result);
    await refresh(result.vaultPath);
  }

  async function openMarkdown(path: string) {
    if (!vaultPath || !path) return;
    const result = await run(() => commands.readMarkdown(vaultPath, path), "读取 Markdown", "已打开");
    if (!result) return;
    setDocument(result);
    setRelativePath(result.relativePath);
    setContent(result.content);
    setFrontmatterText(safeJson(result.frontmatter));
    setExported("");
    setActiveView("markdown");
  }

  async function saveMarkdown() {
    if (!vaultPath || !relativePath) return;
    let frontmatter: unknown;
    try {
      frontmatter = JSON.parse(frontmatterText || "{}");
    } catch {
      setAppError({
        title: "YAML 属性无效",
        detail: "当前属性编辑器内容不是合法 JSON。",
        recovery: "修正 JSON 后再保存，Markdown 正文暂未提交。",
      });
      return;
    }
    const result = await run(
      () => commands.saveMarkdown(vaultPath, relativePath, content, frontmatter),
      "保存 Markdown",
      "Markdown 已保存",
    );
    if (!result) return;
    setDocument(result);
    setFrontmatterText(safeJson(result.frontmatter));
    await refresh();
  }

  async function exportMarkdown() {
    if (!vaultPath || !relativePath) return;
    const result = await run(
      () => commands.exportMarkdown(vaultPath, relativePath),
      "导出 Markdown",
      "Markdown 已导出",
    );
    if (!result) return;
    setExported(result.markdown);
  }

  async function saveSettings() {
    const result = await run(
      async () => {
        const saved = await commands.saveAppSettings(vaultPath, settings);
        if (settings.enableGlobalShortcut) {
          await commands.registerGlobalShortcut("Ctrl+Space");
        }
        await commands.prewarmStickyWindows(settings.prewarmWindows);
        return saved;
      },
      "保存设置",
      "设置已保存",
    );
    if (!result) return;
    setSettings((current) => ({ ...current, ...result }));
  }

  async function saveBudget() {
    const result = await run(
      () =>
        commands.saveBudgetSettings(vaultPath, {
          monthlyLimitCents: settings.budgetMonthlyCents,
          hardStopCents: settings.budgetHardStopCents,
        }),
      "保存预算",
      "预算已保存",
    );
    if (!result) return;
    setBudgetStatus(result);
  }

  async function toggleQueue() {
    const result = await run(
      () => (queueIsPaused ? commands.resumeQueue(vaultPath) : commands.pauseQueue(vaultPath)),
      queueIsPaused ? "恢复队列" : "暂停队列",
      queueIsPaused ? "队列已恢复" : "队列已暂停",
    );
    if (!result) return;
    setQueueStatus(result);
  }

  async function planOrganize() {
    const result = await run(
      () => commands.planAiOrganize(vaultPath),
      "生成整理计划",
      "整理计划已生成",
    );
    if (!result) return;
    setOrganizePlan(result);
    setActiveView("organize");
  }

  async function runOrganize() {
    const result = await run(
      () => commands.runAiOrganize(vaultPath, organizePlan?.id),
      "执行 AI 整理",
      "AI 整理已执行",
    );
    if (!result) return;
    setOrganizeResult(result);
    await refresh(vaultPath);
  }

  async function moveInboxItem() {
    if (!selectedInboxPath || !targetMovePath) {
      setAppError({
        title: "移动信息不完整",
        detail: "需要选择收集箱文件并填写目标相对路径。",
        recovery: "目标路径必须仍位于 Vault 内，后端会负责最终校验。",
      });
      return;
    }
    const result = await run(
      () => commands.moveInboxItem(vaultPath, selectedInboxPath, targetMovePath, overwriteMove),
      "移动收集箱文件",
      "移动请求已提交",
    );
    if (!result) return;
    setOrganizeResult({
      moved: result.moved ? 1 : 0,
      skipped: result.moved ? 0 : 1,
      conflicts: 0,
      auditId: result.auditId,
      message: result.message,
      isFallback: result.isFallback,
      fallbackReason: result.fallbackReason,
    });
    if (result.auditId) setRollbackAuditId(result.auditId);
    await refresh(vaultPath);
  }

  async function rollbackMove() {
    const result = await run(
      () => commands.rollbackMove(vaultPath, rollbackAuditId || undefined, selectedInboxPath || undefined),
      "回滚移动",
      "回滚请求已提交",
    );
    if (!result) return;
    setOrganizeResult({
      moved: 0,
      skipped: result.rolledBack ? 0 : 1,
      conflicts: 0,
      auditId: result.auditId,
      message: result.message,
      isFallback: result.isFallback,
      fallbackReason: result.fallbackReason,
    });
    await refresh(vaultPath);
  }

  async function confirmCandidate(candidate: TodoScheduleCandidate) {
    const result = await run(
      () => commands.confirmTodoScheduleCandidate(vaultPath, candidate.id, candidate),
      "确认候选",
      "候选已确认",
    );
    if (!result) return;
    setTodoCandidates((items) =>
      items.map((item) => (item.id === candidate.id ? { ...item, ...result } : item)),
    );
  }

  async function dismissCandidate(candidate: TodoScheduleCandidate) {
    const result = await run(
      () => commands.dismissTodoScheduleCandidate(vaultPath, candidate.id),
      "忽略候选",
      "候选已忽略",
    );
    if (!result) return;
    setTodoCandidates((items) =>
      items.map((item) => (item.id === candidate.id ? { ...item, status: "dismissed" } : item)),
    );
  }

  function addStickyNote() {
    const note = createStickyNote(settings.stickyNotesPath);
    setStickyNotes((notes) => [note, ...notes]);
    setActiveNoteId(note.id);
    setDirtyNoteId(note.id);
  }

  function updateActiveNote(patch: Partial<StickyNoteRecord>) {
    if (!activeNote) return;
    const updatedAt = new Date().toISOString();
    setStickyNotes((notes) =>
      notes.map((note) =>
        note.id === activeNote.id ? { ...note, ...patch, updatedAt } : note,
      ),
    );
    setDirtyNoteId(activeNote.id);
  }

  async function persistStickyNote(note: StickyNoteRecord, silent = false) {
    try {
      const result = vaultPath
        ? await commands.saveStickyNote(vaultPath, note)
        : {
            ...note,
            updatedAt: new Date().toISOString(),
            isFallback: true,
            fallbackReason: "未选择 Vault，已写入浏览器本地缓存。",
          };
      setStickyNotes((notes) => notes.map((item) => (item.id === note.id ? result : item)));
      setDirtyNoteId((current) => (current === note.id ? null : current));
      setStatus(`${silent ? "便利贴已自动保存" : "便利贴已保存"}：${result.autosavePath}`);
    } catch (error) {
      setAppError({
        title: "便利贴保存失败",
        detail: errorMessage(error),
        recovery: "内容已保留在当前界面和浏览器本地缓存，可稍后重试。",
      });
    }
  }

  async function deleteActiveNote() {
    if (!activeNote) return;
    const result = await run(
      () => commands.deleteStickyNote(vaultPath, activeNote.id),
      "删除便利贴",
      "便利贴已删除",
    );
    if (!result) return;
    setStickyNotes((notes) => notes.filter((note) => note.id !== activeNote.id));
    setDirtyNoteId((current) => (current === activeNote.id ? null : current));
  }

  async function resolveConflict(conflict: ConflictItem, action: ConflictResolutionAction) {
    const result = await run(
      () => commands.resolveConflict(vaultPath, conflict.id, action),
      "处理冲突",
      "冲突已处理",
    );
    if (!result) return;
    if (result.resolved) {
      setConflicts((items) => items.filter((item) => item.id !== conflict.id));
    }
  }

  function renderDashboard() {
    return (
      <div className="view-stack">
        <section className="metric-strip" aria-label="运行状态">
          <div className="metric-cell accent-green">
            <span>队列</span>
            <strong>{queueStateLabel(queueStatus?.state)}</strong>
            <small>{queueStatus?.lastEvent ?? "等待后端状态"}</small>
          </div>
          <div className="metric-cell accent-blue">
            <span>预算</span>
            <strong>{budgetStatus ? formatBudget(budgetStatus.remainingCents) : "未连接"}</strong>
            <small>剩余额度</small>
          </div>
          <div className="metric-cell accent-gold">
            <span>收集箱</span>
            <strong>{inbox.length}</strong>
            <small>直接子项</small>
          </div>
          <div className="metric-cell accent-red">
            <span>冲突</span>
            <strong>{conflicts.length}</strong>
            <small>待处理提示</small>
          </div>
        </section>

        <section className="section-panel">
          <div className="section-heading">
            <h2>运行控制</h2>
            <span className="status-chip">{status}</span>
          </div>
          <div className="control-grid">
            <button type="button" onClick={toggleQueue}>
              {queueIsPaused ? (
                <PlayCircle size={16} aria-hidden="true" />
              ) : (
                <PauseCircle size={16} aria-hidden="true" />
              )}
              {queueIsPaused ? "恢复队列" : "暂停队列"}
            </button>
            <button type="button" onClick={planOrganize} disabled={!vaultPath}>
              <Bot size={16} aria-hidden="true" />
              生成整理计划
            </button>
            <button type="button" onClick={runOrganize} disabled={!vaultPath}>
              <MoveRight size={16} aria-hidden="true" />
              执行整理
            </button>
            <button type="button" onClick={rollbackMove} disabled={!vaultPath}>
              <RotateCcw size={16} aria-hidden="true" />
              回滚
            </button>
          </div>
        </section>

        <section className="split-layout">
          <div className="section-panel">
            <div className="section-heading">
              <h2>最近收集</h2>
              <button type="button" className="ghost-button" onClick={() => setActiveView("markdown")}>
                打开文档
              </button>
            </div>
            <div className="dense-list">
              {inbox.slice(0, 6).map((item) => (
                <button
                  type="button"
                  key={item.relativePath}
                  onClick={() => !item.isDir && openMarkdown(item.relativePath)}
                  disabled={item.isDir}
                >
                  <FileText size={14} aria-hidden="true" />
                  <span>{item.name}</span>
                  <small>{item.isDir ? "目录" : formatDate(item.modifiedAt)}</small>
                </button>
              ))}
              {inbox.length === 0 ? <p className="empty-state">暂无收集箱条目</p> : null}
            </div>
          </div>

          <div className="section-panel">
            <div className="section-heading">
              <h2>候选确认</h2>
              <span className="status-chip">{todoCandidates.length} 项</span>
            </div>
            <div className="dense-list">
              {todoCandidates.slice(0, 5).map((candidate) => (
                <button
                  type="button"
                  key={candidate.id}
                  onClick={() => setActiveView("todo")}
                  className={candidate.status !== "pending" ? "muted-row" : undefined}
                >
                  <CalendarClock size={14} aria-hidden="true" />
                  <span>{candidate.title}</span>
                  <small>{candidate.kind === "todo" ? "TODO" : "日程"}</small>
                </button>
              ))}
              {todoCandidates.length === 0 ? <p className="empty-state">暂无候选</p> : null}
            </div>
          </div>
        </section>
      </div>
    );
  }

  function renderMarkdownWorkspace() {
    return (
      <div className="view-stack">
        <section className="section-panel">
          <div className="document-toolbar">
            <input
              aria-label="Markdown 相对路径"
              value={relativePath}
              onChange={(event) => setRelativePath(event.target.value)}
            />
            <button type="button" onClick={saveMarkdown} disabled={!vaultPath || !relativePath}>
              <Save size={16} aria-hidden="true" />
              保存
            </button>
            <button type="button" onClick={exportMarkdown} disabled={!vaultPath || !relativePath}>
              <Download size={16} aria-hidden="true" />
              导出
            </button>
          </div>
        </section>

        <section className="editor-grid">
          <div className="editor-pane">
            <div className="section-heading compact">
              <h2>编辑</h2>
            </div>
            <textarea
              className="content-editor"
              value={content}
              onChange={(event) => {
                setContent(event.target.value);
                setDocument(null);
              }}
            />
          </div>

          <div className="editor-pane">
            <div className="section-heading compact">
              <h2>预览</h2>
            </div>
            <article className="markdown-preview" dangerouslySetInnerHTML={{ __html: previewHtml }} />
          </div>
        </section>

        <section className="lower-grid">
          <div className="editor-pane">
            <div className="section-heading compact">
              <h2>YAML 属性</h2>
            </div>
            <textarea
              className="frontmatter-editor"
              value={frontmatterText}
              onChange={(event) => setFrontmatterText(event.target.value)}
            />
          </div>

          <div className="editor-pane">
            <div className="section-heading compact">
              <h2>导出</h2>
            </div>
            <textarea className="export-output" value={exported} readOnly />
          </div>
        </section>
      </div>
    );
  }

  function renderOrganize() {
    return (
      <div className="view-stack">
        <section className="section-panel">
          <div className="section-heading">
            <h2>AI 整理控制</h2>
            {organizePlan?.isFallback ? <span className="status-chip warning">前端占位</span> : null}
          </div>
          <div className="control-grid">
            <button type="button" onClick={planOrganize} disabled={!vaultPath}>
              <Bot size={16} aria-hidden="true" />
              生成计划
            </button>
            <button type="button" onClick={runOrganize} disabled={!vaultPath}>
              <MoveRight size={16} aria-hidden="true" />
              执行移动
            </button>
            <button type="button" onClick={rollbackMove} disabled={!vaultPath}>
              <ArchiveRestore size={16} aria-hidden="true" />
              回滚移动
            </button>
          </div>
          {organizeResult ? (
            <div className="result-line">
              <strong>{organizeResult.message}</strong>
              <span>移动 {organizeResult.moved}，跳过 {organizeResult.skipped}，冲突 {organizeResult.conflicts}</span>
            </div>
          ) : null}
        </section>

        <section className="section-panel">
          <div className="section-heading">
            <h2>手动移动</h2>
            <span className="status-chip">仅收集箱</span>
          </div>
          <div className="move-form">
            <select
              aria-label="源文件"
              value={selectedInboxPath}
              onChange={(event) => setSelectedInboxPath(event.target.value)}
            >
              <option value="">选择收集箱文件</option>
              {inbox
                .filter((item) => !item.isDir)
                .map((item) => (
                  <option key={item.relativePath} value={item.relativePath}>
                    {item.relativePath}
                  </option>
                ))}
            </select>
            <input
              aria-label="目标相对路径"
              value={targetMovePath}
              onChange={(event) => setTargetMovePath(event.target.value)}
              placeholder="目标相对路径，例如 100-项目/新笔记.md"
            />
            <label className="inline-check">
              <input
                type="checkbox"
                checked={overwriteMove}
                onChange={(event) => setOverwriteMove(event.target.checked)}
              />
              覆盖
            </label>
            <button type="button" onClick={moveInboxItem} disabled={!vaultPath}>
              <MoveRight size={16} aria-hidden="true" />
              移动
            </button>
          </div>
          <div className="move-form narrow">
            <input
              aria-label="回滚审计 ID"
              value={rollbackAuditId}
              onChange={(event) => setRollbackAuditId(event.target.value)}
              placeholder="审计 ID"
            />
            <button type="button" onClick={rollbackMove} disabled={!vaultPath}>
              <RotateCcw size={16} aria-hidden="true" />
              回滚
            </button>
          </div>
        </section>

        <section className="section-panel">
          <div className="section-heading">
            <h2>整理候选</h2>
            <span className="status-chip">{organizePlan?.candidates.length ?? 0} 项</span>
          </div>
          <div className="candidate-list">
            {organizePlan?.candidates.map((candidate) => (
              <article className="candidate-row" key={candidate.id}>
                <div>
                  <strong>{candidate.sourceRelativePath}</strong>
                  <span>{candidate.targetRelativePath}</span>
                  <small>{candidate.reason}</small>
                </div>
                <div className="candidate-meta">
                  <span>{percentage(candidate.confidence)}</span>
                  <small>{candidate.tags.join(" / ")}</small>
                </div>
              </article>
            ))}
            {!organizePlan ? <p className="empty-state">尚未生成整理计划</p> : null}
          </div>
        </section>
      </div>
    );
  }

  function renderTodoSchedule() {
    return (
      <div className="view-stack">
        <section className="section-panel">
          <div className="section-heading">
            <h2>TODO / 日程候选</h2>
            <button type="button" className="ghost-button" onClick={() => refreshOperations(vaultPath)}>
              <RefreshCw size={15} aria-hidden="true" />
              刷新
            </button>
          </div>
          <div className="candidate-list">
            {todoCandidates.map((candidate) => (
              <article className="todo-row" key={candidate.id}>
                <div className="todo-main">
                  <span className={`kind-badge ${candidate.kind}`}>
                    {candidate.kind === "todo" ? "TODO" : "日程"}
                  </span>
                  <strong>{candidate.title}</strong>
                  <span>{candidate.sourceRelativePath}</span>
                  <small>{candidate.excerpt}</small>
                </div>
                <div className="todo-meta">
                  <span>{percentage(candidate.confidence)}</span>
                  <span>{candidate.dueAt ? formatDate(candidate.dueAt) : "无时间"}</span>
                  <span>{candidate.status}</span>
                </div>
                <div className="row-actions">
                  <button
                    type="button"
                    onClick={() => confirmCandidate(candidate)}
                    disabled={candidate.status !== "pending"}
                  >
                    <Check size={15} aria-hidden="true" />
                    确认
                  </button>
                  <button
                    type="button"
                    className="secondary-button"
                    onClick={() => dismissCandidate(candidate)}
                    disabled={candidate.status !== "pending"}
                  >
                    忽略
                  </button>
                </div>
              </article>
            ))}
            {todoCandidates.length === 0 ? <p className="empty-state">暂无 TODO 或日程候选</p> : null}
          </div>
        </section>
      </div>
    );
  }

  function renderNotes() {
    return (
      <div className="notes-layout">
        <section className="section-panel notes-list">
          <div className="section-heading">
            <h2>便利贴</h2>
            <button type="button" className="icon-button" title="新建便利贴" onClick={addStickyNote}>
              <Plus size={16} aria-hidden="true" />
            </button>
          </div>
          <div className="dense-list">
            {stickyNotes.map((note) => (
              <button
                type="button"
                key={note.id}
                className={note.id === activeNote?.id ? "active-row" : undefined}
                onClick={() => setActiveNoteId(note.id)}
              >
                <StickyNote size={14} aria-hidden="true" />
                <span>{note.title || "未命名"}</span>
                <small>{formatDate(note.updatedAt)}</small>
              </button>
            ))}
          </div>
        </section>

        <section className="section-panel note-editor">
          <div className="section-heading">
            <h2>{activeNote?.title || "便利贴"}</h2>
            <span className="status-chip">{dirtyNoteId === activeNote?.id ? "等待自动保存" : "已保存"}</span>
          </div>
          {activeNote ? (
            <>
              <input
                aria-label="便利贴标题"
                value={activeNote.title}
                onChange={(event) => updateActiveNote({ title: event.target.value })}
              />
              <textarea
                className="note-textarea"
                value={activeNote.content}
                onChange={(event) => updateActiveNote({ content: event.target.value })}
              />
              <div className="note-footer">
                <input
                  aria-label="自动保存路径"
                  value={activeNote.autosavePath}
                  onChange={(event) => updateActiveNote({ autosavePath: event.target.value })}
                />
                <label className="inline-check">
                  <input
                    type="checkbox"
                    checked={activeNote.pinned}
                    onChange={(event) => updateActiveNote({ pinned: event.target.checked })}
                  />
                  置顶
                </label>
                <button type="button" onClick={() => persistStickyNote(activeNote)}>
                  <Save size={16} aria-hidden="true" />
                  保存
                </button>
                <button type="button" className="danger-button" onClick={deleteActiveNote}>
                  <Trash2 size={16} aria-hidden="true" />
                  删除
                </button>
              </div>
            </>
          ) : (
            <p className="empty-state">暂无便利贴</p>
          )}
        </section>
      </div>
    );
  }

  function renderSettings() {
    return (
      <div className="view-stack">
        <section className="settings-grid">
          <div className="section-panel">
            <div className="section-heading">
              <h2>AI 与整理</h2>
            </div>
            <label className="field">
              整理范式
              <input
                value={settings.organizationTemplate}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    organizationTemplate: event.target.value,
                  }))
                }
              />
            </label>
            <label className="field">
              决策模型
              <input
                value={settings.aiDecisionModel}
                onChange={(event) =>
                  setSettings((current) => ({ ...current, aiDecisionModel: event.target.value }))
                }
              />
            </label>
            <label className="field">
              抽取模型
              <input
                value={settings.extractionModel}
                onChange={(event) =>
                  setSettings((current) => ({ ...current, extractionModel: event.target.value }))
                }
              />
            </label>
          </div>

          <div className="section-panel">
            <div className="section-heading">
              <h2>队列与窗口</h2>
            </div>
            <label className="field">
              单并发数
              <input
                type="number"
                min={1}
                value={settings.queueConcurrency}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    queueConcurrency: Number(event.target.value) || 1,
                  }))
                }
              />
            </label>
            <label className="field">
              重试上限
              <input
                type="number"
                min={0}
                value={settings.retryLimit}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    retryLimit: Number(event.target.value) || 0,
                  }))
                }
              />
            </label>
            <label className="field">
              冷却分钟
              <input
                type="number"
                min={0}
                value={settings.cooldownMinutes}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    cooldownMinutes: Number(event.target.value) || 0,
                  }))
                }
              />
            </label>
            <div className="two-field-row">
              <label className="field">
                预热窗口
                <input
                  type="number"
                  min={0}
                  value={settings.prewarmWindows}
                  onChange={(event) =>
                    setSettings((current) => ({
                      ...current,
                      prewarmWindows: Number(event.target.value) || 0,
                    }))
                  }
                />
              </label>
              <label className="field">
                活跃上限
                <input
                  type="number"
                  min={1}
                  value={settings.activeWindowLimit}
                  onChange={(event) =>
                    setSettings((current) => ({
                      ...current,
                      activeWindowLimit: Number(event.target.value) || 1,
                    }))
                  }
                />
              </label>
            </div>
          </div>

          <div className="section-panel">
            <div className="section-heading">
              <h2>预算</h2>
            </div>
            <label className="field">
              月预算
              <input
                type="number"
                min={0}
                value={settings.budgetMonthlyCents}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    budgetMonthlyCents: Number(event.target.value) || 0,
                  }))
                }
              />
            </label>
            <label className="field">
              硬停止
              <input
                type="number"
                min={0}
                value={settings.budgetHardStopCents}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    budgetHardStopCents: Number(event.target.value) || 0,
                  }))
                }
              />
            </label>
            <button type="button" onClick={saveBudget}>
              <Wallet size={16} aria-hidden="true" />
              保存预算
            </button>
          </div>

          <div className="section-panel">
            <div className="section-heading">
              <h2>便利贴与冲突</h2>
            </div>
            <label className="field">
              自动保存路径
              <input
                value={settings.stickyNotesPath}
                onChange={(event) =>
                  setSettings((current) => ({ ...current, stickyNotesPath: event.target.value }))
                }
              />
            </label>
            <label className="field">
              自动保存秒数
              <input
                type="number"
                min={1}
                value={settings.autoSaveIntervalSeconds}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    autoSaveIntervalSeconds: Number(event.target.value) || 1,
                  }))
                }
              />
            </label>
            <label className="field">
              冲突默认动作
              <select
                value={settings.conflictDefaultAction}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    conflictDefaultAction: event.target.value as ConflictResolutionAction,
                  }))
                }
              >
                <option value="rename">重命名</option>
                <option value="keep_existing">保留现有</option>
                <option value="overwrite">覆盖</option>
                <option value="skip">跳过</option>
              </select>
            </label>
            <label className="inline-check">
              <input
                type="checkbox"
                checked={settings.enableGlobalShortcut}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    enableGlobalShortcut: event.target.checked,
                  }))
                }
              />
              全局快捷键
            </label>
          </div>
        </section>

        <section className="section-panel">
          <div className="section-heading">
            <h2>设置保存</h2>
            {settings.isFallback ? <span className="status-chip warning">后端未接入</span> : null}
          </div>
          <div className="control-grid">
            <button type="button" onClick={saveSettings}>
              <Save size={16} aria-hidden="true" />
              保存设置
            </button>
            <button type="button" className="secondary-button" onClick={() => setSettings(defaultAppSettings)}>
              恢复默认
            </button>
          </div>
        </section>
      </div>
    );
  }

  function renderActiveView() {
    if (activeView === "markdown") return renderMarkdownWorkspace();
    if (activeView === "organize") return renderOrganize();
    if (activeView === "todo") return renderTodoSchedule();
    if (activeView === "notes") return renderNotes();
    if (activeView === "settings") return renderSettings();
    return renderDashboard();
  }

  return (
    <main className="app-shell">
      <aside className="left-rail" aria-label="主导航">
        <div className="brand">
          <HardDrive size={22} aria-hidden="true" />
          <div>
            <strong>TheBrain</strong>
            <span>本地 Vault 工作台</span>
          </div>
        </div>

        <div className="vault-box">
          <div className="vault-row">
            <input
              aria-label="Vault 路径"
              value={vaultPath}
              onChange={(event) => setVaultPath(event.target.value)}
              placeholder="Vault 路径"
            />
            <button type="button" className="icon-button" title="选择 Vault" onClick={chooseVault}>
              <FolderOpen size={18} aria-hidden="true" />
            </button>
          </div>
          <div className="button-row">
            <button type="button" onClick={initVault} disabled={!vaultPath}>
              <Check size={16} aria-hidden="true" />
              初始化
            </button>
            <button type="button" onClick={() => refresh()} disabled={!vaultPath}>
              <RefreshCw size={16} aria-hidden="true" />
              刷新
            </button>
          </div>
        </div>

        <nav className="primary-nav" aria-label="功能">
          {navItems.map((item) => (
            <button
              type="button"
              key={item.id}
              className={activeView === item.id ? "nav-active" : undefined}
              onClick={() => setActiveView(item.id)}
            >
              {item.icon}
              {item.label}
            </button>
          ))}
        </nav>

        <section className="side-section">
          <h2>
            <Inbox size={15} aria-hidden="true" />
            收集箱
          </h2>
          <div className="side-list">
            {inbox.map((item) => (
              <button
                type="button"
                key={item.relativePath}
                onClick={() => !item.isDir && openMarkdown(item.relativePath)}
                disabled={item.isDir}
              >
                <FileText size={14} aria-hidden="true" />
                <span>{item.name}</span>
              </button>
            ))}
          </div>
        </section>

        <section className="side-section">
          <h2>Markdown</h2>
          <div className="side-list">
            {markdownFiles.map((node) => (
              <button
                type="button"
                key={node.relativePath}
                onClick={() => openMarkdown(node.relativePath)}
              >
                <FileText size={14} aria-hidden="true" />
                <span>{node.relativePath}</span>
              </button>
            ))}
          </div>
        </section>

        <section className="side-section">
          <h2>历史归档</h2>
          <div className="side-list">
            {ledger.map((item) => (
              <button
                type="button"
                key={`${item.lineNumber}-${item.targetRelativePath}`}
                onClick={() => item.exists && openMarkdown(item.targetRelativePath)}
                disabled={!item.exists}
              >
                <FileText size={14} aria-hidden="true" />
                <span>{item.displayName}</span>
              </button>
            ))}
          </div>
        </section>
      </aside>

      <section className="main-stage">
        <header className="app-header">
          <div>
            <h1>{viewTitle(activeView)}</h1>
            <span>{vaultPath || "未选择 Vault"}</span>
          </div>
          <div className="header-actions">
            <span className="status-chip">{status}</span>
            {initResult ? <span className="status-chip">Index 已初始化</span> : null}
            {usage?.isFallback ? <span className="status-chip warning">Usage fallback</span> : null}
          </div>
        </header>

        {appError ? (
          <section className="alert-panel" role="alert">
            <ShieldAlert size={18} aria-hidden="true" />
            <div>
              <strong>{appError.title}</strong>
              <span>{appError.detail}</span>
              <small>{appError.recovery}</small>
            </div>
          </section>
        ) : null}

        <div className="view-surface">{renderActiveView()}</div>
      </section>

      <aside className="right-rail" aria-label="状态">
        <section className="rail-panel">
          <div className="section-heading compact">
            <h2>
              <ClipboardCheck size={15} aria-hidden="true" />
              队列
            </h2>
            {queueStatus?.isFallback ? <span className="status-chip warning">占位</span> : null}
          </div>
          <div className="rail-stat">
            <strong>{queueStateLabel(queueStatus?.state)}</strong>
            <span>待处理 {queueStatus?.pending ?? 0}</span>
          </div>
          <div className="mini-grid">
            <span>活动 {queueStatus?.active ?? 0}</span>
            <span>重试 {queueStatus?.retrying ?? 0}</span>
            <span>失败 {queueStatus?.failed ?? 0}</span>
            <span>今日 {queueStatus?.completedToday ?? 0}</span>
          </div>
          <button type="button" className="full-width" onClick={toggleQueue}>
            {queueIsPaused ? <PlayCircle size={16} aria-hidden="true" /> : <PauseCircle size={16} aria-hidden="true" />}
            {queueIsPaused ? "恢复" : "暂停"}
          </button>
        </section>

        <section className="rail-panel">
          <div className="section-heading compact">
            <h2>
              <CircleDollarSign size={15} aria-hidden="true" />
              预算
            </h2>
            {budgetStatus?.exhausted ? <span className="status-chip danger">耗尽</span> : null}
          </div>
          <div className="budget-bar" aria-label="预算使用率">
            <span style={{ width: `${budgetRatio * 100}%` }} />
          </div>
          <div className="rail-stat">
            <strong>{budgetStatus ? formatBudget(budgetStatus.spentCents) : "0.00 元"}</strong>
            <span>上限 {budgetStatus ? formatBudget(budgetStatus.monthlyLimitCents) : "未连接"}</span>
          </div>
          <small>Tokens {budgetStatus?.totalTokens ?? usage?.totalTokens ?? 0}</small>
        </section>

        <section className="rail-panel">
          <div className="section-heading compact">
            <h2>
              <AlertTriangle size={15} aria-hidden="true" />
              冲突
            </h2>
            <span className="status-chip">{conflicts.length}</span>
          </div>
          <div className="conflict-list">
            {conflicts.map((conflict) => (
              <article key={conflict.id} className="conflict-item">
                <strong>{conflict.message}</strong>
                <span>{conflict.sourceRelativePath}</span>
                <small>{conflict.targetRelativePath}</small>
                <div className="conflict-actions">
                  {conflict.options.map((action) => (
                    <button
                      type="button"
                      key={action}
                      className={action === conflict.recommendedAction ? undefined : "secondary-button"}
                      onClick={() => resolveConflict(conflict, action)}
                    >
                      {actionLabel(action)}
                    </button>
                  ))}
                </div>
              </article>
            ))}
            {conflicts.length === 0 ? <p className="empty-state">无冲突</p> : null}
          </div>
        </section>
      </aside>
    </main>
  );
}
