// main.rs — thin binary entry; all logic lives in lib.rs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    collider_lib::run()
}
