// 防止 release 构建在 Windows 上弹出额外的控制台窗口，切勿删除！！
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    desktop_lib::run()
}
