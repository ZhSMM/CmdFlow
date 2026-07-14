import { useEffect } from "react";
import { Outlet, useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { Sidebar } from "./Sidebar";
import { StatusBar } from "./StatusBar";
import { CommandPalette } from "@/components/palette/CommandPalette";

export function MainLayout() {
  const nav = useNavigate();

  // 监听独立启动器窗口的导航事件
  useEffect(() => {
    const un = listen<{ target: string }>("palette-navigate", (e) => {
      const target = e.payload?.target;
      if (target) nav(target);
    });
    return () => {
      un.then((u) => u());
    };
  }, [nav]);

  return (
    <div className="flex h-screen w-screen bg-background text-foreground">
      <Sidebar />
      <div className="flex flex-1 flex-col overflow-hidden">
        <main className="flex-1 overflow-auto">
          <Outlet />
        </main>
        <StatusBar />
      </div>
      {/* popup 模式仍保留作为 fallback (Phase 9.4 主要走独立窗口) */}
      <CommandPalette mode="popup" />
    </div>
  );
}
