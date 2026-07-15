import { useCallback, useEffect, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  addEdge,
  applyEdgeChanges,
  applyNodeChanges,
  MarkerType,
  type Edge,
  type Node,
  type NodeChange,
  type EdgeChange,
  type Connection,
} from "reactflow";
import { Save, Play, CheckCircle2, XCircle, AlertTriangle, Loader2, Download } from "lucide-react";
import { api, type WorkflowEdge as WfEdge, TauriError } from "@/lib/tauri";
import { Button } from "@/components/ui/Button";
import { NodePanel } from "./NodePanel";
import { PropertyPanel } from "./PropertyPanel";
import { cn } from "@/lib/utils";
import { ReactFlowLazy } from "./ReactFlowLazy";

export interface NodeTypeMeta {
  type_id: string;
  display_name: string;
  category: string;
  description: string;
  config_schema: any;
}

interface WorkflowEditorProps {
  workflowId: string;
}

export function WorkflowEditor({ workflowId }: WorkflowEditorProps) {
  const qc = useQueryClient();
  const [nodes, setNodes] = useState<Node[]>([]);
  const [edges, setEdges] = useState<Edge[]>([]);
  const [selectedNode, setSelectedNode] = useState<Node | null>(null);
  const [selectedEdge, setSelectedEdge] = useState<Edge | null>(null);
  const [dirty, setDirty] = useState(false);
  const [runState, setRunState] = useState<"idle" | "running" | "success" | "failed">("idle");
  const [executionId, setExecutionId] = useState<string | null>(null);
  const [validation, setValidation] = useState<{ ok: boolean; errors: string[]; warnings: string[] } | null>(null);

  const detail = useQuery({
    queryKey: ["workflow", workflowId],
    queryFn: () => api.workflow.get(workflowId),
  });

  const nodeTypes = useQuery({
    queryKey: ["node-types"],
    queryFn: () => api.workflow.listNodeTypes(),
  });

  // 加载工作流详情到本地 state
  useEffect(() => {
    if (!detail.data) return;
    const wf = detail.data;
    setNodes(
      wf.nodes.map((n) => ({
        id: n.id,
        type: "default",
        position: { x: n.position_x, y: n.position_y },
        data: {
          type_id: n.type,
          config: n.config,
          label: n.type,
        },
      })),
    );
    setEdges(
      wf.edges.map((e) => ({
        id: e.id,
        source: e.source_node,
        target: e.target_node,
        sourceHandle: e.source_port || undefined,
        targetHandle: e.target_port || undefined,
        label: e.condition ? describeCondition(e.condition) : undefined,
        markerEnd: { type: MarkerType.ArrowClosed },
        style: { strokeWidth: 2 },
        data: { condition: e.condition },
      })),
    );
    setDirty(false);
  }, [detail.data]);

  const onNodesChange = useCallback((changes: NodeChange[]) => {
    setNodes((nds) => applyNodeChanges(changes, nds));
    setDirty(true);
  }, []);

  const onEdgesChange = useCallback((changes: EdgeChange[]) => {
    setEdges((eds) => applyEdgeChanges(changes, eds));
    setDirty(true);
  }, []);

  const onConnect = useCallback((connection: Connection) => {
    setEdges((eds) =>
      addEdge(
        {
          ...connection,
          markerEnd: { type: MarkerType.ArrowClosed },
          style: { strokeWidth: 2 },
        },
        eds,
      ),
    );
    setDirty(true);
  }, []);

  const onDrop = useCallback(
    (event: React.DragEvent) => {
      event.preventDefault();
      const data = event.dataTransfer.getData("application/reactflow");
      if (!data) return;
      try {
        const { type_id: droppedTypeId } = JSON.parse(data);
        const meta = nodeTypes.data?.find((t: NodeTypeMeta) => t.type_id === droppedTypeId);
        if (!meta) return;

        const position = (event.target as HTMLElement)
          .closest(".reactflow__pane")
          ?.getBoundingClientRect();
        if (!position) return;
        const newNode: Node = {
          id: `n_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`,
          type: "default",
          position: {
            x: event.clientX - position.left - 100,
            y: event.clientY - position.top - 30,
          },
          data: {
            type_id: droppedTypeId,
            config: defaultConfigFor(droppedTypeId, meta.config_schema),
            label: meta.display_name,
          },
        };
        setNodes((nds) => [...nds, newNode]);
        setDirty(true);
      } catch {
        // ignore
      }
    },
    [nodeTypes.data],
  );

  const onDragOver = useCallback((event: React.DragEvent) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
  }, []);

  const updateNodeData = (id: string, patch: any) => {
    setNodes((nds) =>
      nds.map((n) => (n.id === id ? { ...n, data: { ...n.data, ...patch } } : n)),
    );
    setDirty(true);
  };

  const deleteNode = (id: string) => {
    setNodes((nds) => nds.filter((n) => n.id !== id));
    setEdges((eds) => eds.filter((e) => e.source !== id && e.target !== id));
    if (selectedNode?.id === id) setSelectedNode(null);
    setDirty(true);
  };

  const save = useMutation({
    mutationFn: async () => {
      return api.workflow.update({
        id: workflowId,
        nodes: nodes.map((n, i) => ({
          id: n.id,
          workflow_id: workflowId,
          type: n.data.type_id,
          command_id: null,
          config: n.data.config,
          position_x: n.position.x,
          position_y: n.position.y,
          sort_order: i,
        })) as any,
        edges: edges.map<WfEdge>((e) => ({
          id: e.id,
          workflow_id: workflowId,
          source_node: e.source,
          target_node: e.target,
          source_port: e.sourceHandle || null,
          target_port: e.targetHandle || null,
          condition: e.data?.condition || null,
        })),
      });
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["workflow", workflowId] });
      qc.invalidateQueries({ queryKey: ["workflows"] });
      setDirty(false);
    },
  });

  const validate = useMutation({
    mutationFn: () =>
      api.workflow.validateDag({
        nodes: nodes.map((n) => ({
          id: n.id,
          workflow_id: workflowId,
          type: n.data.type_id,
          command_id: null,
          config: n.data.config,
          position_x: n.position.x,
          position_y: n.position.y,
          sort_order: 0,
        })) as any,
        edges: edges.map((e) => ({
          id: e.id,
          workflow_id: workflowId,
          source_node: e.source,
          target_node: e.target,
          source_port: e.sourceHandle || null,
          target_port: e.targetHandle || null,
          condition: e.data?.condition || null,
        })) as any,
      }),
    onSuccess: (res) => {
      setValidation({ ok: res.ok, errors: res.errors, warnings: res.warnings });
    },
  });

  const run = useMutation({
    mutationFn: () => api.workflow.run({ workflow_id: workflowId, params: {} }),
    onSuccess: (res) => {
      setExecutionId(res.execution_id);
      setRunState("running");
    },
    onError: (e) => {
      setRunState("failed");
      alert(`执行失败: ${(e as TauriError).message}`);
    },
  });

  const cancel = useMutation({
    mutationFn: () =>
      executionId ? api.workflow.cancelExecution(executionId) : Promise.resolve(false),
  });

  const exportYaml = useMutation({
    mutationFn: () => api.yaml.exportWorkflow(workflowId),
    onSuccess: (yaml) => {
      const blob = new Blob([yaml], { type: "text/yaml" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${detail.data?.name || "workflow"}.yaml`;
      a.click();
      URL.revokeObjectURL(url);
    },
  });

  // 监听执行事件
  useEffect(() => {
    if (!executionId) return;
    const handler = (e: any) => {
      if (e.execution_id !== executionId) return;
      if (e.kind === "execution_finished") {
        setRunState(e.status === "success" ? "success" : "failed");
      }
    };
    // 通过 onRunEvent 订阅
    let unlisten: (() => void) | undefined;
    import("@/lib/tauri").then(({ onRunEvent }) => {
      onRunEvent(handler).then((u) => {
        unlisten = u;
      });
    });
    return () => unlisten?.();
  }, [executionId]);

  return (
    <div className="flex h-full min-h-0">
      {/* 左侧:节点面板 */}
      <NodePanel nodeTypes={nodeTypes.data || []} />

      {/* 中间:画布 */}
      <div className="flex flex-1 min-w-0 flex-col min-h-0">
        <div className="flex items-center gap-2 border-b bg-card px-4 py-2">
          <span className="text-sm font-medium">{detail.data?.name || "加载中..."}</span>
          {dirty && <span className="text-xs text-amber-500">未保存</span>}
          {validation && (
            <span
              className={cn(
                "flex items-center gap-1 text-xs",
                validation.ok ? "text-green-600" : "text-destructive",
              )}
            >
              {validation.ok ? (
                <CheckCircle2 className="h-3 w-3" />
              ) : (
                <XCircle className="h-3 w-3" />
              )}
              {validation.ok ? "DAG 有效" : `${validation.errors.length} 个错误`}
            </span>
          )}
          <div className="flex-1" />
          <Button size="sm" variant="outline" onClick={() => validate.mutate()}>
            <CheckCircle2 className="h-3 w-3" />
            验证
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={() => save.mutate()}
            disabled={!dirty || save.isPending}
          >
            {save.isPending ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Save className="h-3 w-3" />
            )}
            保存
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={() => exportYaml.mutate()}
            disabled={exportYaml.isPending}
            title="导出为 YAML 文件"
          >
            <Download className="h-3 w-3" />
            导出
          </Button>
          {runState === "running" ? (
            <Button size="sm" variant="destructive" onClick={() => cancel.mutate()}>
              取消
            </Button>
          ) : (
            <Button size="sm" onClick={() => run.mutate()} disabled={run.isPending}>
              <Play className="h-3 w-3" />
              运行
            </Button>
          )}
          {runState === "success" && (
            <span className="flex items-center gap-1 text-xs text-green-600">
              <CheckCircle2 className="h-3 w-3" /> 完成
            </span>
          )}
          {runState === "failed" && (
            <span className="flex items-center gap-1 text-xs text-destructive">
              <XCircle className="h-3 w-3" /> 失败
            </span>
          )}
        </div>

        <div className="relative flex-1 min-h-0" onDrop={onDrop} onDragOver={onDragOver}>
          <ReactFlowLazy
            nodes={nodes}
            edges={edges}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            nodesDraggable
            elementsSelectable
            onNodeClick={(_, n) => {
              setSelectedNode(n);
              setSelectedEdge(null);
            }}
            onEdgeClick={(_, e) => {
              setSelectedEdge(e);
              setSelectedNode(null);
            }}
            onPaneClick={() => {
              setSelectedNode(null);
              setSelectedEdge(null);
            }}
          />
        </div>

        {validation && (validation.errors.length > 0 || validation.warnings.length > 0) && (
          <div className="border-t bg-card p-2 text-xs">
            {validation.errors.map((e, i) => (
              <div key={i} className="flex items-center gap-1 text-destructive">
                <XCircle className="h-3 w-3" /> {e}
              </div>
            ))}
            {validation.warnings.map((w, i) => (
              <div key={i} className="flex items-center gap-1 text-amber-600">
                <AlertTriangle className="h-3 w-3" /> {w}
              </div>
            ))}
          </div>
        )}
      </div>

      {/* 右侧:属性面板 */}
      <PropertyPanel
        node={selectedNode}
        edge={selectedEdge}
        nodeTypes={nodeTypes.data || []}
        onUpdateNode={updateNodeData}
        onDeleteNode={deleteNode}
        onDeleteEdge={() => {
          if (selectedEdge) {
            setEdges((eds) => eds.filter((e) => e.id !== selectedEdge.id));
            setSelectedEdge(null);
            setDirty(true);
          }
        }}
        onUpdateEdge={(id, patch) => {
          setEdges((eds) =>
            eds.map((e) => (e.id === id ? { ...e, data: { ...e.data, ...patch } } : e)),
          );
          setDirty(true);
        }}
      />
    </div>
  );
}

function describeCondition(cond: any): string {
  if (typeof cond === "string") return cond;
  if (cond?.type) {
    if (cond.type === "on_success") return "成功";
    if (cond.type === "on_failure") return "失败";
    if (cond.type === "always") return "总是";
  }
  return cond?.type || "";
}

function defaultConfigFor(_typeId: string, schema: any): any {
  const props = schema?.properties || {};
  const result: any = {};
  for (const [k, v] of Object.entries(props) as [string, any][]) {
    if (v.default !== undefined) {
      result[k] = v.default;
    } else if (v.type === "string") {
      result[k] = "";
    } else if (v.type === "integer" || v.type === "number") {
      result[k] = 0;
    } else if (v.type === "boolean") {
      result[k] = false;
    } else if (v.type === "object") {
      result[k] = {};
    } else if (v.type === "array") {
      result[k] = [];
    }
  }
  return result;
}
