import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  Activity,
  Archive,
  ArrowRight,
  Bot,
  Calendar,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Circle,
  CircleDollarSign,
  ClipboardCheck,
  Clock3,
  CloudUpload,
  Code2,
  Database,
  Download,
  Eye,
  File,
  FileAudio,
  FileImage,
  FileText,
  Folder,
  FolderOpen,
  HardDrive,
  Home,
  ImageIcon,
  Inbox,
  Keyboard,
  LayoutDashboard,
  Lightbulb,
  Link2,
  ListChecks,
  Mail,
  MessageCircle,
  MessageSquare,
  MoreHorizontal,
  Paperclip,
  Pause,
  Pencil,
  Pin,
  Play,
  Plus,
  RefreshCw,
  RotateCcw,
  Save,
  Search,
  Send,
  Settings,
  Shield,
  Sparkles,
  Star,
  StickyNote,
  Target,
  Trash2,
  Upload,
  UserRound,
  Wand2,
  X,
  Zap,
} from "lucide-react";
import DOMPurify from "dompurify";
import { marked } from "marked";
import {
  AiOrganizePlan,
  AiOrganizeResult,
  AppSettings,
  BudgetStatus,
  commands,
  ConflictItem,
  ConflictResolutionAction,
  defaultAppSettings,
  InboxItem,
  InboxImportResult,
  InboxPlanResult,
  LedgerItem,
  MarkdownDocument,
  MimoExtractResult,
  MimoStatus,
  QueueStatus,
  RagAnswer,
  RagIndexRun,
  RagIndexStatus,
  RagTraceRun,
  selectImportFiles,
  selectVault,
  StickyNote as StickyNoteRecord,
  TodoScheduleCandidate,
  UsageSummary,
  VaultInitResult,
  VaultTreeNode,
} from "./api";

type ViewId = "dashboard" | "inbox" | "markdown" | "notes" | "personal" | "project" | "settings";
type ImportBehavior = "copy" | "move";

interface AppError {
  title: string;
  detail: string;
  recovery: string;
}

interface ProjectItem {
  id: string;
  name: string;
  relativePath: string;
  updatedAt: string;
  count: number;
}

const defaultRelativePath = "000-收集箱/新笔记.md";
const settingsStorageKey = "thebrain.settings";
const notesStorageKey = "thebrain.stickyNotes";
const fallbackProjects: ProjectItem[] = [
  { id: "ai-exam", name: "人工智能-期末考试", relativePath: "100-学校/人工智能-期末考试", updatedAt: "10 分钟前", count: 12 },
  { id: "deep-learning", name: "深度学习-期末报告", relativePath: "100-学校/深度学习-期末报告", updatedAt: "1 小时前", count: 8 },
  { id: "front-dox", name: "front-dox", relativePath: "200-工作/front-dox", updatedAt: "6 小时前", count: 16 },
];

const basicNavItems: { id: ViewId; label: string; icon: ReactNode }[] = [
  { id: "dashboard", label: "新对话", icon: <MessageCircle size={17} aria-hidden="true" /> },
  { id: "inbox", label: "收集箱", icon: <Inbox size={17} aria-hidden="true" /> },
  { id: "markdown", label: "Markdown", icon: <FileText size={17} aria-hidden="true" /> },
  { id: "personal", label: "个人页", icon: <UserRound size={17} aria-hidden="true" /> },
  { id: "notes", label: "便利贴", icon: <StickyNote size={17} aria-hidden="true" /> },
];

const importTypes = [
  { label: "TXT / Markdown", icon: <Code2 size={15} aria-hidden="true" /> },
  { label: "PNG / JPG", icon: <FileImage size={15} aria-hidden="true" /> },
  { label: "MP3 / WAV / M4A / AAC", icon: <FileAudio size={15} aria-hidden="true" /> },
  { label: "PDF / DOCX / PPTX 后续", icon: <FileText size={15} aria-hidden="true" /> },
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
  if (Number.isNaN(date.getTime())) return typeof value === "string" ? value : "未记录";
  return date.toLocaleString("zh-CN", { hour12: false });
}

function formatBudget(cents: number): string {
  return `${(cents / 100).toFixed(2)} 元`;
}

