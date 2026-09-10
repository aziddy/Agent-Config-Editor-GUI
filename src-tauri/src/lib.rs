pub mod actions;
pub mod claude;
pub mod codex;
pub mod commands;
pub mod error;
pub mod fsutil;
pub mod markdown;
pub mod model;
pub mod scan;
pub mod settings;
pub mod watcher;
pub mod writeplan;

use commands::CodexMcpStatusEntry;
use fsutil::{BackupStore, Homes};
use model::AppSettings;
use settings::SettingsStore;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::Manager;
use watcher::{ConfigWatcher, RecentWrites};

pub struct AppState {
    pub homes: Homes,
    pub settings: Mutex<AppSettings>,
    pub settings_store: SettingsStore,
    pub backups: Mutex<BackupStore>,
    pub recent_writes: Arc<RecentWrites>,
    pub watcher: Option<ConfigWatcher>,
    pub codex_status: Mutex<Option<(Instant, Vec<CodexMcpStatusEntry>)>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let homes = Homes::detect()?;
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let settings_store = SettingsStore::new(&config_dir);
            let mut settings = settings_store.load().unwrap_or_else(|e| {
                log::warn!("failed to load settings, using defaults: {e}");
                AppSettings::default()
            });
            if settings.editor_command.is_none() {
                settings.editor_command = Some(settings::detect_editor_command());
            }
            if settings.codex_binary.is_none() {
                settings.codex_binary =
                    settings::detect_codex_binary().map(|p| p.to_string_lossy().into_owned());
            }
            let backups =
                BackupStore::new(data_dir.join("backups"), settings.backup_limit as usize);
            log::info!("homes: {:?}", homes.info());
            let recent_writes = Arc::new(RecentWrites::default());
            let watcher = match ConfigWatcher::start(app.handle().clone(), recent_writes.clone()) {
                Ok(w) => {
                    w.watch_homes(&homes);
                    Some(w)
                }
                Err(e) => {
                    log::warn!("file watcher unavailable: {e}");
                    None
                }
            };
            app.manage(AppState {
                homes,
                settings: Mutex::new(settings),
                settings_store,
                backups: Mutex::new(backups),
                recent_writes,
                watcher,
                codex_status: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan_all,
            commands::get_file_text,
            commands::list_dir,
            commands::get_app_settings,
            commands::set_app_settings,
            commands::reveal_in_file_manager,
            commands::open_url,
            commands::open_in_editor,
            commands::preview_save_file_text,
            commands::preview_upsert_mcp_server,
            commands::preview_delete_mcp_server,
            commands::preview_set_mcp_enabled,
            commands::preview_set_plugin_enabled,
            commands::preview_set_skill_enabled,
            commands::preview_save_frontmatter,
            commands::preview_save_automation,
            commands::preview_set_automation_status,
            commands::preview_create_automation,
            commands::preview_create_entity,
            commands::preview_delete_path,
            commands::apply_write_plan,
            commands::list_backups,
            commands::read_backup,
            commands::preview_restore_backup,
            commands::codex_mcp_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
