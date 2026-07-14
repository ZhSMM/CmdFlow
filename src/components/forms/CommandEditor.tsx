import { useState, useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Save, Plus, Trash2 } from "lucide-react";
import { api, type CommandDetail, type CommandType, type ParamType, type CreateParamInput } from "@/lib/tauri";
import { Button } from "@/components/ui/Button";
import { Input, Textarea } from "@/components/ui/Input";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { TauriError } from "@/lib/tauri";
import { cn } from "@/lib/utils";

interface CommandEditorProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  commandId?: string; // 不传则新建
  onSaved?: (id: string) => void;
}

const COMMAND_TYPES: { value: CommandType; label: string; desc: string }[] = [
  { value: "cmd", label: "CMD", desc: "Windows cmd.exe" },
  { value: "pwsh", label: "PowerShell", desc: "PowerShell Core / Windows PowerShell" },
  { value: "python", label: "Python", desc: "python -c" },
  { value: "node", label: "Node", desc: "node -e" },
  { value: "bash", label: "Bash", desc: "bash -c" },
  { value: "script", label: "Script", desc: "脚本节点用" },
];

const PARAM_TYPES: ParamType[] = [
  "text", "textarea", "number", "boolean", "select", "multiselect",
  "file", "dir", "password", "json", "datetime", "cron",
];

