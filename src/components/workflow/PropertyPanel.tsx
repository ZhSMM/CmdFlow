import { Trash2 } from "lucide-react";
import { Input, Textarea } from "@/components/ui/Input";
import { Button } from "@/components/ui/Button";
import { Label } from "@/components/ui/Label";
import type { Edge, Node } from "reactflow";

interface NodeTypeMeta {
  type_id: string;
  display_name: string;
  category: string;
  description: string;
  config_schema: any;
}

interface PropertyPanelProps {
  node: Node | null;
  edge: Edge | null;
  nodeTypes: NodeTypeMeta[];
  onUpdateNode: (id: string, patch: any) => void;
  onDeleteNode: (id: string) => void;
  onDeleteEdge: () => void;
  onUpdateEdge: (id: string, patch: any) => void;
}

export function PropertyPanel({
  node,
  edge,
  nodeTypes,
  onUpdateNode,
  onDeleteNode,
  onDeleteEdge,
  onUpdateEdge,
}: PropertyPanelProps) {
  if (!node && !edge) {
    return (
      <div className="w-72 flex-shrink-0 border-l bg-card p-4 text-xs text-muted-foreground">
        <p>选中节点或边查看属性</p>
        <p className="mt-2 text-[10px]">
          提示：从左侧拖拽节点到画布，按住节点拖动创建连接
        </p>
      </div>
    );
  }

  if (edge) {
    return (
      <div className="w-72 flex-shrink-0 overflow-auto border-l bg-card p-4">
        <div className="mb-3 flex items-center justify-between">
          <h3 className="text-sm font-semibold">边属性</h3>
          <Button size="sm" variant="ghost" onClick={onDeleteEdge}>
            <Trash2 className="h-3 w-3" />
          </Button>
        </div>
        <div className="space-y-2 text-xs">
          <div>
            <Label>源节点</Label>
            <Input value={edge.source} readOnly className="h-7 text-xs" />
          </div>
          <div>
            <Label>目标节点</Label>
            <Input value={edge.target} readOnly className="h-7 text-xs" />
          </div>
          <div>
            <Label>触发条件</Label>
            <select
              className="flex h-7 w-full rounded-md border border-input bg-transparent px-2 text-xs"
              value={edge.data?.condition?.type || "on_success"}
              onChange={(e) => {
                const newCond = { ...(edge.data?.condition || {}), type: e.target.value };
                onUpdateEdge(edge.id, { condition: newCond });
              }}
            >
              <option value="on_success">上游成功时</option>
              <option value="on_failure">上游失败时</option>
              <option value="always">总是</option>
            </select>
          </div>
        </div>
      </div>
    );
  }

  if (node) {
    const meta = nodeTypes.find((t) => t.type_id === node.data.type_id);
    if (!meta) {
      return (
        <div className="w-72 border-l bg-card p-4 text-xs text-destructive">
          未知节点类型: {node.data.type_id}
        </div>
      );
    }
    return (
      <div className="w-72 flex-shrink-0 overflow-auto border-l bg-card">
        <div className="flex items-center justify-between border-b p-3">
          <div>
            <h3 className="text-sm font-semibold">{meta.display_name}</h3>
            <p className="text-[10px] text-muted-foreground">{meta.description}</p>
          </div>
          <Button size="sm" variant="ghost" onClick={() => onDeleteNode(node.id)}>
            <Trash2 className="h-3 w-3" />
          </Button>
        </div>
        <div className="space-y-3 p-3 text-xs">
          <SchemaForm
            schema={meta.config_schema}
            value={node.data.config || {}}
            onChange={(v) => onUpdateNode(node.id, { config: v })}
          />
        </div>
      </div>
    );
  }

  return null;
}

function SchemaForm({
  schema,
  value,
  onChange,
}: {
  schema: any;
  value: any;
  onChange: (v: any) => void;
}) {
  if (!schema?.properties) {
    return <div className="text-muted-foreground">无配置项</div>;
  }

  return (
    <div className="space-y-3">
      {(schema.required || []).map((k: string) => {
        const prop = schema.properties[k];
        if (!prop) return null;
        return (
          <Field
            key={k}
            name={k}
            prop={prop}
            value={value?.[k]}
            required
            onChange={(v) => onChange({ ...value, [k]: v })}
          />
        );
      })}
      {Object.entries(schema.properties as Record<string, any>)
        .filter(([k]) => !(schema.required || []).includes(k))
        .map(([k, prop]) => (
          <Field
            key={k}
            name={k}
            prop={prop}
            value={value?.[k]}
            onChange={(v) => onChange({ ...value, [k]: v })}
          />
        ))}
    </div>
  );
}

function Field({
  name,
  prop,
  value,
  required,
  onChange,
}: {
  name: string;
  prop: any;
  value: any;
  required?: boolean;
  onChange: (v: any) => void;
}) {
  return (
    <div>
      <Label>
        {prop.title || name}
        {required && <span className="ml-0.5 text-destructive">*</span>}
      </Label>
      {prop.description && (
        <p className="mb-1 text-[10px] text-muted-foreground">{prop.description}</p>
      )}
      {prop.enum ? (
        <select
          className="flex h-7 w-full rounded-md border border-input bg-transparent px-2 text-xs"
          value={value ?? ""}
          onChange={(e) => onChange(e.target.value)}
        >
          <option value="">-- 请选择 --</option>
          {prop.enum.map((v: string) => (
            <option key={v} value={v}>
              {v}
            </option>
          ))}
        </select>
      ) : prop.type === "boolean" ? (
        <label className="flex cursor-pointer items-center gap-2">
          <input
            type="checkbox"
            checked={!!value}
            onChange={(e) => onChange(e.target.checked)}
            className="h-3.5 w-3.5"
          />
          <span>{value ? "是" : "否"}</span>
        </label>
      ) : prop.type === "integer" || prop.type === "number" ? (
        <Input
          type="number"
          value={value === undefined || value === null ? "" : String(value)}
          onChange={(e) => onChange(e.target.value === "" ? undefined : Number(e.target.value))}
          className="h-7 text-xs"
        />
      ) : prop.format === "textarea" || (prop.type === "string" && (name.includes("code") || name.includes("body") || name.includes("content"))) ? (
        <Textarea
          rows={4}
          value={value ?? ""}
          onChange={(e) => onChange(e.target.value)}
          className="text-xs font-mono"
        />
      ) : prop.type === "object" ? (
        <Textarea
          rows={3}
          value={value !== undefined ? JSON.stringify(value, null, 2) : ""}
          onChange={(e) => {
            try {
              onChange(JSON.parse(e.target.value));
            } catch {
              onChange(e.target.value);
            }
          }}
          className="font-mono text-[10px]"
        />
      ) : (
        <Input
          value={value ?? ""}
          onChange={(e) => onChange(e.target.value)}
          className="h-7 text-xs"
        />
      )}
    </div>
  );
}
