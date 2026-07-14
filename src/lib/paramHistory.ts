/**
 * 参数输入历史（localStorage 持久化）
 *
 * 每个命令维护最近 N 次的参数快照，用户可以一键回填。
 * 敏感参数（password/secret 等）以 sensitive=true 标记，不写入历史。
 */

const STORAGE_PREFIX = "cmdflow:param-history:";
const MAX_ENTRIES = 20;

export interface ParamHistoryEntry {
  values: Record<string, unknown>;
  used_at: number; // Date.now()
}

export function getParamHistory(commandId: string): ParamHistoryEntry[] {
  try {
    const raw = localStorage.getItem(STORAGE_PREFIX + commandId);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (e) =>
        e &&
        typeof e.used_at === "number" &&
        e.values &&
        typeof e.values === "object",
    );
  } catch {
    return [];
  }
}

/**
 * 把当前参数推入历史。
 * - 跳过空参数（用户没填过任何字段）
 * - 跳过跟最近一次完全相同的（重复点执行不污染历史）
 * - 自动裁剪到 MAX_ENTRIES
 *
 * sensitiveParams 是不应该写历史的参数名集合（前端过滤，不走后端）。
 */
export function pushParamHistory(
  commandId: string,
  values: Record<string, unknown>,
  sensitiveParams: string[] = [],
): void {
  const filtered: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(values)) {
    if (sensitiveParams.includes(k)) continue;
    filtered[k] = v;
  }
  if (Object.keys(filtered).length === 0) return;

  const history = getParamHistory(commandId);
  // 跟最近一条相同 → 跳过（仅刷新时间）
  if (history.length > 0 && shallowEqual(history[0].values, filtered)) {
    history[0].used_at = Date.now();
    saveHistory(commandId, history);
    return;
  }
  history.unshift({ values: filtered, used_at: Date.now() });
  if (history.length > MAX_ENTRIES) history.length = MAX_ENTRIES;
  saveHistory(commandId, history);
}

export function clearParamHistory(commandId: string): void {
  localStorage.removeItem(STORAGE_PREFIX + commandId);
}

function saveHistory(commandId: string, history: ParamHistoryEntry[]): void {
  try {
    localStorage.setItem(STORAGE_PREFIX + commandId, JSON.stringify(history));
  } catch (e) {
    // localStorage 满 / 不可用，忽略
    console.warn("[paramHistory] save failed", e);
  }
}

function shallowEqual(
  a: Record<string, unknown>,
  b: Record<string, unknown>,
): boolean {
  const ka = Object.keys(a);
  const kb = Object.keys(b);
  if (ka.length !== kb.length) return false;
  for (const k of ka) {
    if (a[k] !== b[k]) return false;
  }
  return true;
}

/** 把历史条目格式化成单行预览（用于下拉展示） */
export function formatHistoryPreview(
  values: Record<string, unknown>,
  maxLen = 60,
): string {
  const parts = Object.entries(values)
    .filter(([, v]) => v !== undefined && v !== null && v !== "")
    .map(([k, v]) => {
      const s = Array.isArray(v) ? `[${v.join(",")}]` : String(v);
      return `${k}=${s.length > 20 ? s.slice(0, 20) + "…" : s}`;
    });
  const joined = parts.join("  ");
  return joined.length > maxLen ? joined.slice(0, maxLen) + "…" : joined || "(空)";
}
