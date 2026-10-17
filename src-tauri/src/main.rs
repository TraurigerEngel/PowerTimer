#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    powertimer_lib::run();
}
