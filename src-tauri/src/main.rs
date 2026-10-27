#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if ai_signal_lib::run_hook_cli_if_requested() { return; }
    ai_signal_lib::run();
}
