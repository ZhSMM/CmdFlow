/**
 * 启动器独立窗口 (Phase 9.4)
 *
 * 通过 Tauri `show_palette_window` 创建独立窗口,URL 携带 ?window=palette,
 * main.tsx 据此直接渲染本组件,绕过主路由。
 */
import { CommandPalette } from "@/components/palette/CommandPalette";

export function PalettePage() {
  return <CommandPalette mode="standalone" />;
}
