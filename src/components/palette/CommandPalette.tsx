import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useQuery } from "@tanstack/react-query";
import { Search, Play, Star, Hash, FolderTree } from "lucide-react";
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

export function CommandPalette() {
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIdx, setActiveIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // 监听全局快捷键触发 + DOM 事件
  useEffect(() => {
    const unTauri = listen("palette-toggle", () => {
      setOpen((o) => !o);
    });
    const onDom = () => setOpen((o) => !o);
    window.addEventListener("open-palette", onDom);
    return () => {
      unTauri.then((u) => u());
      window.removeEventListener("open-palette", onDom);
    };
  }, []);

  // 打开时聚焦输入框
  useEffect(() => {
    if (open) {
      setQuery("");
      setActiveIdx(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  const commands = useQuery({
    queryKey: ["commands-palette"],
    queryFn: () => api.library.list(),
    enabled: open,
  });
  const favorites = useQuery({
    queryKey: ["favorites-palette"],
    queryFn: () => api.favorite.list(),
    enabled: open,
  });
  const workflows = useQuery({
    queryKey: ["workflows-palette"],
    queryFn: () => api.workflow.list(),
    enabled: open,
  });

  // 构造候选项
  const items: PaletteItem[] = useMemo(() => {
    if (!open) return [];
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
  }, [open, commands.data, favorites.data, workflows.data, query]);

  // 选中执行
  const execute = (item: PaletteItem) => {
    setOpen(false);
    if (item.kind === "command" || item.kind === "favorite") {
      navigate(`/runner?cmd=${encodeURIComponent(item.id)}`);
    } else if (item.kind === "workflow") {
      // 工作流直接进编辑/执行
      navigate(`/workflows/${item.id}`);
    }
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      setOpen(false);
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
      setOpen(false);
    }
  };

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-24"
      onClick={() => setOpen(false)}
    >
      <div
        className="w-full max-w-2xl overflow-hidden rounded-lg border bg-card shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
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
            placeholder="搜命令/工作流 (Cmd+Shift+Space)"
            className="flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
          />
          <kbd className="rounded bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
            ESC
          </kbd>
        </div>
        <div className="max-h-96 overflow-y-auto p-1">
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
