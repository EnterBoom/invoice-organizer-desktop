import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

type OrganizerSettings = {
  sourceDir?: string | null;
  targetDir?: string | null;
  manualDate?: string | null;
  projectName?: string | null;
  ownerName?: string | null;
  groupByMonth: boolean;
  includeTypeFolder: boolean;
  preserveOriginals: boolean;
};

type PlanItem = {
  sourcePath: string;
  sourceName: string;
  proposedName: string;
  targetPath: string;
  relativeOutputPath: string;
  manualDate: string;
  projectName: string;
  ownerName: string;
  invoiceMonth?: string | null;
  issuer?: string | null;
  amount?: string | null;
  invoiceType?: string | null;
  warnings: string[];
};

type PreviewSummary = {
  totalFiles: number;
  supportedFiles: number;
  plannedFiles: number;
  ignoredFiles: number;
  warningFiles: number;
};

type PreviewResponse = {
  resolvedTargetDir: string;
  summary: PreviewSummary;
  items: PlanItem[];
};

type ExecuteResponse = {
  writtenCount: number;
  skippedCount: number;
  errorCount: number;
  errors: string[];
};

const defaultSettings: OrganizerSettings = {
  sourceDir: "",
  targetDir: "",
  manualDate: "",
  projectName: "",
  ownerName: "",
  groupByMonth: true,
  includeTypeFolder: true,
  preserveOriginals: true,
};

const isTauriRuntime = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function valueOrFallback(value?: string | null, fallback = "待识别"): string {
  if (!value || value.trim().length === 0) {
    return fallback;
  }
  return value;
}

function isNonEmptyText(value?: string | null): value is string {
  return Boolean(value && value.trim().length > 0);
}

async function pickDirectory(): Promise<string | null> {
  if (!isTauriRuntime) {
    return null;
  }

  const selection = await open({
    directory: true,
    multiple: false,
    title: "选择文件夹",
  });

  if (typeof selection === "string") {
    return selection;
  }

  return null;
}

function Badge({ label, tone = "default" }: { label: string; tone?: "default" | "warning" | "accent" }) {
  return <span className={`badge badge--${tone}`}>{label}</span>;
}

