// lib.rs — the Tauri application entry. Registers state + IPC commands.

mod core;
mod neutron;
mod commands;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::prefix_info,
            commands::doctor,
            commands::apply_display_fix,
            commands::launch_premiere,
            commands::is_premiere_alive,
            commands::clean_exit,
            commands::force_quit,
            commands::current_step,
            commands::get_settings,
            commands::set_settings,
            commands::detect_scale,
            commands::compositor_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Collider");
}
