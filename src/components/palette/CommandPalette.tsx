import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useQuery } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Search, Play, Star, Hash, FolderTree, X } from "lucide-react";
import { api, type Command, type Favorite } from "@/lib/tauri";
import { useNavigate } from "react-router-dom";
import { cn } from "@/lib/utils";

interface PaletteItem {
  kind: "favorite" | "command" | "workflow";
  id: string;
  label: string;
  hint?: string;
  data?: any;
  score: number;
}

interface CommandPaletteProps {
  /**
   * 渲染模式:
   * - popup:  在父页面盖一层黑色遮罩,主窗口内弹窗
   * - standalone: 不带遮罩,用于独立窗口 (Tauri palette window)
   */
  mode?: "popup" | "standalone";
  /** 关闭时回调 (standalone 模式下一般是 getCurrentWindow().close()) */
  onClose?: () => void;
}

export function CommandPalette({ mode = "popup", onClose }: CommandPaletteProps) {
  const navigate = useNavigate();
  const [open, setOpen] = useState(mode === "standalone");
  const [query, setQuery] = useState("");
  const [activeIdx, setActiveIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // popup 模式: 监听全局快捷键 + DOM 事件
  useEffect(() => {
    if (mode === "standalone") return;
    const unTauri = listen("palette-toggle", () => {
      setOpen((o) => !o);
    });
    const onDom = () => setOpen((o) => !o);
    window.addEventListener("open-palette", onDom);
    return () => {
      unTauri.then((u) => u());
      window.removeEventListener("open-palette", onDom);
    };
  }, [mode]);

  // 打开时聚焦输入框
  useEffect(() => {
    if (open) {
      setQuery("");
      setActiveIdx(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  // 关闭自身 (standalone 模式)
  const closeSelf = async () => {
    if (onClose) {
      onClose();
    } else if (mode === "standalone") {
      try {
        await getCurrentWindow().close();
      } catch (e) {
        console.error("close palette window failed:", e);
      }
    } else {
      setOpen(false);
    }
  };

  const commands = useQuery({
    queryKey: ["commands-palette", mode],
    queryFn: () => api.library.list(),
    enabled: open || mode === "standalone",
  });
  const favorites = useQuery({
    queryKey: ["favorites-palette", mode],
    queryFn: () => api.favorite.list(),
    enabled: open || mode === "standalone",
  });
  const workflows = useQuery({
    queryKey: ["workflows-palette", mode],
    queryFn: () => api.workflow.list(),
    enabled: open || mode === "standalone",
  });

  // 构造候选项
  const items: PaletteItem[] = useMemo(() => {
    if (!open && mode !== "standalone") return [];
    const result: PaletteItem[] = [];

    // 收藏优先
    (favorites.data ?? []).forEach((f: Favorite) => {
      if (!f.command) return;
      result.push({
        kind: "favorite",
        id: f.command_id,
        label: f.command.name,
        hint: f.command.description ?? f.command.type,
        data: f.command,
        score: scoreMatch(f.command.name, query) + 100, // 收藏加权
      });
    });

    // 命令
    (commands.data ?? []).forEach((c: Command) => {
      result.push({
        kind: "command",
        id: c.id,
        label: c.name,
        hint: c.description ?? c.type,
        data: c,
        score: scoreMatch(c.name, query),
      });
    });

    // 工作流
    (workflows.data ?? []).forEach((w) => {
      result.push({
        kind: "workflow",
        id: w.id,
        label: w.name,
        hint: w.description ?? w.trigger_type,
        data: w,
        score: scoreMatch(w.name, query),
      });
    });

    return result
      .filter((it) => query.length === 0 || it.score > 0)
      .sort((a, b) => b.score - a.score)
      .slice(0, 20);
  }, [open, mode, commands.data, favorites.data, workflows.data, query]);

  // 选中执行
  const execute = (item: PaletteItem) => {
    if (mode === "standalone") {
      // 独立窗口模式: 给主窗口发事件 + 关闭自己
      // 1) 计算目标 URL
      let target = "/library";
      if (item.kind === "command" || item.kind === "favorite") {
        target = `/runner?cmd=${encodeURIComponent(item.id)}`;
      } else if (item.kind === "workflow") {
        target = `/workflows/${item.id}`;
      }
      // 2) 通过 Tauri 事件通知主窗口 + 自定义事件,主窗口监听 navigate
      import("@tauri-apps/api/event").then(({ emit }) => {
        emit("palette-navigate", { target });
        setTimeout(async () => {
          try {
            await getCurrentWindow().close();
          } catch (e) {
            console.error(e);
          }
        }, 80);
      });
    } else {
      // popup 模式: 直接 navigate
      setOpen(false);
      if (item.kind === "command" || item.kind === "favorite") {
        navigate(`/runner?cmd=${encodeURIComponent(item.id)}`);
      } else if (item.kind === "workflow") {
        navigate(`/workflows/${item.id}`);
      }
    }
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      closeSelf();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIdx((i) => Math.min(items.length - 1, i + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIdx((i) => Math.max(0, i - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const item = items[activeIdx];
      if (item) execute(item);
    } else if (e.key === " " && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      closeSelf();
    }
  };

  // popup 模式 + 关闭 → 不渲染
  if (mode === "popup" && !open) return null;

  const containerClass =
    mode === "standalone"
      ? "flex h-screen w-screen flex-col bg-card text-card-foreground"
      : "fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-24";

  const innerClass =
    mode === "standalone"
      ? "flex h-full w-full flex-col"
      : "w-full max-w-2xl overflow-hidden rounded-lg border bg-card shadow-2xl";

  return (
    <div
      className={containerClass}
      onClick={(e) => {
        // standalone: 整个窗口就是 palette,点击空白不关;只能 Esc 关
        // popup: 点击背景关
        if (mode === "popup") {
          if (e.target === e.currentTarget) setOpen(false);
        }
      }}
    >
      <div className={innerClass}>
        <div className="flex items-center gap-2 border-b px-4 py-3">
          <Search className="h-4 w-4 text-muted-foreground" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setActiveIdx(0);
            }}
            onKeyDown={onKey}
            placeholder="搜命令/工作流..."
            className="flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
          />
          {mode === "standalone" ? (
            <button
              onClick={closeSelf}
              className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
              title="关闭 (Esc)"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          ) : (
            <kbd className="rounded bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
              ESC
            </kbd>
          )}
        </div>
        <div className="flex-1 overflow-y-auto p-1">
          {items.length === 0 && (
            <div className="p-8 text-center text-sm text-muted-foreground">
              {query ? "没找到匹配项" : "输入关键词搜索..."}
            </div>
          )}
          {items.map((it, i) => (
            <button
              key={`${it.kind}-${it.id}`}
              onClick={() => execute(it)}
              onMouseEnter={() => setActiveIdx(i)}
              className={cn(
                "flex w-full items-center gap-3 rounded-md px-3 py-2 text-left text-sm",
                activeIdx === i ? "bg-accent" : "hover:bg-accent/50",
              )}
            >
              <ItemIcon kind={it.kind} />
              <div className="flex-1 truncate">
                <div className="truncate font-medium">{it.label}</div>
                {it.hint && (
                  <div className="truncate text-xs text-muted-foreground">
                    {it.hint}
                  </div>
                )}
              </div>
              {activeIdx === i && (
                <Play className="h-3.5 w-3.5 text-primary" />
              )}
            </button>
          ))}
        </div>
        <div className="border-t bg-muted/30 px-4 py-1.5 text-[10px] text-muted-foreground">
          ↑↓ 移动 · Enter 执行 · ESC 关闭 · {items.length} 项
        </div>
      </div>
    </div>
  );
}

function ItemIcon({ kind }: { kind: PaletteItem["kind"] }) {
  if (kind === "favorite")
    return <Star className="h-3.5 w-3.5 fill-amber-400 text-amber-500" />;
  if (kind === "workflow")
    return <FolderTree className="h-3.5 w-3.5 text-purple-500" />;
  return <Hash className="h-3.5 w-3.5 text-blue-500" />;
}

/// 简单模糊匹配: 子序列 + 前缀加权
function scoreMatch(text: string, query: string): number {
  if (!query) return 1; // 无查询: 都返回, 排序按原顺序
  const t = text.toLowerCase();
  const q = query.toLowerCase();
  if (t.startsWith(q)) return 100;
  if (t.includes(q)) return 50;
  // 子序列匹配
  let i = 0;
  for (const c of t) {
    if (c === q[i]) i++;
    if (i === q.length) return 10;
  }
  return i === q.length ? 1 : 0;
}
