// lib.rs — the Tauri application entry. Registers state + IPC commands.

mod core;
mod neutron;
mod mudhut;
mod commands;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Run stored-settings migrations BEFORE the UI can read or rewrite settings.
    core::settings::migrate();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::prefix_info,
            commands::working_prefix,
            commands::list_prefixes,
            commands::discover_prefixes,
            commands::add_prefix,
            commands::remove_prefix,
            commands::rename_prefix,
            commands::select_prefix,
            commands::provision_prefix,
            commands::doctor,
            commands::apply_display_fix,
            commands::fonts_check,
            commands::fonts_repair,
            // generic per-app surface (multi-app widgets)
            commands::list_apps,
            commands::launch_app,
            commands::adopt_app,
            commands::is_app_alive,
            commands::clean_exit_app,
            commands::force_quit_app,
            commands::current_step_app,
            // premiere-compat (current frontend, until P2)
            commands::launch_premiere,
            commands::is_premiere_alive,
            commands::clean_exit,
            commands::force_quit,
            commands::current_step,
            commands::get_settings,
            commands::set_settings,
            commands::detect_scale,
            commands::compositor_info,
            commands::get_theme_presets,
            commands::get_icon_sets,
            commands::import_icon_set,
            // Mud Hut installer — Adobe sign-in (device/QR flow)
            commands::adobe_auth_begin,
            commands::adobe_auth_poll,
            // Mud Hut installer — app catalog + streaming install
            commands::mudhut_apps,
            commands::install_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Collider");
}