export function CommandEditor({ open, onOpenChange, commandId, onSaved }: CommandEditorProps) {
  const qc = useQueryClient();
  const isEdit = !!commandId;

  const [form, setForm] = useState({
    name: "",
    description: "",
    category: "",
    type: "cmd" as CommandType,
    template: "",
    working_dir: "",
    timeout_ms: "",
    tags: "",
  });
  const [params, setParams] = useState<CreateParamInput[]>([]);

  const detail = useQuery({
    queryKey: ["command", commandId],
    queryFn: () => api.library.get(commandId!),
    enabled: open && isEdit,
  });

  useEffect(() => {
    if (open && isEdit && detail.data) {
      const d: CommandDetail = detail.data;
      setForm({
        name: d.name,
        description: d.description ?? "",
        category: d.category ?? "",
        type: d.type,
        template: d.version.template,
        working_dir: d.version.working_dir ?? "",
        timeout_ms: d.version.timeout_ms ? String(d.version.timeout_ms) : "",
        tags: d.tags.join(", "),
      });
      setParams(
        d.params.map((p) => ({
          name: p.name,
          label: p.label,
          type: p.type,
          required: p.required,
          default_value: p.default_value ?? undefined,
          options: p.options ?? undefined,
          validation: p.validation ?? undefined,
          sensitive: p.sensitive,
          description: p.description ?? undefined,
          sort_order: p.sort_order,
        })),
      );
    } else if (open && !isEdit) {
      setForm({
        name: "",
        description: "",
        category: "",
        type: "cmd",
        template: "",
        working_dir: "",
        timeout_ms: "",
        tags: "",
      });
      setParams([]);
    }
  }, [open, isEdit, detail.data]);

  const save = useMutation({
    mutationFn: async () => {
      const input = {
        ...(isEdit ? { id: commandId! } : {}),
        name: form.name.trim(),
        description: form.description.trim() || undefined,
        category: form.category.trim() || undefined,
        type: form.type,
        template: form.template,
        working_dir: form.working_dir.trim() || undefined,
        timeout_ms: form.timeout_ms ? Number(form.timeout_ms) : undefined,
        tags: form.tags
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean),
        params,
      };
      return isEdit
        ? api.library.update(input as any).then(() => commandId!)
        : api.library.create(input as any);
    },
    onSuccess: (id) => {
      qc.invalidateQueries({ queryKey: ["commands"] });
      qc.invalidateQueries({ queryKey: ["command", id] });
      onSaved?.(id);
      onOpenChange(false);
    },
  });

  const addParam = () => {
    setParams([
      ...params,
      {
        name: `param${params.length + 1}`,
        label: `参数 ${params.length + 1}`,
        type: "text",
        required: false,
        sensitive: false,
        sort_order: params.length,
      },
    ]);
  };

  const removeParam = (idx: number) => {
    setParams(params.filter((_, i) => i !== idx));
  };

  const updateParam = (idx: number, patch: Partial<CreateParamInput>) => {
    setParams(params.map((p, i) => (i === idx ? { ...p, ...patch } : p)));
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={isEdit ? "编辑命令" : "新建命令"}
      description={
        form.template
          ? "模板中使用 {{paramName}} 引用参数"
          : "模板留空也能保存，后续编辑"
      }
      className="max-w-3xl"
      footer={
        <>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            取消
          </Button>
          <Button
            onClick={() => save.mutate()}
            disabled={!form.name.trim() || !form.template.trim() || save.isPending}
          >
            <Save className="h-4 w-4" />
            {save.isPending ? "保存中..." : "保存"}
          </Button>
        </>
      }
    >
      {save.error && (
        <div className="mb-3 rounded-md border border-destructive bg-destructive/10 p-2 text-sm text-destructive">
          保存失败：{(save.error as TauriError).message}
        </div>
      )}

      <div className="space-y-4">
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="mb-1 block text-sm font-medium">名称 *</label>
            <Input
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              placeholder="如: git push"
            />
          </div>
          <div>
            <label className="mb-1 block text-sm font-medium">类型 *</label>
            <select
              className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 text-sm"
              value={form.type}
              onChange={(e) => setForm({ ...form, type: e.target.value as CommandType })}
            >
              {COMMAND_TYPES.map((t) => (
                <option key={t.value} value={t.value}>
                  {t.label} - {t.desc}
                </option>
              ))}
            </select>
          </div>
        </div>

        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="mb-1 block text-sm font-medium">分类</label>
            <Input
              value={form.category}
              onChange={(e) => setForm({ ...form, category: e.target.value })}
              placeholder="如: git, deploy, cleanup"
            />
          </div>
          <div>
            <label className="mb-1 block text-sm font-medium">标签 (逗号分隔)</label>
            <Input
              value={form.tags}
              onChange={(e) => setForm({ ...form, tags: e.target.value })}
              placeholder="如: git, push, force"
            />
          </div>
        </div>

        <div>
          <label className="mb-1 block text-sm font-medium">描述</label>
          <Input
            value={form.description}
            onChange={(e) => setForm({ ...form, description: e.target.value })}
            placeholder="简单说明这个命令做什么"
          />
        </div>

        <div>
          <label className="mb-1 block text-sm font-medium">命令模板 *</label>
          <Textarea
            rows={3}
            value={form.template}
            onChange={(e) => setForm({ ...form, template: e.target.value })}
            placeholder="git push origin {{branch}} --force"
            className="font-mono text-xs"
          />
          <p className="mt-1 text-xs text-muted-foreground">
            使用 <code className="rounded bg-secondary px-1">{"{{name}}"}</code> 引用参数
          </p>
        </div>

        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="mb-1 block text-sm font-medium">工作目录</label>
            <Input
              value={form.working_dir}
              onChange={(e) => setForm({ ...form, working_dir: e.target.value })}
              placeholder="留空用当前目录"
            />
          </div>
          <div>
            <label className="mb-1 block text-sm font-medium">超时 (毫秒)</label>
            <Input
              type="number"
              value={form.timeout_ms}
              onChange={(e) => setForm({ ...form, timeout_ms: e.target.value })}
              placeholder="留空无超时"
            />
          </div>
        </div>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <CardTitle className="text-sm">参数</CardTitle>
            <Button size="sm" variant="outline" onClick={addParam}>
              <Plus className="h-3 w-3" />
              添加参数
            </Button>
          </CardHeader>
          <CardContent>
            {params.length === 0 ? (
              <p className="text-xs text-muted-foreground">没有参数。执行时直接跑模板。</p>
            ) : (
              <div className="space-y-3">
                {params.map((p, idx) => (
                  <div key={idx} className="rounded-md border bg-muted/30 p-3">
                    <div className="mb-2 flex items-center justify-between">
                      <span className="text-xs font-medium text-muted-foreground">
                        参数 #{idx + 1}
                      </span>
                      <button
                        type="button"
                        onClick={() => removeParam(idx)}
                        className="text-muted-foreground hover:text-destructive"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </div>
                    <div className="grid grid-cols-3 gap-2">
                      <div>
                        <label className={cn(labelCls)}>变量名</label>
                        <Input
                          value={p.name}
                          onChange={(e) => updateParam(idx, { name: e.target.value })}
                          placeholder="branch"
                          className="h-8 text-xs"
                        />
                      </div>
                      <div>
                        <label className={cn(labelCls)}>显示名</label>
                        <Input
                          value={p.label}
                          onChange={(e) => updateParam(idx, { label: e.target.value })}
                          placeholder="分支"
                          className="h-8 text-xs"
                        />
                      </div>
                      <div>
                        <label className={cn(labelCls)}>类型</label>
                        <select
                          className="flex h-8 w-full rounded-md border border-input bg-transparent px-2 text-xs"
                          value={p.type}
                          onChange={(e) =>
                            updateParam(idx, { type: e.target.value as ParamType })
                          }
                        >
                          {PARAM_TYPES.map((t) => (
                            <option key={t} value={t}>
                              {t}
                            </option>
                          ))}
                        </select>
                      </div>
                    </div>
                    <div className="mt-2 flex flex-wrap items-center gap-3 text-xs">
                      <label className="flex items-center gap-1">
                        <input
                          type="checkbox"
                          checked={p.required}
                          onChange={(e) => updateParam(idx, { required: e.target.checked })}
                        />
                        必填
                      </label>
                      <label className="flex items-center gap-1">
                        <input
                          type="checkbox"
                          checked={p.sensitive}
                          onChange={(e) => updateParam(idx, { sensitive: e.target.checked })}
                        />
                        敏感 (加密)
                      </label>
                      <Input
                        value={p.default_value !== undefined ? String(p.default_value) : ""}
                        onChange={(e) => updateParam(idx, { default_value: e.target.value })}
                        placeholder="默认值"
                        className="h-7 w-32 text-xs"
                      />
                    </div>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </Dialog>
  );
}

const labelCls = "mb-0.5 block text-[11px] font-medium text-muted-foreground";
