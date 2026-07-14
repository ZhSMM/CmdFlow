import { useState } from "react";
import { useTheme, type Theme } from "@/stores/theme";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/Card";
import { Button } from "@/components/ui/Button";
import { Sun, Moon, Laptop2, Eye, EyeOff, KeyRound, Database } from "lucide-react";
import { api } from "@/lib/tauri";
import { useQuery } from "@tanstack/react-query";
import { formatDate } from "@/lib/utils";

export function SettingsPage() {
  const { theme, setTheme } = useTheme();
  const [secretName, setSecretName] = useState("");
  const [secretValue, setSecretValue] = useState("");
  const [showValue, setShowValue] = useState(false);

  const appInfo = useQuery({
    queryKey: ["app-info"],
    queryFn: () => api.system.getAppInfo(),
  });

  return (
    <div className="h-full overflow-auto p-6">
      <h1 className="mb-4 text-lg font-semibold">设置</h1>
      <div className="space-y-4">
        {/* 主题 */}
        <Card>
          <CardHeader>
            <CardTitle>外观</CardTitle>
            <CardDescription>主题切换</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="flex gap-2">
              {([
                { v: "light" as Theme, label: "浅色", icon: Sun },
                { v: "dark" as Theme, label: "深色", icon: Moon },
                { v: "system" as Theme, label: "跟随系统", icon: Laptop2 },
              ]).map((t) => (
                <Button
                  key={t.v}
                  variant={theme === t.v ? "default" : "outline"}
                  size="sm"
                  onClick={() => setTheme(t.v)}
                >
                  <t.icon className="h-4 w-4" />
                  {t.label}
                </Button>
              ))}
            </div>
          </CardContent>
        </Card>

        {/* 敏感参数 */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <KeyRound className="h-4 w-4" />
              敏感参数
            </CardTitle>
            <CardDescription>
              通过 OS keyring 加密存储,用于 AI 节点等场景
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="grid grid-cols-2 gap-2">
              <div>
                <label className="mb-1 block text-xs font-medium">名称</label>
                <input
                  value={secretName}
                  onChange={(e) => setSecretName(e.target.value)}
                  placeholder="openai_key"
                  className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 text-sm"
                />
              </div>
              <div>
                <label className="mb-1 block text-xs font-medium">值</label>
                <div className="relative">
                  <input
                    type={showValue ? "text" : "password"}
                    value={secretValue}
                    onChange={(e) => setSecretValue(e.target.value)}
                    className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 pr-9 text-sm"
                  />
                  <button
                    type="button"
                    onClick={() => setShowValue(!showValue)}
                    className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                  >
                    {showValue ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                  </button>
                </div>
              </div>
            </div>
            <div className="text-xs text-muted-foreground">
              暂未实现 keyring IPC 端(Phase 4 简化)。先用环境变量 (OPENAI_API_KEY) 代替。
            </div>
          </CardContent>
        </Card>

        {/* 应用信息 */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Database className="h-4 w-4" />
              应用信息
            </CardTitle>
          </CardHeader>
          <CardContent className="text-xs">
            {appInfo.data && (
              <div className="space-y-1">
                <div>
                  <span className="text-muted-foreground">名称: </span>
                  <code>{appInfo.data.name}</code>
                </div>
                <div>
                  <span className="text-muted-foreground">版本: </span>
                  <code>{appInfo.data.version}</code>
                </div>
                <div>
                  <span className="text-muted-foreground">启动时间: </span>
                  <code>{formatDate(appInfo.data.started_at)}</code>
                </div>
                <div>
                  <span className="text-muted-foreground">运行时长: </span>
                  <code>{appInfo.data.uptime_seconds} 秒</code>
                </div>
              </div>
            )}
          </CardContent>
        </Card>

        {/* 关于 */}
        <Card>
          <CardHeader>
            <CardTitle>关于 CmdFlow</CardTitle>
          </CardHeader>
          <CardContent className="text-sm text-muted-foreground">
            <p>本地命令编排工具</p>
            <p className="mt-2">技术栈: Tauri 2 + Rust + React + TypeScript + SQLite</p>
            <p className="mt-1">设计: 见 <a className="underline" href="#" onClick={(e) => e.preventDefault()}>DESIGN.md</a></p>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
