import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/tauri";
import { Circle } from "lucide-react";

export function StatusBar() {
  const { data, isLoading, error } = useQuery({
    queryKey: ["app-info"],
    queryFn: () => api.system.getAppInfo(),
    refetchInterval: 5000,
  });

  return (
    <footer className="flex h-7 items-center justify-between border-t bg-card px-3 text-xs text-muted-foreground">
      <div className="flex items-center gap-2">
        <Circle className="h-2 w-2 fill-green-500 text-green-500" />
        <span>就绪</span>
      </div>
      <div className="flex items-center gap-4">
        {isLoading && <span>连接中...</span>}
        {error && <span className="text-destructive">后端连接失败</span>}
        {data && (
          <>
            <span>
              {data.name} v{data.version}
            </span>
            <span>运行 {data.uptime_seconds}s</span>
          </>
        )}
      </div>
    </footer>
  );
}
