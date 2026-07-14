import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "react-router-dom";
import { router } from "./router";
import { useTheme, applyTheme } from "./stores/theme";
import { PalettePage } from "./pages/PalettePage";
import "./styles/globals.css";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 1000 * 30,
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

// 检测 URL: ?window=palette 表示这是独立启动器窗口
function isPaletteWindow(): boolean {
  const params = new URLSearchParams(window.location.search);
  return params.get("window") === "palette";
}

function ThemedShell({ children }: { children: React.ReactNode }) {
  const theme = useTheme((s) => s.theme);
  React.useEffect(() => {
    applyTheme(theme);
  }, [theme]);
  React.useEffect(() => {
    if (theme !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => applyTheme("system");
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, [theme]);
  return <>{children}</>;
}

function App() {
  if (isPaletteWindow()) {
    // 独立启动器窗口: 不挂主路由,直接渲染 palette
    return (
      <ThemedShell>
        <PalettePage />
      </ThemedShell>
    );
  }
  return (
    <ThemedShell>
      <RouterProvider router={router} />
    </ThemedShell>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>
);
