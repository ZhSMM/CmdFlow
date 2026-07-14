import { useState, useEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Input, Textarea } from "@/components/ui/Input";
import type { Param, ParamType } from "@/lib/tauri";
import { Eye, EyeOff, FolderOpen, FileText } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { cn } from "@/lib/utils";

interface ParamFieldProps {
  param: Param;
  value: unknown;
  onChange: (v: unknown) => void;
}

/**
 * 根据参数类型渲染对应输入控件
 */
export function ParamField({ param, value, onChange }: ParamFieldProps) {
  const [showPwd, setShowPwd] = useState(false);

  // 同步 default_value
  useEffect(() => {
    if (value === undefined && param.default_value !== undefined) {
      onChange(param.default_value);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const labelEl = (
    <div className="mb-1 flex items-center gap-2">
      <label className="text-sm font-medium">
        {param.label}
        {param.required && <span className="ml-0.5 text-destructive">*</span>}
      </label>
      <span className="rounded bg-secondary px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
        {param.name}
      </span>
      {param.sensitive && (
        <span className="rounded bg-amber-500/20 px-1.5 py-0.5 text-[10px] text-amber-600">
          敏感
        </span>
      )}
    </div>
  );

  const descEl = param.description && (
    <p className="mb-1.5 text-xs text-muted-foreground">{param.description}</p>
  );

  switch (param.type as ParamType) {
    case "text":
    case "password":
      return (
        <div>
          {labelEl}
          {descEl}
          <div className="relative">
            <Input
              type={param.type === "password" && !showPwd ? "password" : "text"}
              value={(value as string) ?? ""}
              onChange={(e) => onChange(e.target.value)}
              placeholder={param.default_value ? String(param.default_value) : undefined}
            />
            {param.type === "password" && (
              <button
                type="button"
                className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                onClick={() => setShowPwd(!showPwd)}
              >
                {showPwd ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
              </button>
            )}
          </div>
        </div>
      );

    case "textarea":
      return (
        <div>
          {labelEl}
          {descEl}
          <Textarea
            rows={4}
            value={(value as string) ?? ""}
            onChange={(e) => onChange(e.target.value)}
          />
        </div>
      );

    case "number":
      return (
        <div>
          {labelEl}
          {descEl}
          <Input
            type="number"
            value={value === undefined || value === null ? "" : String(value)}
            onChange={(e) => {
              const v = e.target.value;
              onChange(v === "" ? undefined : Number(v));
            }}
          />
        </div>
      );

    case "boolean":
      return (
        <div>
          {labelEl}
          {descEl}
          <label className="flex cursor-pointer items-center gap-2">
            <input
              type="checkbox"
              checked={!!value}
              onChange={(e) => onChange(e.target.checked)}
              className="h-4 w-4 rounded border-gray-300"
            />
            <span className="text-sm">{value ? "是" : "否"}</span>
          </label>
        </div>
      );

    case "select": {
      const options = (param.options as Array<{ label: string; value: string }>) || [];
      return (
        <div>
          {labelEl}
          {descEl}
          <select
            className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            value={(value as string) ?? ""}
            onChange={(e) => onChange(e.target.value)}
          >
            <option value="">-- 请选择 --</option>
            {options.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
        </div>
      );
    }

    case "multiselect": {
      const options = (param.options as Array<{ label: string; value: string }>) || [];
      const selected = (value as string[]) || [];
      return (
        <div>
          {labelEl}
          {descEl}
          <div className="space-y-1 rounded-md border p-2">
            {options.map((opt) => {
              const checked = selected.includes(opt.value);
              return (
                <label key={opt.value} className="flex cursor-pointer items-center gap-2">
                  <input
                    type="checkbox"
                    checked={checked}
                    onChange={(e) => {
                      const next = e.target.checked
                        ? [...selected, opt.value]
                        : selected.filter((s) => s !== opt.value);
                      onChange(next);
                    }}
                  />
                  <span className="text-sm">{opt.label}</span>
                </label>
              );
            })}
          </div>
        </div>
      );
    }

    case "file":
    case "dir": {
      return (
        <div>
          {labelEl}
          {descEl}
          <div className="flex gap-2">
            <Input
              value={(value as string) ?? ""}
              onChange={(e) => onChange(e.target.value)}
              placeholder={
                param.type === "file" ? "选择文件..." : "选择目录..."
              }
              readOnly
            />
            <Button
              type="button"
              variant="outline"
              onClick={async () => {
                const selected = await open({
                  directory: param.type === "dir",
                  multiple: false,
                });
                if (typeof selected === "string") {
                  onChange(selected);
                }
              }}
            >
              {param.type === "dir" ? (
                <FolderOpen className="h-4 w-4" />
              ) : (
                <FileText className="h-4 w-4" />
              )}
            </Button>
          </div>
        </div>
      );
    }

    case "datetime":
      return (
        <div>
          {labelEl}
          {descEl}
          <Input
            type="datetime-local"
            value={(value as string) ?? ""}
            onChange={(e) => onChange(e.target.value)}
          />
        </div>
      );

    case "cron":
      return (
        <div>
          {labelEl}
          {descEl}
          <Input
            value={(value as string) ?? ""}
            onChange={(e) => onChange(e.target.value)}
            placeholder="0 2 * * *"
          />
          <p className="mt-1 text-xs text-muted-foreground">
            标准 5 段 cron 表达式（分 时 日 月 周）
          </p>
        </div>
      );

    case "json":
      return (
        <div>
          {labelEl}
          {descEl}
          <Textarea
            rows={6}
            value={value !== undefined ? JSON.stringify(value, null, 2) : ""}
            onChange={(e) => {
              try {
                onChange(JSON.parse(e.target.value));
              } catch {
                onChange(e.target.value); // 暂存为字符串
              }
            }}
            className="font-mono text-xs"
          />
        </div>
      );

    default:
      return (
        <div>
          {labelEl}
          {descEl}
          <Input
            value={(value as string) ?? ""}
            onChange={(e) => onChange(e.target.value)}
            className={cn("border-dashed")}
          />
        </div>
      );
  }
}