function formatBytes(value?: number): string {
  if (!value) return "0 KB";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
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

function pageTitle(view: ViewId): { title: string; subtitle: string } {
  const titles: Record<ViewId, { title: string; subtitle: string }> = {
    dashboard: { title: "主仪表盘", subtitle: "本地优先 · 自动整理 · AI 助手" },
    inbox: { title: "收集箱", subtitle: "无脑丢入 · 自动整理 · 本地优先" },
    markdown: { title: "Markdown 编辑器", subtitle: "Obsidian 式 Vault 树 · 编辑预览 · AI 辅助" },
    notes: { title: "便利贴", subtitle: "Ctrl+Space 快速记录 · Ctrl+Enter 保存" },
    personal: { title: "个人页面", subtitle: "学习、目标、复习与知识沉淀" },
    project: { title: "项目工作台", subtitle: "项目资料、Agent 历史与下一步行动" },
    settings: { title: "设置", subtitle: "本地优先 · AI 配置 · 快捷键 · 自动化" },
  };
  return titles[view];
}

function fileIcon(path: string, size = 15): ReactNode {
  if (/\.(png|jpe?g|gif|webp)$/i.test(path)) return <FileImage size={size} aria-hidden="true" />;
  if (/\.(mp3|wav|m4a|flac|aac|ogg)$/i.test(path)) return <FileAudio size={size} aria-hidden="true" />;
  if (/\.md$/i.test(path)) return <Code2 size={size} aria-hidden="true" />;
  if (/\.url$/i.test(path)) return <Link2 size={size} aria-hidden="true" />;
  return <FileText size={size} aria-hidden="true" />;
}

export default function App() {
  const [activeView, setActiveView] = useState<ViewId>("dashboard");
  const [vaultPath, setVaultPath] = useState(() => localStorage.getItem("thebrain.vault") ?? "");
  const [relativePath, setRelativePath] = useState(defaultRelativePath);
  const [content, setContent] = useState("# 新笔记\n");
  const [frontmatterText, setFrontmatterText] = useState(
    '{\n  "title": "新笔记",\n  "status": "draft"\n}',
  );
  const [markdownDoc, setMarkdownDoc] = useState<MarkdownDocument | null>(null);
  const [initResult, setInitResult] = useState<VaultInitResult | null>(null);
  const [tree, setTree] = useState<VaultTreeNode[]>([]);
  const [inbox, setInbox] = useState<InboxItem[]>([]);
  const [ledger, setLedger] = useState<LedgerItem[]>([]);
  const [usage, setUsage] = useState<UsageSummary | null>(null);
  const [ragStatus, setRagStatus] = useState<RagIndexStatus | null>(null);
  const [ragIndexRun, setRagIndexRun] = useState<RagIndexRun | null>(null);
  const [ragAnswer, setRagAnswer] = useState<RagAnswer | null>(null);
  const [ragTrace, setRagTrace] = useState<RagTraceRun | null>(null);
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
  const [organizePlan, setOrganizePlan] = useState<AiOrganizePlan | null>(null);
  const [organizeResult, setOrganizeResult] = useState<AiOrganizeResult | null>(null);
  const [inboxPlanResult, setInboxPlanResult] = useState<InboxPlanResult | null>(null);
  const [extractResult, setExtractResult] = useState<MimoExtractResult | null>(null);
  const [importResults, setImportResults] = useState<InboxImportResult[]>([]);
  const [mimoStatus, setMimoStatus] = useState<MimoStatus | null>(null);
  const [selectedInboxPath, setSelectedInboxPath] = useState("");
  const [targetMovePath, setTargetMovePath] = useState("");
  const [rollbackAuditId, setRollbackAuditId] = useState("");
  const [importBehavior, setImportBehavior] = useState<ImportBehavior>("copy");
  const [dragActive, setDragActive] = useState(false);
  const [chatPrompt, setChatPrompt] = useState("");
  const [projectPrompt, setProjectPrompt] = useState("");
  const [stickyMode, setStickyMode] = useState<"new" | "continue">("new");
  const [stickyNotes, setStickyNotes] = useState<StickyNoteRecord[]>(() =>
    readStorage<StickyNoteRecord[]>(notesStorageKey, [
      createStickyNote(defaultAppSettings.stickyNotesPath),
    ]),
  );
  const [activeNoteId, setActiveNoteId] = useState("");
  const [dirtyNoteId, setDirtyNoteId] = useState<string | null>(null);
  const [selectedProjectId, setSelectedProjectId] = useState("");

  const allNodes = useMemo(() => flattenTree(tree), [tree]);

  const markdownFiles = useMemo(
    () =>
      allNodes.filter(
        (node) => !node.isDir && /\.(md|markdown)$/i.test(node.relativePath),
      ),
    [allNodes],
  );

  const inboxFiles = useMemo(() => inbox.filter((item) => !item.isDir), [inbox]);

  const projects = useMemo<ProjectItem[]>(() => {
    const topLevelDirs = tree
      .filter((node) => node.isDir && !node.name.startsWith("."))
      .filter((node) => node.name !== "000-收集箱")
      .slice(0, 8)
      .map((node) => ({
        id: node.relativePath,
        name: node.name,
        relativePath: node.relativePath,
        updatedAt: "最近更新",
        count: node.children.length,
      }));
    return topLevelDirs.length > 0 ? topLevelDirs : fallbackProjects;
  }, [tree]);

  const selectedProject = useMemo(
    () => projects.find((project) => project.id === selectedProjectId) ?? projects[0],
    [projects, selectedProjectId],
  );

  const activeNote = useMemo(
    () => stickyNotes.find((note) => note.id === activeNoteId) ?? stickyNotes[0] ?? null,
    [activeNoteId, stickyNotes],
  );

  const pendingCandidates = useMemo(
    () => todoCandidates.filter((candidate) => candidate.status === "pending"),
    [todoCandidates],
  );

  const recentMarkdown = useMemo(() => markdownFiles.slice(0, 5), [markdownFiles]);

  const relatedFiles = useMemo(() => {
    if (!relativePath) return recentMarkdown;
    const firstSegment = relativePath.split("/")[0];
    return markdownFiles
      .filter((node) => node.relativePath !== relativePath)
      .filter((node) => node.relativePath.startsWith(firstSegment))
      .slice(0, 4);
  }, [markdownFiles, recentMarkdown, relativePath]);

  const previewHtml = useMemo(() => {
    const markdown = markdownDoc?.previewMarkdown ?? content;
    const unsafeHtml = marked.parse(markdown, { async: false }) as string;
    return DOMPurify.sanitize(unsafeHtml, { USE_PROFILES: { html: true } });
  }, [content, markdownDoc]);

  const page = pageTitle(activeView);
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
    if (!selectedProjectId && projects[0]) {
      setSelectedProjectId(projects[0].id);
    }
  }, [projects, selectedProjectId]);

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
    const [nextSettings, nextQueue, nextBudget, nextConflicts, nextTodoCandidates, nextNotes, nextMimoStatus, nextRagStatus] =
      await Promise.all([
        commands.getAppSettings(nextVaultPath),
        commands.getQueueStatus(nextVaultPath),
        commands.getBudgetStatus(nextVaultPath),
        commands.listConflicts(nextVaultPath),
        commands.listTodoScheduleCandidates(nextVaultPath),
        commands.listStickyNotes(nextVaultPath),
        nextVaultPath ? commands.getMimoStatus(nextVaultPath) : Promise.resolve(null),
        nextVaultPath ? commands.getRagIndexStatus(nextVaultPath) : Promise.resolve(null),
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
    setMimoStatus(nextMimoStatus);
    setRagStatus(nextRagStatus);
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
    setMarkdownDoc(result);
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
    setMarkdownDoc(result);
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

  async function rebuildRagIndex() {
    if (!vaultPath) {
      setAppError({
        title: "尚未选择 Vault",
        detail: "RAG 索引需要先绑定一个本地 Vault。",
        recovery: "选择并初始化 Vault 后再重建索引。",
      });
      return;
    }
    const result = await run(
      () => commands.rebuildRagIndex(vaultPath),
      "重建 RAG 索引",
      "RAG 索引已更新",
    );
    if (!result) return;
    setRagIndexRun(result);
    const nextStatus = await commands.getRagIndexStatus(vaultPath);
    setRagStatus(nextStatus);
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

  async function importFiles(sourcePaths?: string[]) {
    if (!vaultPath) {
      setAppError({
        title: "尚未选择 Vault",
        detail: "导入前需要先选择并初始化 Vault。",
        recovery: "点击左上角 Vault 按钮选择本地 Vault，再执行导入。",
      });
      return;
    }
    const paths = sourcePaths ?? (await selectImportFiles());
    if (paths.length === 0) return;
    const result = await run(
      () => commands.importToInbox(vaultPath, paths, importBehavior),
      "导入文件到收集箱",
      "导入已完成",
    );
    if (!result) return;
    setImportResults(result);
    const firstImported = result.find((item) =>
      ["imported", "already_inbox", "conflict"].includes(item.status),
    );
    if (firstImported?.relativePath) {
      setSelectedInboxPath(firstImported.relativePath);
      if (firstImported.status !== "conflict") {
        setTargetMovePath(firstImported.relativePath.replace(/^000-[^/]+\//, "100-Organized/"));
      }
    }
    await refresh(vaultPath);
  }

  async function planOrganize() {
    if (!vaultPath) return;
    const source = selectedInboxPath || inboxFiles[0]?.relativePath;
    if (!source) {
      setAppError({
        title: "没有可整理的收集箱文件",
        detail: "请先导入或选择 000-收集箱 内的文件。",
        recovery: "v0.3 只对收集箱内的 md/txt、音频和图片生成整理计划。",
      });
      return;
    }
    const result = await run(
      () => commands.planInboxItem(vaultPath, source),
      "生成整理计划",
      "整理计划已生成",
    );
    if (!result) return;
    setInboxPlanResult(result);
    setExtractResult(result.extraction);
    setSelectedInboxPath(result.plan.sourceRelativePath);
    setTargetMovePath(result.plan.targetRelativePath);
    setBudgetStatus(result.budget);
    setOrganizePlan({
      id: `${result.plan.provider}-${Date.now()}`,
      summary: result.plan.summary || result.plan.reason || "整理计划已生成",
      candidates: [
        {
          id: result.plan.sourceRelativePath,
          sourceRelativePath: result.plan.sourceRelativePath,
          targetRelativePath: result.plan.targetRelativePath,
          confidence: result.plan.confidence,
          reason: result.plan.reason || result.plan.error || "",
          tags: result.plan.tags,
          isFallback: result.plan.isMock || result.extraction.isMock,
          fallbackReason: result.plan.error || result.extraction.error,
        },
      ],
      createdAt: new Date().toISOString(),
      isFallback: result.plan.isMock || result.extraction.isMock,
      fallbackReason: result.plan.error || result.extraction.error,
    });
    setActiveView("inbox");
    await refreshOperations(vaultPath);
  }

  async function runOrganize() {
    const candidate = organizePlan?.candidates[0];
    const source = selectedInboxPath || candidate?.sourceRelativePath;
    const target = targetMovePath || candidate?.targetRelativePath;
    if (!source || !target) {
      setAppError({
        title: "整理移动信息不完整",
        detail: "执行前需要明确 sourceRelativePath 和 targetRelativePath。",
        recovery: "先对一个收集箱文件生成整理计划，或手动填写移动目标。",
      });
      return;
    }
    const result = await run(
      () =>
        commands.runAiOrganize(
          vaultPath,
          organizePlan?.id,
          source,
          target,
          candidate?.reason || inboxPlanResult?.plan.reason,
        ),
      "执行 AI 整理",
      "AI 整理已执行",
    );
    if (!result) return;
    setOrganizeResult(result);
    if (result.auditId) setRollbackAuditId(result.auditId);
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
      () => commands.moveInboxItem(vaultPath, selectedInboxPath, targetMovePath, false),
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
    setStickyMode("new");
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

  async function submitPrompt(prompt: string, scope: string) {
    const trimmed = prompt.trim();
    if (!trimmed) return;
    if (!vaultPath) {
      setAppError({
        title: "尚未选择 Vault",
        detail: "RAG 问答需要先读取本地 Vault 索引。",
        recovery: "选择并初始化 Vault，然后重建 RAG 索引。",
      });
      return;
    }
    const result = await run(
      () => commands.askRag(vaultPath, trimmed, 6),
      `${scope} RAG 问答`,
      "RAG 已回答",
    );
    if (!result) return;
    setRagAnswer(result);
    setRagTrace(result.trace);
  }

  function handleDragEnter(event: React.DragEvent<HTMLElement>) {
    event.preventDefault();
    setDragActive(true);
  }

  function handleDragOver(event: React.DragEvent<HTMLElement>) {
    event.preventDefault();
    event.dataTransfer.dropEffect = importBehavior === "move" ? "move" : "copy";
    setDragActive(true);
  }

  function handleDragLeave(event: React.DragEvent<HTMLElement>) {
    if (event.currentTarget === event.target) {
      setDragActive(false);
    }
  }

  async function handleDrop(event: React.DragEvent<HTMLElement>) {
    event.preventDefault();
    const count = event.dataTransfer.files.length;
    setDragActive(false);
    setActiveView("inbox");
    const paths = Array.from(event.dataTransfer.files)
      .map((file) => (file as File & { path?: string }).path)
      .filter((path): path is string => Boolean(path));
    if (paths.length > 0) {
      await importFiles(paths);
    } else {
      setStatus(count > 0 ? "拖拽文件缺少本地路径，请使用导入文件按钮" : "拖拽导入层已打开");
    }
  }

  function renderVaultTree(nodes: VaultTreeNode[], depth = 0): ReactNode {
    if (nodes.length === 0 && depth === 0) {
      return <p className="sidebar-empty">选择并初始化 Vault 后显示目录树</p>;
    }

    return nodes.map((node) => {
      const isMarkdown = /\.(md|markdown)$/i.test(node.relativePath);
      return (
        <div className="tree-node" key={node.relativePath || node.name}>
          <button
            type="button"
            className={node.relativePath === relativePath ? "tree-row active" : "tree-row"}
            style={{ paddingLeft: `${10 + depth * 14}px` }}
            onClick={() => {
              if (node.isDir) {
                const project = projects.find((entry) => entry.relativePath === node.relativePath);
                if (project) {
                  setSelectedProjectId(project.id);
                  setActiveView("project");
                }
                return;
              }
              if (isMarkdown) {
                void openMarkdown(node.relativePath);
              }
            }}
          >
            {node.isDir ? <ChevronRight size={13} aria-hidden="true" /> : <span className="tree-indent" />}
            {node.isDir ? <Folder size={15} aria-hidden="true" /> : fileIcon(node.relativePath)}
            <span>{node.name}</span>
          </button>
          {node.isDir && node.children.length > 0 ? (
            <div className="tree-children">{renderVaultTree(node.children, depth + 1)}</div>
          ) : null}
        </div>
      );
    });
  }

  function renderLeftSidebar() {
    return (
      <aside className="sidebar" aria-label="主导航">
        <div className="brand-row">
          <div className="brand-mark">
            <HardDrive size={17} aria-hidden="true" />
          </div>
          <strong>TheBrain</strong>
          <button type="button" className="sidebar-collapse" title="折叠侧栏">
            <ChevronDown size={16} aria-hidden="true" />
          </button>
        </div>

        <section className="sidebar-section">
          <h2>基础功能</h2>
          <nav className="nav-stack" aria-label="基础功能">
            {basicNavItems.map((item) => (
              <button
                type="button"
                key={item.id}
                className={activeView === item.id ? "nav-item active" : "nav-item"}
                onClick={() => setActiveView(item.id)}
              >
                {item.icon}
                <span>{item.label}</span>
              </button>
            ))}
          </nav>
        </section>

        <section className="sidebar-section">
          <div className="section-title-row">
            <h2>项目</h2>
            <button type="button" className="tiny-icon-button" title="新建项目" onClick={() => setActiveView("project")}>
              <Plus size={14} aria-hidden="true" />
            </button>
          </div>
          <div className="project-list">
            {projects.map((project) => (
              <button
                type="button"
                key={project.id}
                className={selectedProject?.id === project.id && activeView === "project" ? "project-link active" : "project-link"}
                onClick={() => {
                  setSelectedProjectId(project.id);
                  setActiveView("project");
                }}
              >
                <span>{project.name}</span>
                <small>{project.updatedAt}</small>
              </button>
            ))}
          </div>
        </section>

        <section className="sidebar-section vault-tree-section">
          <h2>Vault</h2>
          <div className="vault-actions">
            <button type="button" className="mini-button" onClick={chooseVault}>
              <FolderOpen size={14} aria-hidden="true" />
              选择
            </button>
            <button type="button" className="mini-button" onClick={initVault} disabled={!vaultPath}>
              <Check size={14} aria-hidden="true" />
              初始化
            </button>
          </div>
          <div className="vault-root">
            <button type="button" className="vault-root-row" onClick={() => setActiveView("inbox")}>
              <Inbox size={15} aria-hidden="true" />
              <span>000-收集箱</span>
              <small>{inboxFiles.length}</small>
            </button>
            <div className="tree-scroll">{renderVaultTree(tree)}</div>
          </div>
        </section>

        <section className="sidebar-section conversation-section">
          <h2>对话 / 历史</h2>
          <div className="conversation-list">
            {ledger.slice(0, 3).map((item) => (
              <button
                type="button"
                key={`${item.lineNumber}-${item.targetRelativePath}`}
                onClick={() => item.exists && openMarkdown(item.targetRelativePath)}
                disabled={!item.exists}
              >
                <span>{item.displayName}</span>
                <small>归档记录</small>
              </button>
            ))}
            {ledger.length === 0 ? <p className="sidebar-empty">暂无归档历史</p> : null}
          </div>
        </section>

        <button type="button" className={activeView === "settings" ? "settings-link active" : "settings-link"} onClick={() => setActiveView("settings")}>
          <Settings size={17} aria-hidden="true" />
          设置
        </button>
      </aside>
    );
  }

  function renderTopbar() {
    return (
      <header className="topbar">
        <div>
          <h1>{page.title}</h1>
          <p>{page.subtitle}</p>
        </div>
        <div className="topbar-actions">
          <button type="button" className="ghost-button" onClick={() => refresh()}>
            <RefreshCw size={15} aria-hidden="true" />
            刷新
          </button>
          <span className="sync-pill">
            <Circle size={9} aria-hidden="true" />
            同步状态 · 5 分钟前
          </span>
          <button type="button" className="avatar-button" title="个人页" onClick={() => setActiveView("personal")}>
            <UserRound size={18} aria-hidden="true" />
          </button>
        </div>
      </header>
    );
  }

  function renderPromptBox(kind: "dashboard" | "project") {
    const value = kind === "dashboard" ? chatPrompt : projectPrompt;
    const setValue = kind === "dashboard" ? setChatPrompt : setProjectPrompt;
    const placeholder =
      kind === "dashboard"
        ? "输入你的问题或想法，按 Enter 发送，Shift + Enter 换行"
        : `询问关于 ${selectedProject?.name ?? "项目"} 的问题，或让 Agent 帮你推进项目...`;
    return (
      <div className={kind === "dashboard" ? "prompt-box hero-prompt" : "prompt-box"}>
        <textarea
          value={value}
          placeholder={placeholder}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              void submitPrompt(value, kind === "dashboard" ? "主对话" : "项目对话");
              setValue("");
            }
          }}
        />
        <div className="prompt-footer">
          <div className="prompt-tools">
            <button type="button" className="icon-soft-button" title="添加资料" onClick={() => setActiveView("inbox")}>
              <Plus size={17} aria-hidden="true" />
            </button>
            <button type="button" className="chip-button">
              <Sparkles size={15} aria-hidden="true" />
              智能模式
              <ChevronDown size={14} aria-hidden="true" />
            </button>
          </div>
          <div className="prompt-tools">
            <button type="button" className="chip-button">RAG · MiMo</button>
            <button type="button" className="chip-button api-connected">
              <Circle size={8} aria-hidden="true" />
              {mimoStatus?.hasKey ? "MiMo ready" : "本地检索"}
            </button>
            <button type="button" className="send-button" onClick={() => void submitPrompt(value, kind === "dashboard" ? "主对话" : "项目对话")}>
              <Send size={18} aria-hidden="true" />
            </button>
          </div>
        </div>
      </div>
    );
  }

  function renderRagPanel() {
    const lastRun = ragStatus?.lastRun ?? ragIndexRun;
    const trace = ragTrace ?? ragAnswer?.trace ?? null;
    return (
      <section className="rag-panel">
        <article className="concept-card rag-answer-card">
          <div className="card-heading">
            <h3>
              <Bot size={18} aria-hidden="true" />
              长期记忆问答
            </h3>
            <button type="button" className="ghost-button" onClick={rebuildRagIndex} disabled={!vaultPath}>
              <RefreshCw size={15} aria-hidden="true" />
              重建索引
            </button>
          </div>
          <div className="rag-stats">
            <div>
              <span>文档</span>
              <strong>{ragStatus?.documentCount ?? 0}</strong>
            </div>
            <div>
              <span>Chunks</span>
              <strong>{ragStatus?.chunkCount ?? 0}</strong>
            </div>
            <div>
              <span>Schema</span>
              <strong>v{ragStatus?.schemaVersion ?? 0}</strong>
            </div>
            <div>
              <span>最近运行</span>
              <strong>{lastRun?.status ?? "未运行"}</strong>
            </div>
          </div>
          {lastRun ? (
            <div className="rag-run-line">
              <span>扫描 {lastRun.scannedCount}</span>
              <span>更新 {lastRun.indexedCount}</span>
              <span>跳过 {lastRun.skippedCount}</span>
              <span>删除 {lastRun.deletedCount}</span>
            </div>
          ) : null}
          {ragAnswer ? (
            <div className="rag-answer">
              <div className="rag-answer-meta">
                <span>{ragAnswer.provider} · {ragAnswer.model}</span>
                <span>{ragAnswer.status}</span>
                {ragAnswer.isMock ? <span>fallback</span> : null}
              </div>
              <p>{ragAnswer.answer}</p>
              {ragAnswer.fallbackReason ? <small>{ragAnswer.fallbackReason}</small> : null}
            </div>
          ) : (
            <p className="empty-state">暂无 RAG 回答</p>
          )}
          <div className="rag-citations">
            {(ragAnswer?.citations ?? []).map((citation) => {
              const canOpen = /\.(md|markdown)$/i.test(citation.relativePath);
              return (
                <button
                  type="button"
                  key={`${citation.id}-${citation.relativePath}-${citation.startLine}`}
                  onClick={() => {
                    if (canOpen) {
                      void openMarkdown(citation.relativePath);
                    } else {
                      setStatus(`引用文件：${citation.relativePath}`);
                    }
                  }}
                >
                  <strong>{citation.id}</strong>
                  <span>{citation.title}</span>
                  <small>{citation.relativePath} · {citation.startLine}-{citation.endLine} · {percentage(Math.min(1, citation.score))}</small>
                  <em>{citation.snippet}</em>
                </button>
              );
            })}
          </div>
        </article>

        <article className="concept-card rag-trace-card">
          <div className="card-heading">
            <h3>
              <Database size={18} aria-hidden="true" />
              Trace
            </h3>
            <button
              type="button"
              className="text-link-button"
              onClick={async () => {
                if (!vaultPath) return;
                const nextTrace = await commands.getLatestRagTrace(vaultPath);
                setRagTrace(nextTrace);
              }}
            >
              刷新
            </button>
          </div>
          <div className="trace-list">
            {(trace?.nodes ?? []).map((node) => (
              <div key={node.id}>
                <span>{node.nodeType}</span>
                <strong>{node.name}</strong>
                <small>{node.status} · {node.durationMs ?? 0}ms</small>
              </div>
            ))}
            {trace?.nodes.length ? null : <p className="empty-state">暂无 Trace</p>}
          </div>
        </article>
      </section>
    );
  }

  function renderDashboard() {
    return (
      <div className="dashboard-page">
        <section className="dashboard-hero">
          <h2>你好，今天想探索或整理些什么？</h2>
          {renderPromptBox("dashboard")}
        </section>

        {renderRagPanel()}

        <section className="concept-grid three">
          <article className="concept-card">
            <div className="card-heading">
              <h3>
                <Calendar size={18} aria-hidden="true" />
                日程提醒与管理
              </h3>
              <span>{pendingCandidates.length}</span>
            </div>
            <div className="task-groups">
              {pendingCandidates.slice(0, 4).map((candidate) => (
                <div className="task-line" key={candidate.id}>
                  <input type="checkbox" aria-label={candidate.title} />
                  <span>{candidate.title}</span>
                  <small>{candidate.dueAt ? formatDate(candidate.dueAt) : "待确认"}</small>
                </div>
              ))}
              {pendingCandidates.length === 0 ? (
                <>
                  <div className="task-line">
                    <input type="checkbox" aria-label="深度学习作业" />
                    <span>深度学习-作业4</span>
                    <small>今天</small>
                  </div>
                  <div className="task-line">
                    <input type="checkbox" aria-label="项目评审" />
                    <span>项目评审准备</span>
                    <small>明天</small>
                  </div>
                </>
              ) : null}
            </div>
            <button type="button" className="text-link-button" onClick={() => setActiveView("inbox")}>
              添加任务 <ArrowRight size={14} aria-hidden="true" />
            </button>
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3>
                <Clock3 size={18} aria-hidden="true" />
                你先前停留的位置
              </h3>
            </div>
            <div className="recent-list">
              {recentMarkdown.slice(0, 3).map((node) => (
                <button type="button" key={node.relativePath} onClick={() => openMarkdown(node.relativePath)}>
                  {fileIcon(node.relativePath)}
                  <span>{node.name}</span>
                  <small>{node.relativePath}</small>
                </button>
              ))}
              {recentMarkdown.length === 0 ? (
                <p className="empty-state">刷新 Vault 后会显示最近 Markdown 文件</p>
              ) : null}
            </div>
            <div className="review-list">
              <strong>推荐复习</strong>
              <span>线性代数基础知识</span>
              <span>Python 常用语法速查</span>
            </div>
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3>
                <LayoutDashboard size={18} aria-hidden="true" />
                更多功能
              </h3>
            </div>
            <div className="feature-grid">
              <button type="button" onClick={() => setActiveView("inbox")}>
                <Mail size={25} aria-hidden="true" />
                <strong>连接邮箱</strong>
                <span>收集与整理邮件内容</span>
              </button>
              <button type="button" onClick={() => setActiveView("inbox")}>
                <FolderOpen size={25} aria-hidden="true" />
                <strong>连接文件</strong>
                <span>导入本地文件与资料</span>
              </button>
              <button type="button" onClick={planOrganize}>
                <Wand2 size={25} aria-hidden="true" />
                <strong>AI 整理模板</strong>
                <span>智能分类与结构化</span>
              </button>
              <button type="button" onClick={() => setActiveView("settings")}>
                <Zap size={25} aria-hidden="true" />
                <strong>自动化</strong>
                <span>队列、快捷键与预算</span>
              </button>
            </div>
          </article>
        </section>
      </div>
    );
  }

  function renderImportZone(compact = false) {
    return (
      <section className={compact ? "import-zone compact" : "import-zone"}>
        <CloudUpload size={40} aria-hidden="true" />
        <h3>拖入或选择文件到收集箱</h3>
        <p>v0.3 支持 md/txt、mp3/wav/m4a/aac、png/jpg/jpeg；导入、抽取、计划和移动都通过后端受控命令执行。</p>
        <div className="import-type-row">
          {importTypes.map((type) => (
            <span key={type.label}>
              {type.icon}
              {type.label}
            </span>
          ))}
        </div>
      </section>
    );
  }

  function renderInboxPage() {
    const plannedRows = organizePlan?.candidates ?? [];
    const plannedSources = new Set(plannedRows.map((item) => item.sourceRelativePath));
    const queueRows = [
      ...plannedRows,
      ...inboxFiles.filter((item) => !plannedSources.has(item.relativePath)),
    ];
    return (
      <div className="inbox-page">
        <div className="page-action-row">
          <button type="button" className="ghost-button" onClick={() => importFiles()}>
            <Upload size={16} aria-hidden="true" />
            导入文件
          </button>
          <button type="button" className="secondary-button" onClick={planOrganize} disabled={!vaultPath}>
            <Sparkles size={16} aria-hidden="true" />
            生成计划
          </button>
          <button type="button" onClick={runOrganize} disabled={!vaultPath}>
            <Play size={16} aria-hidden="true" />
            运行整理
          </button>
          <button type="button" className="secondary-button" onClick={toggleQueue}>
            {queueIsPaused ? <Play size={16} aria-hidden="true" /> : <Pause size={16} aria-hidden="true" />}
            {queueIsPaused ? "恢复 AI" : "暂停 AI"}
          </button>
        </div>

        <section className="inbox-layout">
          <div className="inbox-main">
            {renderImportZone()}
            <div className="import-options">
              <label>
                <input
                  type="radio"
                  checked={importBehavior === "copy"}
                  onChange={() => setImportBehavior("copy")}
                />
                导入后保留原文件
              </label>
              <label>
                <input
                  type="radio"
                  checked={importBehavior === "move"}
                  onChange={() => setImportBehavior("move")}
                />
                导入后删除原文件
              </label>
              <span>真实文件不会被前端直接移动，后端命令会负责安全边界。</span>
            </div>
            {importResults.length > 0 ? (
              <div className="operation-summary">
                {importResults.slice(0, 4).map((item) => (
                  <span key={`${item.sourcePath}-${item.status}`}>
                    {item.status}: {item.relativePath ?? item.fileName ?? item.sourcePath}
                  </span>
                ))}
              </div>
            ) : null}
            {extractResult ? (
              <div className="operation-summary">
                <span>
                  extract {extractResult.provider}/{extractResult.model}: {extractResult.status}
                  {extractResult.isMock ? " fallback" : ""}
                </span>
                {extractResult.error ? <span>{extractResult.error}</span> : null}
              </div>
            ) : null}

            <section className="concept-card table-card">
              <div className="card-heading">
                <h3>
                  <ClipboardCheck size={18} aria-hidden="true" />
                  AI 整理队列
                </h3>
                <span>{queueRows.length}</span>
              </div>
              <div className="queue-table">
                <div className="queue-head">
                  <span>文件</span>
                  <span>提取状态</span>
                  <span>AI 分类</span>
                  <span>目标路径</span>
                  <span>置信度</span>
                </div>
                {queueRows.map((item, index) => {
                  const isInboxItem = "relativePath" in item;
                  const sourcePath = isInboxItem ? item.relativePath : item.sourceRelativePath;
                  const targetPath = isInboxItem
                    ? `100-Organized / ${item.name}`
                    : item.targetRelativePath;
                  const confidence = isInboxItem ? 0 : item.confidence;
                  const isSelectedPlan = !isInboxItem && inboxPlanResult?.plan.sourceRelativePath === sourcePath;
                  return (
                    <button
                      type="button"
                      className={selectedInboxPath === sourcePath ? "queue-row active" : "queue-row"}
                      key={sourcePath}
                      onClick={() => {
                        setSelectedInboxPath(sourcePath);
                        setTargetMovePath(targetPath.replaceAll(" / ", "/"));
                      }}
                    >
                      <span>{fileIcon(sourcePath)} {sourcePath.split("/").pop()}</span>
                      <span className={isSelectedPlan ? "green-text" : "muted-text"}>
                        {isSelectedPlan ? `${inboxPlanResult?.extraction.status} / ${inboxPlanResult?.plan.status}` : "等待计划"}
                      </span>
                      <span>{isInboxItem ? "待抽取" : item.tags.join("、") || "待确认"}</span>
                      <span>{targetPath}</span>
                      <span>
                        {percentage(confidence)}
                        <i style={{ width: `${Math.max(18, confidence * 100)}%` }} />
                      </span>
                    </button>
                  );
                })}
                {queueRows.length === 0 ? <p className="empty-state">收集箱暂无待整理文件</p> : null}
              </div>
            </section>

            <section className="concept-card move-card">
              <div className="card-heading">
                <h3>
                  <Archive size={18} aria-hidden="true" />
                  移动与回滚
                </h3>
                {organizeResult ? <span>{organizeResult.message}</span> : null}
              </div>
              <div className="move-grid">
                <input
                  aria-label="收集箱文件"
                  value={selectedInboxPath}
                  onChange={(event) => setSelectedInboxPath(event.target.value)}
                  placeholder="000-收集箱/文件.md"
                />
                <input
                  aria-label="目标相对路径"
                  value={targetMovePath}
                  onChange={(event) => setTargetMovePath(event.target.value)}
                  placeholder="100-学校/课程资料/文件.md"
                />
                <button type="button" onClick={moveInboxItem} disabled={!vaultPath}>
                  <ArrowRight size={16} aria-hidden="true" />
                  移动
                </button>
                <button type="button" className="secondary-button" onClick={rollbackMove} disabled={!vaultPath}>
                  <RotateCcw size={16} aria-hidden="true" />
                  回滚
                </button>
              </div>
              {inboxPlanResult ? (
                <div className="plan-detail">
                  <span>{inboxPlanResult.plan.provider}/{inboxPlanResult.plan.model}</span>
                  <strong>{percentage(inboxPlanResult.plan.confidence)}</strong>
                  <p>{inboxPlanResult.plan.reason || inboxPlanResult.plan.error || inboxPlanResult.plan.summary}</p>
                </div>
              ) : null}
            </section>
          </div>

          <aside className="inbox-aside">
            <section className="concept-card">
              <div className="card-heading">
                <h3>整理模板</h3>
                <button type="button" className="text-link-button">管理模板</button>
              </div>
              <div className="template-tabs">
                {["学生模式", "工作模式", "混合模式", "频率模式"].map((template) => (
                  <button
                    type="button"
                    key={template}
                    className={settings.organizationTemplate.includes(template.slice(0, 2)) ? "active" : undefined}
                    onClick={() => setSettings((current) => ({ ...current, organizationTemplate: template }))}
                  >
                    {template}
                  </button>
                ))}
              </div>
              <ul className="check-list">
                <li>优先识别课程与学习内容</li>
                <li>自动提取学习笔记与知识点</li>
                <li>识别作业、试题与参考资料</li>
              </ul>
            </section>

            <section className="concept-card">
              <div className="card-heading">
                <h3>待确认日程</h3>
                <span>{pendingCandidates.length}</span>
              </div>
              {pendingCandidates.slice(0, 2).map((candidate) => (
                <article className="schedule-card" key={candidate.id}>
                  <Calendar size={20} aria-hidden="true" />
                  <div>
                    <strong>{candidate.title}</strong>
                    <span>{candidate.excerpt}</span>
                    <small>来源：{candidate.sourceRelativePath}</small>
                  </div>
                  <button type="button" onClick={() => confirmCandidate(candidate)}>添加到日程</button>
                  <button type="button" className="secondary-button" onClick={() => dismissCandidate(candidate)}>忽略</button>
                </article>
              ))}
              {pendingCandidates.length === 0 ? <p className="empty-state">暂无待确认日程</p> : null}
            </section>

            <section className="concept-card">
              <div className="card-heading">
                <h3>整理动态</h3>
                <button type="button" className="text-link-button">查看全部</button>
              </div>
              <div className="activity-list">
                {(organizePlan?.candidates ?? []).slice(0, 5).map((candidate, index) => (
                  <div key={candidate.id}>
                    <small>10:{21 - index}</small>
                    <span>{candidate.sourceRelativePath} {"->"} {candidate.targetRelativePath}</span>
                    <strong>{index === 0 ? "已完成" : "等待中"}</strong>
                  </div>
                ))}
                {!organizePlan ? (
                  <>
                    <div><small>10:21</small><span>{"课程笔记.pdf -> 100-学校 / 课程资料"}</span><strong>已完成</strong></div>
                    <div><small>10:20</small><span>{"客户会议录音.mp3 -> 处理中"}</span><strong>处理中</strong></div>
                  </>
                ) : null}
              </div>
            </section>
          </aside>
        </section>
      </div>
    );
  }

  function renderMarkdownWorkspace() {
    return (
      <div className="markdown-page">
        <div className="breadcrumb-row">
          <span>Vault</span>
          <ChevronRight size={14} aria-hidden="true" />
          <span>{relativePath.split("/").slice(0, -1).join(" / ") || "000-收集箱"}</span>
          <ChevronRight size={14} aria-hidden="true" />
          <strong>{relativePath.split("/").pop()}</strong>
          <div className="breadcrumb-actions">
            <span className="save-state"><Check size={14} aria-hidden="true" /> 已保存 09:42</span>
            <button type="button" className="ghost-button" onClick={exportMarkdown}>
              <Download size={15} aria-hidden="true" />
              导出
            </button>
            <button type="button" className="ghost-button">
              <Link2 size={15} aria-hidden="true" />
              插入链接
            </button>
          </div>
        </div>

        <section className="markdown-layout">
          <aside className="markdown-tree">
            <h3>Vault 文件</h3>
            <div className="tree-scroll large">{renderVaultTree(tree)}</div>
          </aside>

          <section className="editor-surface">
            <div className="editor-toolbar">
              {["正文", "H1", "H2", "H3", "B", "I"].map((tool) => (
                <button type="button" className="toolbar-button" key={tool}>{tool}</button>
              ))}
              <button type="button" className="toolbar-button"><Code2 size={15} aria-hidden="true" /></button>
              <button type="button" className="toolbar-button"><Link2 size={15} aria-hidden="true" /></button>
              <button type="button" className="toolbar-button"><ImageIcon size={15} aria-hidden="true" /></button>
              <button type="button" className="toolbar-button"><MoreHorizontal size={15} aria-hidden="true" /></button>
              <button type="button" className="save-inline" onClick={saveMarkdown}>
                <Save size={15} aria-hidden="true" />
                保存
              </button>
            </div>
            <div className="editor-preview-grid">
              <div className="code-editor-pane">
                <textarea
                  value={content}
                  onChange={(event) => setContent(event.target.value)}
                  aria-label="Markdown 正文"
                  spellCheck={false}
                />
                <footer>
                  <span>行 1，列 1</span>
                  <span>字数：{content.length}</span>
                  <span>Markdown</span>
                </footer>
              </div>
              <div className="preview-pane">
                <div className="pane-heading">
                  <h3>预览</h3>
                  <Eye size={15} aria-hidden="true" />
                </div>
                <article className="markdown-preview" dangerouslySetInnerHTML={{ __html: previewHtml }} />
              </div>
            </div>
          </section>

          <aside className="markdown-inspector">
            <section className="inspector-card">
              <h3>AI 助手</h3>
              <button type="button"><Bot size={15} aria-hidden="true" /> 总结本章</button>
              <button type="button"><Sparkles size={15} aria-hidden="true" /> 生成复习卡片</button>
              <button type="button"><MessageSquare size={15} aria-hidden="true" /> 解释算法复杂度</button>
              <div className="mini-prompt">
                <input placeholder="询问关于本文的问题..." />
                <button type="button" className="send-button"><Send size={15} aria-hidden="true" /></button>
              </div>
            </section>

            <section className="inspector-card">
              <h3>相关文件</h3>
              {relatedFiles.map((file) => (
                <button type="button" key={file.relativePath} onClick={() => openMarkdown(file.relativePath)}>
                  {fileIcon(file.relativePath)}
                  <span>{file.name}</span>
                  <small>{file.relativePath.split("/").slice(0, -1).join(" / ")}</small>
                </button>
              ))}
              {relatedFiles.length === 0 ? <p className="empty-state">暂无相关文件</p> : null}
            </section>

            <section className="inspector-card">
              <h3>文档信息</h3>
              <dl className="doc-info">
                <dt>创建日期</dt>
                <dd>2025/05/10 14:32</dd>
                <dt>最近修改</dt>
                <dd>今天 09:42</dd>
                <dt>类型</dt>
                <dd>学习笔记</dd>
                <dt>访问频率</dt>
                <dd><span className="frequency-bars"><i /><i /><i /><i /><i /></span></dd>
              </dl>
              <textarea
                className="frontmatter-editor"
                value={frontmatterText}
                onChange={(event) => setFrontmatterText(event.target.value)}
                aria-label="Frontmatter JSON"
              />
              {exported ? <textarea className="export-output" readOnly value={exported} aria-label="导出 Markdown" /> : null}
            </section>
          </aside>
        </section>
      </div>
    );
  }

  function renderStickyNotes() {
    return (
      <div className="sticky-page">
        <section className="sticky-window" aria-label="便利贴窗口">
          <header>
            <div className="sticky-tabs">
              <button type="button" className={stickyMode === "new" ? "active" : undefined} onClick={() => setStickyMode("new")}>新建</button>
              <button type="button" className={stickyMode === "continue" ? "active" : undefined} onClick={() => setStickyMode("continue")}>
                继续之前的便签
                <ChevronDown size={14} aria-hidden="true" />
              </button>
            </div>
            <div className="sticky-window-actions">
              <button type="button" className="icon-soft-button" title="置顶" onClick={() => activeNote && updateActiveNote({ pinned: !activeNote.pinned })}>
                <Pin size={17} aria-hidden="true" />
              </button>
              <button type="button" className="icon-soft-button" title="关闭" onClick={() => setActiveView("dashboard")}>
                <X size={18} aria-hidden="true" />
              </button>
            </div>
          </header>

          {stickyMode === "continue" ? (
            <div className="sticky-history-popover">
              {stickyNotes.slice(0, 3).map((note) => (
                <button
                  type="button"
                  key={note.id}
                  onClick={() => {
                    setActiveNoteId(note.id);
                    setStickyMode("new");
                  }}
                >
                  <FileText size={18} aria-hidden="true" />
                  <span>{note.title || "未命名便签"}</span>
                  <small>{formatDate(note.updatedAt)}</small>
                </button>
              ))}
            </div>
          ) : null}

          {activeNote ? (
            <>
              <input
                className="sticky-title-input"
                aria-label="便利贴标题"
                value={activeNote.title}
                placeholder="标题（可选）"
                onChange={(event) => updateActiveNote({ title: event.target.value })}
              />
              <textarea
                className="sticky-body-input"
                value={activeNote.content}
                placeholder="写点什么......"
                onChange={(event) => updateActiveNote({ content: event.target.value })}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && event.ctrlKey) {
                    event.preventDefault();
                    void persistStickyNote(activeNote);
                  }
                }}
              />
              <div className="sticky-tool-row">
                <button type="button" title="Markdown"><Code2 size={17} aria-hidden="true" /></button>
                <button type="button" title="截图"><ImageIcon size={17} aria-hidden="true" /></button>
                <button type="button" title="附件"><Paperclip size={17} aria-hidden="true" /></button>
                <button type="button" title="链接"><Link2 size={17} aria-hidden="true" /></button>
              </div>
              <footer>
                <span>
                  <Folder size={17} aria-hidden="true" />
                  已自动保存到 000-收集箱 / 便签草稿
                  <Circle size={8} aria-hidden="true" />
                </span>
                <kbd>Ctrl+Space 打开</kbd>
                <kbd>Ctrl+Enter 新建便签</kbd>
                <button type="button" onClick={() => persistStickyNote(activeNote)}>
                  <Save size={16} aria-hidden="true" />
                  保存
                </button>
              </footer>
            </>
          ) : (
            <div className="empty-sticky">
              <p>暂无便利贴</p>
              <button type="button" onClick={addStickyNote}>新建便签</button>
            </div>
          )}
        </section>

        <aside className="sticky-side-list">
          <div className="card-heading">
            <h3>最近便签</h3>
            <button type="button" className="tiny-icon-button" onClick={addStickyNote}>
              <Plus size={14} aria-hidden="true" />
            </button>
          </div>
          {stickyNotes.map((note) => (
            <button
              type="button"
              key={note.id}
              className={activeNote?.id === note.id ? "active" : undefined}
              onClick={() => setActiveNoteId(note.id)}
            >
              <StickyNote size={15} aria-hidden="true" />
              <span>{note.title || "未命名便签"}</span>
              <small>{formatDate(note.updatedAt)}</small>
            </button>
          ))}
          {activeNote ? (
            <button type="button" className="danger-soft" onClick={deleteActiveNote}>
              <Trash2 size={15} aria-hidden="true" />
              删除当前便签
            </button>
          ) : null}
        </aside>
      </div>
    );
  }

  function renderPersonalPage() {
    return (
      <div className="personal-page">
        <section className="concept-grid three">
          <article className="concept-card chart-card">
            <div className="card-heading">
              <h3><FileText size={18} aria-hidden="true" /> 文件活跃度</h3>
              <button type="button" className="chip-button">近 30 天</button>
            </div>
            <div className="stat-pair">
              <div><span>本月创建</span><strong>{markdownFiles.length + inboxFiles.length}</strong></div>
              <div><span>本月修改</span><strong>{Math.max(18, markdownFiles.length * 3)}</strong></div>
            </div>
            <div className="line-chart" aria-label="文件活跃度趋势">
              <i style={{ height: "35%" }} /><i style={{ height: "62%" }} /><i style={{ height: "48%" }} /><i style={{ height: "76%" }} /><i style={{ height: "58%" }} /><i style={{ height: "82%" }} /><i style={{ height: "64%" }} />
            </div>
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3><FileText size={18} aria-hidden="true" /> 常用文件与打开频率</h3>
              <button type="button" className="chip-button">近 30 天</button>
            </div>
            <div className="rank-list">
              {recentMarkdown.slice(0, 5).map((file, index) => (
                <button type="button" key={file.relativePath} onClick={() => openMarkdown(file.relativePath)}>
                  <strong>{index + 1}</strong>
                  <span>{file.name}</span>
                  <small>{48 - index * 6} 次</small>
                  <i style={{ width: `${75 - index * 8}%` }} />
                </button>
              ))}
              {recentMarkdown.length === 0 ? <p className="empty-state">暂无常用文件数据</p> : null}
            </div>
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3><Target size={18} aria-hidden="true" /> 目标 / TODO / 愿景 / 创意想法</h3>
            </div>
            <div className="goal-grid">
              <div><Target size={26} aria-hidden="true" /><strong>8</strong><span>目标</span></div>
              <div><CheckCircle2 size={26} aria-hidden="true" /><strong>{pendingCandidates.length || 25}</strong><span>TODO</span></div>
              <div><Star size={26} aria-hidden="true" /><strong>3</strong><span>愿景</span></div>
              <div><Lightbulb size={26} aria-hidden="true" /><strong>17</strong><span>创意想法</span></div>
            </div>
          </article>
        </section>

        <section className="concept-grid three">
          <article className="concept-card">
            <div className="card-heading"><h3><Activity size={18} aria-hidden="true" /> 学习进度</h3></div>
            {["期末复习进度", "《深度学习》阅读进度", "CS61A 课程学习"].map((item, index) => (
              <div className="progress-line" key={item}>
                <span>{item}</span>
                <strong>{68 - index * 8}%</strong>
                <i><b style={{ width: `${68 - index * 8}%` }} /></i>
              </div>
            ))}
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3><Shield size={18} aria-hidden="true" /> 推荐复习</h3>
              <button type="button" className="chip-button">智能推荐</button>
            </div>
            <div className="review-table">
              {["线性代数基础知识", "深度学习-损失函数", "Python 装饰器详解", "计算机网络-TCP 三次握手"].map((item, index) => (
                <div key={item}>
                  <span>{item}</span>
                  <small>{2 + index} 天后</small>
                </div>
              ))}
            </div>
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3><Database size={18} aria-hidden="true" /> 最近积累</h3>
              <button type="button" className="chip-button">近 7 天</button>
            </div>
            <div className="accumulation-list">
              <div><FileText size={22} aria-hidden="true" /><span>新建笔记</span><strong>{markdownFiles.length || 18}</strong></div>
              <div><Archive size={22} aria-hidden="true" /><span>收藏内容</span><strong>{ledger.length || 9}</strong></div>
              <div><Bot size={22} aria-hidden="true" /><span>知识增长</span><strong>+145</strong></div>
            </div>
          </article>
        </section>

        <section className="stats-ribbon">
          <div><Activity size={29} aria-hidden="true" /><strong>16 天</strong><span>连续记录</span></div>
          <div><Clock3 size={29} aria-hidden="true" /><strong>2.6 小时</strong><span>今日专注时长</span></div>
          <div><CheckCircle2 size={29} aria-hidden="true" /><strong>12 项</strong><span>本周完成任务</span></div>
          <div><Link2 size={29} aria-hidden="true" /><strong>{allNodes.length}</strong><span>知识连接数</span></div>
          <div><Lightbulb size={29} aria-hidden="true" /><strong>7 次</strong><span>灵感时刻</span></div>
        </section>
      </div>
    );
  }

  function renderProjectWorkspace() {
    return (
      <div className="project-page">
        <div className="project-title-row">
          <div>
            <span>项目工作台</span>
            <h2>
              <FolderOpen size={28} aria-hidden="true" />
              {selectedProject?.name ?? "TheBrain"}
              <ChevronDown size={18} aria-hidden="true" />
            </h2>
            <p>智能文档审查与生成工具 · 最近更新 {selectedProject?.updatedAt ?? "6 小时前"}</p>
          </div>
          <div className="project-actions">
            <button type="button" className="ghost-button"><Settings size={16} aria-hidden="true" /> 项目设置</button>
            <button type="button" className="icon-soft-button"><MoreHorizontal size={16} aria-hidden="true" /></button>
          </div>
        </div>

        <section className="project-grid">
          <article className="concept-card">
            <div className="card-heading"><h3><ClipboardCheck size={18} aria-hidden="true" /> 项目概览</h3></div>
            <p>构建一个智能文档审查与生成平台，支持团队协作、多格式解析与 AI 辅助写作。</p>
            <div className="milestone-row">
              <CheckCircle2 size={17} aria-hidden="true" />
              <span>实现文档解析与向量索引</span>
              <small>6 小时前</small>
            </div>
            <div className="project-progress"><i style={{ width: "62%" }} /></div>
            <footer><span>创建于 1 周前</span><strong>62%</strong></footer>
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3><MessageSquare size={18} aria-hidden="true" /> 最近对话 / Agent 历史</h3>
              <button type="button" className="text-link-button">查看全部</button>
            </div>
            <div className="agent-history">
              {["实现文档解析与向量索引", "确认进度", "同步到 GitHub", "Build GO repo scaffold plus S1 web framework", "列出项目技能"].map((item, index) => (
                <div key={item}>
                  <MessageCircle size={16} aria-hidden="true" />
                  <span>{item}</span>
                  <small>{index < 2 ? "6 小时前" : "1 周前"}</small>
                </div>
              ))}
            </div>
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3><FileText size={18} aria-hidden="true" /> 相关文件</h3>
              <button type="button" className="text-link-button">查看全部</button>
            </div>
            <div className="project-files">
              {relatedFiles.length > 0 ? relatedFiles.map((file) => (
                <button type="button" key={file.relativePath} onClick={() => openMarkdown(file.relativePath)}>
                  {fileIcon(file.relativePath)}
                  <span>{file.name}</span>
                  <small>最近更新</small>
                </button>
              )) : ["产品需求文档 PRD.md", "系统架构设计 V1.1.md", "接口规范 API.md", "项目计划 Roadmap.md"].map((file, index) => (
                <button type="button" key={file}>
                  <FileText size={16} aria-hidden="true" />
                  <span>{file}</span>
                  <small>{index === 0 ? "12 小时前" : `${index} 天前`}</small>
                </button>
              ))}
            </div>
            <button type="button" className="full-width ghost-button" onClick={() => setActiveView("inbox")}>
              <Plus size={16} aria-hidden="true" />
              添加文件
            </button>
          </article>

          <article className="concept-card project-chat">
            <div className="card-heading"><h3><Bot size={18} aria-hidden="true" /> 与 Agent 对话 / 下达指令</h3></div>
            {renderPromptBox("project")}
          </article>

          <article className="concept-card">
            <div className="card-heading">
              <h3><ListChecks size={18} aria-hidden="true" /> 下一步行动 / TODO</h3>
              <button type="button" className="text-link-button">添加任务</button>
            </div>
            <div className="project-todos">
              {["完善文档上传与解析流程", "实现文档权限与协作功能", "接入全文搜索与问答能力", "优化解析性能与稳定性"].map((item, index) => (
                <label key={item}>
                  <input type="checkbox" />
                  <span>{item}</span>
                  <small>{["今天", "3 天内", "5 天内", "1 周内"][index]}</small>
                </label>
              ))}
            </div>
          </article>
        </section>

        <section className="quick-actions">
          <span>快速操作：</span>
          <button type="button"><Pencil size={15} aria-hidden="true" /> 新建文档</button>
          <button type="button" onClick={() => setActiveView("inbox")}><Upload size={15} aria-hidden="true" /> 上传文件</button>
          <button type="button"><CheckCircle2 size={15} aria-hidden="true" /> 创建任务</button>
          <button type="button"><Link2 size={15} aria-hidden="true" /> 同步到 GitHub</button>
        </section>
      </div>
    );
  }

  function renderSettingsPage() {
    return (
      <div className="settings-page">
        <section className="settings-grid">
          <article className="settings-card">
            <h3><span>1</span> Vault 与本地存储</h3>
            <label>Vault 路径 <input value={vaultPath} onChange={(event) => setVaultPath(event.target.value)} placeholder="D:\\TheBrain\\Vault" /></label>
            <label>000-收集箱 <input value="000-收集箱" readOnly /></label>
            <div className="segmented">
              <button type="button" className={importBehavior === "copy" ? "active" : undefined} onClick={() => setImportBehavior("copy")}>保留原文件</button>
              <button type="button" className={importBehavior === "move" ? "active" : undefined} onClick={() => setImportBehavior("move")}>删除原文件</button>
            </div>
            <label>本地数据库位置 <input value=".thebrain/index.sqlite" readOnly /></label>
            <p>本地优先，所有数据先保存在本地。</p>
          </article>

          <article className="settings-card">
            <h3><span>2</span> AI 与 API Key</h3>
            <label>Provider <select value="MiMo / OpenAI 兼容" onChange={() => undefined}><option>MiMo / OpenAI 兼容</option></select></label>
            <label>API Key <input value={mimoStatus?.hasKey ? "已从安全位置读取" : "未配置"} readOnly aria-label="API Key 脱敏显示" /></label>
            <div className="connection-row">
              <Circle size={8} aria-hidden="true" />
              {mimoStatus?.hasKey ? `已就绪：${mimoStatus.keySource ?? "runtime"}` : "缺少 MIMO_API_KEY 或本地 .secrets key"}
              <button type="button" className="ghost-button" onClick={() => vaultPath && refreshOperations(vaultPath)}>刷新</button>
            </div>
            <label className="toggle-line"><input type="checkbox" checked readOnly /> 启用 AI 功能</label>
            <label>每月预算上限 <input type="number" value={settings.budgetMonthlyCents} onChange={(event) => setSettings((current) => ({ ...current, budgetMonthlyCents: Number(event.target.value) || 0 }))} /></label>
            <button type="button" className="ghost-button" onClick={saveBudget}>保存预算</button>
          </article>

          <article className="settings-card">
            <h3><span>3</span> 便利贴与快捷键</h3>
            <label>打开便利贴（全局） <input value="Ctrl + Space" readOnly /></label>
            <label>立即整理（全局） <input value="Ctrl + Enter" readOnly /></label>
            <label>便利贴自动保存 <select value={settings.autoSaveIntervalSeconds} onChange={(event) => setSettings((current) => ({ ...current, autoSaveIntervalSeconds: Number(event.target.value) || 3 }))}><option value={3}>输入停止 3 秒后</option><option value={20}>每 20 秒</option></select></label>
            <label className="toggle-line"><input type="checkbox" checked={settings.enableGlobalShortcut} onChange={(event) => setSettings((current) => ({ ...current, enableGlobalShortcut: event.target.checked }))} /> 窗口置顶</label>
            <label>新建便签位置 <select value="居中" onChange={() => undefined}><option>居中</option></select></label>
          </article>

          <article className="settings-card template-settings">
            <h3><span>4</span> 整理模板</h3>
            <p>选择默认整理模板，AI 将按所选模式进行整理。</p>
            <div className="template-card-grid">
              {["学生模式", "工作模式", "混合模式", "频率模式"].map((template) => (
                <button
                  type="button"
                  key={template}
                  className={settings.organizationTemplate === template ? "active" : undefined}
                  onClick={() => setSettings((current) => ({ ...current, organizationTemplate: template }))}
                >
                  <Wand2 size={26} aria-hidden="true" />
                  <strong>{template}</strong>
                  <span>{template === "学生模式" ? "适合学习与考试复习" : "保留扩展入口"}</span>
                </button>
              ))}
            </div>
            <label>默认模板 <select value={settings.organizationTemplate} onChange={(event) => setSettings((current) => ({ ...current, organizationTemplate: event.target.value }))}><option>学生模式</option><option>工作模式</option><option>混合模式</option><option>频率模式</option></select></label>
          </article>

          <article className="settings-card">
            <h3><span>5</span> Markdown 与编辑器</h3>
            <label>Frontmatter 处理 <select value="保留并更新" onChange={() => undefined}><option>保留并更新</option></select></label>
            <label>预览默认行为 <select value="右侧预览" onChange={() => undefined}><option>右侧预览</option></select></label>
            <label>字体大小 <input type="number" value={15} readOnly /></label>
            <label>行宽限制 <input type="number" value={880} readOnly /></label>
            <label>代码块主题 <select value="跟随系统" onChange={() => undefined}><option>跟随系统</option></select></label>
          </article>

          <article className="settings-card">
            <h3><span>6</span> 同步与备份</h3>
            <div className="local-first-note">TheBrain 采用本地优先策略，所有数据先保存在本地，可按需与云端或其他设备同步。</div>
            <label>上次同步时间 <input value="5 分钟前" readOnly /></label>
            <div className="connection-row"><Circle size={8} aria-hidden="true" /> 已同步 <button type="button" className="ghost-button">立即同步</button></div>
            <label>备份目录 <input value="D:\\TheBrain\\Backups" readOnly /></label>
            <label className="toggle-line"><input type="checkbox" checked readOnly /> 自动备份</label>
            <label>保留版本数 <select value={30} onChange={() => undefined}><option value={30}>30 个版本</option></select></label>
          </article>
        </section>
        <div className="settings-footer">
          <button type="button" className="ghost-button" onClick={() => setSettings(defaultAppSettings)}>取消</button>
          <button type="button" onClick={saveSettings}><Save size={16} aria-hidden="true" /> 保存设置</button>
        </div>
      </div>
    );
  }

  function renderDragOverlay() {
    if (!dragActive) return null;
    return (
      <div className="drag-overlay" role="dialog" aria-label="拖拽导入">
        <div className="drag-panel">
          <CloudUpload size={72} aria-hidden="true" />
          <h2>松开鼠标，导入到 000-收集箱</h2>
          <p>文件将默认复制到收集箱，随后由 AI 自动识别与整理</p>
          <div className="drag-choice-row">
            <label>
              <input type="radio" checked={importBehavior === "copy"} onChange={() => setImportBehavior("copy")} />
              复制原文件（默认）
            </label>
            <label>
              <input type="radio" checked={importBehavior === "move"} onChange={() => setImportBehavior("move")} />
              移动并删除原文件
            </label>
          </div>
          <div className="import-type-row overlay-types">
            {importTypes.map((type) => <span key={type.label}>{type.icon}{type.label}</span>)}
          </div>
          <div className="drop-hint"><RefreshCw size={15} aria-hidden="true" /> 你还可以拖入文本片段、截图、链接等内容</div>
        </div>
      </div>
    );
  }

  function renderActiveView() {
    if (activeView === "inbox") return renderInboxPage();
    if (activeView === "markdown") return renderMarkdownWorkspace();
    if (activeView === "notes") return renderStickyNotes();
    if (activeView === "personal") return renderPersonalPage();
    if (activeView === "project") return renderProjectWorkspace();
    if (activeView === "settings") return renderSettingsPage();
    return renderDashboard();
  }

  return (
    <main
      className="app-shell"
      onDragEnter={handleDragEnter}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {renderLeftSidebar()}
      <section className="workspace">
        {renderTopbar()}
        {appError ? (
          <section className="alert-panel" role="alert">
            <Shield size={18} aria-hidden="true" />
            <div>
              <strong>{appError.title}</strong>
              <span>{appError.detail}</span>
              <small>{appError.recovery}</small>
            </div>
          </section>
        ) : null}
        <div className="status-strip">
          <span>{vaultPath || "未选择 Vault"}</span>
          <span>{status}</span>
          <span>队列：{queueStateLabel(queueStatus?.state)} · 待处理 {queueStatus?.pending ?? 0}</span>
          <span>预算：{budgetStatus ? formatBudget(budgetStatus.remainingCents) : "未连接"}</span>
          <span>MiMo：{mimoStatus?.hasKey ? "ready" : "missing key"}</span>
          <span>RAG：{ragStatus ? `${ragStatus.documentCount} 文档 / ${ragStatus.chunkCount} chunks` : "未连接"}</span>
          {initResult ? <span>Index 已初始化</span> : null}
          {usage?.isFallback ? <span>Usage fallback</span> : null}
        </div>
        <div className="workspace-scroll">{renderActiveView()}</div>
      </section>
      {renderDragOverlay()}
    </main>
  );
}
