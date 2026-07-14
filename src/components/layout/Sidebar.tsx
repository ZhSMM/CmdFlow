import { NavLink } from "react-router-dom";
import { cn } from "@/lib/utils";
import {
  Library,
  Workflow,
  PlayCircle,
  History,
  CalendarClock,
  Settings,
  Terminal,
} from "lucide-react";

const NAV = [
  { to: "/library", label: "命令库", icon: Library },
  { to: "/workflows", label: "工作流", icon: Workflow },
  { to: "/runner", label: "执行", icon: PlayCircle },
  { to: "/history", label: "历史", icon: History },
  { to: "/schedules", label: "调度", icon: CalendarClock },
  { to: "/settings", label: "设置", icon: Settings },
] as const;

export function Sidebar() {
  return (
    <aside className="flex w-48 flex-col border-r bg-card">
      <div className="flex h-12 items-center gap-2 border-b px-4">
        <Terminal className="h-5 w-5 text-primary" />
        <span className="font-semibold">CmdFlow</span>
      </div>
      <nav className="flex-1 space-y-0.5 p-2">
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
      </nav>
      <div className="border-t p-3 text-xs text-muted-foreground">
        v0.1.0 · 本地命令编排
      </div>
    </aside>
  );
}