export default function App() {
  const [sourceDir, setSourceDir] = useState(defaultSettings.sourceDir ?? "");
  const [targetDir, setTargetDir] = useState(defaultSettings.targetDir ?? "");
  const [manualDate, setManualDate] = useState(defaultSettings.manualDate ?? "");
  const [projectName, setProjectName] = useState(defaultSettings.projectName ?? "");
  const [ownerName, setOwnerName] = useState(defaultSettings.ownerName ?? "");
  const [groupByMonth, setGroupByMonth] = useState(defaultSettings.groupByMonth);
  const [includeTypeFolder, setIncludeTypeFolder] = useState(defaultSettings.includeTypeFolder);
  const [preserveOriginals, setPreserveOriginals] = useState(defaultSettings.preserveOriginals);
  const [preview, setPreview] = useState<PreviewResponse | null>(null);
  const [booting, setBooting] = useState(true);
  const [loadingPreview, setLoadingPreview] = useState(false);
  const [executing, setExecuting] = useState(false);
  const [message, setMessage] = useState("先填写归档日期、项目归属和归属人，再选择发票文件夹生成预览。");
  const [hydrated, setHydrated] = useState(false);

  useEffect(() => {
    const bootstrap = async () => {
      if (!isTauriRuntime) {
        setBooting(false);
        setHydrated(false);
        setMessage("当前是浏览器预览模式。界面可以查看，但文件夹选择、扫描和整理功能需要在 Tauri 桌面窗口里运行。");
        return;
      }

      try {
        const saved = await invoke<OrganizerSettings | null>("load_organizer_settings");
        if (saved) {
          setSourceDir(saved.sourceDir ?? "");
          setTargetDir(saved.targetDir ?? "");
          setManualDate(saved.manualDate ?? "");
          setProjectName(saved.projectName ?? "");
          setOwnerName(saved.ownerName ?? "");
          setGroupByMonth(saved.groupByMonth);
          setIncludeTypeFolder(saved.includeTypeFolder);
          setPreserveOriginals(saved.preserveOriginals);
        }
      } catch (error) {
        console.error(error);
      } finally {
        setBooting(false);
        setHydrated(true);
      }
    };

    void bootstrap();
  }, []);

  useEffect(() => {
    if (!hydrated) {
      return;
    }

    const timer = window.setTimeout(() => {
      void invoke("save_organizer_settings", {
        settings: {
          sourceDir,
          targetDir,
          manualDate,
          projectName,
          ownerName,
          groupByMonth,
          includeTypeFolder,
          preserveOriginals,
        },
      }).catch((error) => console.error(error));
    }, 200);

    return () => window.clearTimeout(timer);
  }, [sourceDir, targetDir, manualDate, projectName, ownerName, groupByMonth, includeTypeFolder, preserveOriginals, hydrated]);

  const handleChooseSource = async () => {
    if (!isTauriRuntime) {
      setMessage("浏览器预览模式下不能直接打开系统目录选择器，请在桌面窗口里运行这个工具。");
      return;
    }

    const picked = await pickDirectory();
    if (picked) {
      setSourceDir(picked);
      setPreview(null);
      setMessage("已选择原始发票文件夹，接下来可以生成预览。");
    }
  };

  const handleChooseTarget = async () => {
    if (!isTauriRuntime) {
      setMessage("浏览器预览模式下不能直接选择输出目录，请在桌面窗口里运行这个工具。");
      return;
    }

    const picked = await pickDirectory();
    if (picked) {
      setTargetDir(picked);
      setPreview(null);
      setMessage("已更新输出文件夹，预览结果会按新的目标路径重新生成。");
    }
  };

  const handlePreview = async () => {
    if (!isTauriRuntime) {
      setMessage("当前页面只是浏览器预览。要真的扫描发票文件，请打开 Tauri 桌面应用。");
      return;
    }

    if (!isNonEmptyText(sourceDir)) {
      setMessage("先选择要扫描的发票文件夹。");
      return;
    }

    if (!isNonEmptyText(manualDate)) {
      setMessage("先手动填写这批发票要使用的日期。");
      return;
    }

    if (!isNonEmptyText(projectName)) {
      setMessage("先手动填写这批发票的项目归属。");
      return;
    }

    if (!isNonEmptyText(ownerName)) {
      setMessage("先手动填写这批发票的归属人。");
      return;
    }

    setLoadingPreview(true);
    setMessage("正在扫描文件并生成重命名预览...");

    try {
      const result = await invoke<PreviewResponse>("preview_invoice_plan", {
        request: {
          sourceDir,
          targetDir: targetDir.trim() || null,
          manualDate,
          projectName,
          ownerName,
          groupByMonth,
          includeTypeFolder,
        },
      });

      setPreview(result);
      setMessage(`已生成 ${result.summary.plannedFiles} 条整理方案；金额和开票主体已识别，项目归属与归属人已套用。`);
    } catch (error) {
      setPreview(null);
      setMessage(error instanceof Error ? error.message : "生成预览失败");
    } finally {
      setLoadingPreview(false);
    }
  };

  const handleExecute = async () => {
    if (!isTauriRuntime) {
      setMessage("浏览器预览模式不能执行文件整理。这个动作需要桌面应用里的本地文件权限。");
      return;
    }

    if (!preview || preview.items.length === 0) {
      setMessage("没有可执行的整理方案，请先生成预览。");
      return;
    }

    setExecuting(true);
    setMessage(preserveOriginals ? "正在复制并整理发票..." : "正在移动并整理发票...");

    try {
      const result = await invoke<ExecuteResponse>("execute_invoice_plan", {
        request: {
          items: preview.items,
          preserveOriginals,
        },
      });

      if (result.errorCount > 0) {
        setMessage(`已写入 ${result.writtenCount} 个文件，另有 ${result.errorCount} 个文件失败，请先查看提示后再重试。`);
      } else {
        setMessage(
          preserveOriginals
            ? `已完成复制整理，共写入 ${result.writtenCount} 个文件。原始文件仍保留在源目录。`
            : `已完成移动整理，共归档 ${result.writtenCount} 个文件。`,
        );
      }

      const refreshed = await invoke<PreviewResponse>("preview_invoice_plan", {
        request: {
          sourceDir,
          targetDir: targetDir.trim() || null,
          manualDate,
          projectName,
          ownerName,
          groupByMonth,
          includeTypeFolder,
        },
      });
      setPreview(refreshed);
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "执行整理失败");
    } finally {
      setExecuting(false);
    }
  };

  const handleRevealTarget = async () => {
    if (!isTauriRuntime) {
      setMessage("浏览器预览模式不能直接打开本地目录，请在桌面窗口中使用这个按钮。");
      return;
    }

    const resolvedPath = preview?.resolvedTargetDir || targetDir || sourceDir;
    if (!isNonEmptyText(resolvedPath)) {
      setMessage("还没有可打开的目录。");
      return;
    }

    try {
      await invoke("reveal_in_finder", { path: resolvedPath });
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "打开目录失败");
    }
  };

  const warningCount = preview?.summary.warningFiles ?? 0;

  if (booting) {
    return (
      <main className="loading-shell">
        <section className="loading-panel">
          <p className="eyebrow">Invoice Flow</p>
          <h1>发票整理器正在准备中</h1>
          <p className="subcopy">加载上次的文件夹和整理规则...</p>
        </section>
      </main>
    );
  }

  return (
    <main className="app-shell">
      <section className="hero-panel">
        <div className="hero-copy">
          <p className="eyebrow">Invoice Flow</p>
          <h1>把散落的发票文件，整理成一套能直接报销归档的命名系统。</h1>
          <p className="subcopy">
            这版工具会把归档日期、项目归属和归属人交给你手动确认，再从文件名里识别金额和开票主体。你确认预览后，它才会复制或移动到整理目录。
          </p>
          <div className="hero-actions">
            <button type="button" className="primary-button" onClick={() => void handlePreview()} disabled={loadingPreview}>
              {loadingPreview ? "扫描中..." : "生成预览"}
            </button>
            <button type="button" className="ghost-button" onClick={() => void handleRevealTarget()}>
              打开整理目录
            </button>
          </div>
        </div>

        <div className="hero-card">
          <span>当前模式</span>
          <strong>{isTauriRuntime ? (preserveOriginals ? "复制整理" : "移动归档") : "浏览器预览"}</strong>
          <p>
            {isTauriRuntime
              ? preserveOriginals
                ? "适合第一次跑数据，原文件会保留在源目录。"
                : "适合确认规则后直接归档，源目录里的文件会被移动。"
              : "当前页面用于看界面和流程，真正的目录读取与文件整理需要在桌面应用里执行。"}
          </p>
          <div className="hero-card__badges">
            <Badge label={isTauriRuntime ? "日期手填" : "仅预览界面"} tone="accent" />
            <Badge label={isTauriRuntime ? "项目与归属人手填" : "无本地文件权限"} />
            {isTauriRuntime ? <Badge label="金额与开票主体识别" /> : null}
          </div>
        </div>
      </section>

      <section className="stats-strip">
        <article className="stat-card">
          <span>识别方案</span>
          <strong>{preview?.summary.plannedFiles ?? 0}</strong>
        </article>
        <article className="stat-card">
          <span>待检查项目</span>
          <strong>{warningCount}</strong>
        </article>
        <article className="stat-card">
          <span>忽略文件</span>
          <strong>{preview?.summary.ignoredFiles ?? 0}</strong>
        </article>
        <article className="stat-card stat-card--status">
          <span>执行安全性</span>
          <strong>{isTauriRuntime ? (preserveOriginals ? "高" : "需要确认") : "预览中"}</strong>
        </article>
      </section>

      <section className="workspace-grid">
        <aside className="control-panel">
          <div className="panel-header">
            <div>
              <span>整理输入</span>
              <h2>先告诉工具要处理哪一批发票</h2>
            </div>
          </div>

          <label className="field">
            <span>原始发票文件夹</span>
            <div className="path-row">
              <input value={sourceDir} readOnly placeholder="选择下载发票或截图存放的目录" />
              <button
                type="button"
                className="secondary-button"
                onClick={() => void handleChooseSource()}
              >
                选择
              </button>
            </div>
          </label>

          <label className="field">
            <span>输出文件夹</span>
            <div className="path-row">
              <input
                value={targetDir}
                readOnly
                placeholder="可留空，默认会在原目录下新建“已整理发票”"
              />
              <button
                type="button"
                className="secondary-button"
                onClick={() => void handleChooseTarget()}
              >
                选择
              </button>
            </div>
          </label>

          <div className="field">
            <span>归档日期</span>
            <input
              className="text-input"
              type="date"
              value={manualDate}
              onChange={(event) => setManualDate(event.target.value)}
            />
            <p className="field-note">这个日期会用于当前整批发票的月份归档，不再直接放进文件名。</p>
          </div>

          <div className="field">
            <span>项目归属</span>
            <input
              className="text-input"
              value={projectName}
              onChange={(event) => setProjectName(event.target.value)}
              placeholder="例如：青岛项目 / 行政部 / 市场投放"
            />
            <p className="field-note">可填写项目名、部门名或你内部用于归档的归属标签。</p>
          </div>

          <div className="field">
            <span>归属人</span>
            <input
              className="text-input"
              value={ownerName}
              onChange={(event) => setOwnerName(event.target.value)}
              placeholder="例如：世宇"
            />
            <p className="field-note">这个字段会直接进入文件名，适合写负责人、报销人或归档责任人。</p>
          </div>

          <div className="field">
            <span>归档结构</span>
            <div className="toggle-grid">
              <button
                type="button"
                className={groupByMonth ? "toggle-chip toggle-chip--active" : "toggle-chip"}
                onClick={() => setGroupByMonth((current) => !current)}
              >
                {groupByMonth ? "按月份归档" : "单层输出"}
              </button>
              <button
                type="button"
                className={includeTypeFolder ? "toggle-chip toggle-chip--active" : "toggle-chip"}
                onClick={() => setIncludeTypeFolder((current) => !current)}
              >
                {includeTypeFolder ? "按票种建目录" : "不拆票种"}
              </button>
            </div>
          </div>

          <div className="field">
            <span>执行方式</span>
            <div className="mode-switch">
              <button
                type="button"
                className={preserveOriginals ? "mode-switch__button mode-switch__button--active" : "mode-switch__button"}
                onClick={() => setPreserveOriginals(true)}
              >
                复制整理
              </button>
              <button
                type="button"
                className={!preserveOriginals ? "mode-switch__button mode-switch__button--active" : "mode-switch__button"}
                onClick={() => setPreserveOriginals(false)}
              >
                直接移动
              </button>
            </div>
          </div>

          <div className="note-card">
            <strong>当前命名格式</strong>
            <p>`金额-开票主体-项目归属-归属人.扩展名`</p>
            <p>例：`188.00-知合-跃迁交付-世宇.pdf`</p>
          </div>

          <div className="note-card note-card--muted">
            <strong>这版适合什么场景</strong>
            <p>适合你已经知道这批发票归哪个项目、哪个人，但又不想手动一个个抄金额和开票主体的场景。</p>
            <p>如果后面你想继续补 OCR 或逐张手工校对模式，我们可以在这个基础上继续升级。</p>
          </div>

          <button
            type="button"
            className="primary-button"
            onClick={() => void handlePreview()}
            disabled={loadingPreview}
          >
            {loadingPreview ? "重新扫描中..." : "刷新预览"}
          </button>

          <button
            type="button"
            className="danger-button"
            onClick={() => void handleExecute()}
            disabled={executing || (isTauriRuntime && (!preview || preview.items.length === 0))}
          >
            {executing ? "执行中..." : preserveOriginals ? "开始复制整理" : "开始移动归档"}
          </button>

          <p className="status-text">{message}</p>
        </aside>

        <section className="results-panel">
          <div className="panel-header panel-header--wide">
            <div>
              <span>整理结果</span>
              <h2>先看预览，再动文件</h2>
            </div>
            <div className="panel-hints">
              <Badge label="预览不会修改文件" tone="accent" />
              <Badge label={preview?.resolvedTargetDir ? "已解析输出目录" : "等待预览"} />
            </div>
          </div>

          <div className="target-card">
            <span>实际输出目录</span>
            <strong>{preview?.resolvedTargetDir || "尚未生成预览"}</strong>
          </div>

          {preview ? (
            preview.items.length > 0 ? (
              <div className="table-shell">
                <table>
                  <thead>
                    <tr>
                      <th>原文件</th>
                      <th>命名来源</th>
                      <th>新文件名</th>
                      <th>输出位置</th>
                      <th>提示</th>
                    </tr>
                  </thead>
                  <tbody>
                    {preview.items.map((item) => (
                      <tr key={`${item.sourcePath}-${item.targetPath}`}>
                        <td>
                          <div className="table-main">{item.sourceName}</div>
                          <div className="table-sub">{item.sourcePath}</div>
                        </td>
                        <td>
                          <div className="meta-stack">
                            <span>{`归档日期：${item.manualDate}`}</span>
                            <span>{`项目归属：${item.projectName}`}</span>
                            <span>{`归属人：${item.ownerName}`}</span>
                            <span>{`开票主体：${valueOrFallback(item.issuer, "待识别")}`}</span>
                            <span>{`金额：${valueOrFallback(item.amount, "待识别")}`}</span>
                          </div>
                        </td>
                        <td>
                          <div className="table-main">{item.proposedName}</div>
                        </td>
                        <td>
                          <div className="table-main">{item.relativeOutputPath}</div>
                          <div className="table-sub">{item.targetPath}</div>
                        </td>
                        <td>
                          <div className="warning-stack">
                            {item.warnings.length === 0 ? (
                              <Badge label="可直接整理" tone="accent" />
                            ) : (
                              item.warnings.map((warning) => <Badge key={warning} label={warning} tone="warning" />)
                            )}
                          </div>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <div className="empty-panel">
                <strong>没有找到可整理的发票文件</strong>
                <p>支持的格式包括 PDF、OFD、JPG、PNG、WEBP、HEIC。也会自动跳过目标整理目录本身。</p>
              </div>
            )
          ) : (
            <div className="empty-panel">
              <strong>还没有生成预览</strong>
              <p>先选择文件夹，再点击“生成预览”。这一步只做扫描和命名推演，不会改动任何文件。</p>
            </div>
          )}
        </section>
      </section>
    </main>
  );
}
