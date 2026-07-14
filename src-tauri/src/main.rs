// 阻止额外 console 窗口在 Windows 上
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    cmdflow_lib::run()
}
