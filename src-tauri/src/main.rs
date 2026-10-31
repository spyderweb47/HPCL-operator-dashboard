// No console window behind the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_vendor = "win7")]
mod win7;

fn main() {
    hpcl_operator_lib::run()
}
