import { NavLink, useNavigate } from "react-router-dom";
import { useQuery, useQueryClient, useMutation } from "@tanstack/react-query";
import {
  Library,
  Workflow,
  PlayCircle,
  History,
  CalendarClock,
  Settings,
  Terminal,
  Star,
  Puzzle,
  Search,
  X,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { api, type Favorite, TauriError } from "@/lib/tauri";
import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";

const NAV = [
  { to: "/library", label: "命令库", icon: Library },
  { to: "/workflows", label: "工作流", icon: Workflow },
  { to: "/runner", label: "执行", icon: PlayCircle },
  { to: "/history", label: "历史", icon: History },
  { to: "/schedules", label: "调度", icon: CalendarClock },
  { to: "/plugins", label: "插件", icon: Puzzle },
  { to: "/settings", label: "设置", icon: Settings },
] as const;

export function Sidebar() {
  const qc = useQueryClient();
  const nav = useNavigate();

  const favorites = useQuery({
    queryKey: ["favorites"],
    queryFn: () => api.favorite.list(),
  });

  // 监听启动器事件 (快捷键触发)
  useEffect(() => {
    const un = listen("palette-toggle", () => {
      const el = document.getElementById("palette-trigger");
      el?.focus();
      el?.click();
    });
    return () => {
      un.then((u) => u());
    };
  }, []);

  const removeFav = useMutation({
    mutationFn: (id: string) => api.favorite.remove(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["favorites"] }),
  });

  return (
    <aside className="flex w-56 flex-col border-r bg-card">
      <div className="flex h-12 items-center gap-2 border-b px-4">
        <Terminal className="h-5 w-5 text-primary" />
        <span className="font-semibold">CmdFlow</span>
      </div>

      {/* 启动器触发按钮 (无快捷键时 fallback) */}
      <div className="border-b p-2">
        <button
          onClick={() => {
            // 通过 DOM 自定义事件通知 MainLayout 弹启动器
            window.dispatchEvent(new CustomEvent("open-palette"));
          }}
          className="flex w-full items-center gap-2 rounded-md border bg-background px-2.5 py-1.5 text-left text-xs text-muted-foreground hover:bg-accent/30"
        >
          <Search className="h-3.5 w-3.5" />
          <span>搜命令...</span>
          <kbd className="ml-auto rounded bg-secondary px-1 py-0.5 font-mono text-[9px]">
            ⌘⇧Space
          </kbd>
        </button>
      </div>

      <nav className="border-b p-2">
        <div className="mb-1 px-1 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          主导航
        </div>
        <div className="space-y-0.5">
          {NAV.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              className={({ isActive }) =>
                cn(
                  "flex items-center gap-3 rounded-md px-3 py-2 text-sm transition-colors",
                  isActive
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
                )
              }
            >
              <item.icon className="h-4 w-4" />
              {item.label}
            </NavLink>
          ))}
        </div>
      </nav>

      {/* 收藏区 */}
      <div className="flex-1 overflow-auto p-2">
        <div className="mb-1 flex items-center justify-between px-1 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          <span>收藏</span>
          <span className="rounded bg-secondary px-1.5 text-[9px]">
            {favorites.data?.length ?? 0}
          </span>
        </div>
        {favorites.isLoading && (
          <div className="px-2 text-xs text-muted-foreground">加载中...</div>
        )}
        {favorites.error && (
          <div className="px-2 text-xs text-destructive">
            {(favorites.error as TauriError).message}
          </div>
        )}
        {favorites.data && favorites.data.length === 0 && (
          <div className="px-2 py-1 text-xs text-muted-foreground">
            命令库点 ★ 收藏常用命令
          </div>
        )}
        {favorites.data && favorites.data.length > 0 && (
          <div className="space-y-0.5">
            {favorites.data.map((f: Favorite) => (
              <div
                key={f.command_id}
                className="group flex items-center gap-2 rounded-md px-2 py-1.5 text-xs hover:bg-accent/50"
              >
                <span
                  className="h-3 w-3 shrink-0 cursor-pointer text-amber-500"
                  onClick={() => removeFav.mutate(f.command_id)}
                  title="移除收藏"
                >
                  <Star className="h-3 w-3 fill-amber-400" />
                </span>
                <span
                  className="flex-1 cursor-pointer truncate"
                  onClick={() => nav(`/runner?cmd=${f.command_id}`)}
                >
                  {f.command?.name ?? f.command_id.slice(0, 8)}
                </span>
                <button
                  onClick={() => removeFav.mutate(f.command_id)}
                  className="opacity-0 group-hover:opacity-100 text-muted-foreground hover:text-destructive"
                  title="移除"
                >
                  <X className="h-3 w-3" />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="border-t p-3 text-xs text-muted-foreground">
        v0.1.0 · 本地命令编排
      </div>
    </aside>
  );
}
