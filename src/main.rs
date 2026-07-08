use std::{ time::Duration,  io::Stdout, process, path::PathBuf, fs, os::unix::fs::PermissionsExt };

use crossterm::event::{ self, Event };
use beautiful_log;
use tracing::{ info, error };

mod actions;
mod app;
mod events;
mod ssh_config;
mod ssh_operations;
mod ui;
mod terminal;
use events::key_to_action;
use ui::draw_ui;
use ssh_operations::{ start_ssh_process };
use terminal::{ setup_terminal, restore_terminal_to_normal_mode };
use ratatui::{ Terminal, backend::CrosstermBackend };
use crate::app::StatusMsgLevel::{Error, Warn, Info};
use crate::app::{ AppCommand, StatusMsg };
use crate::ssh_config::SshHost;
use app::{ App, AppMode, RsyncStatus, RsyncActiveInput, SshEstablishControlMaster };

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const RSYNC_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/rsync-binary" ));

fn main() -> Result<()> {
    beautiful_log::init_logging("debug", beautiful_log::LogTarget::File, Some("simple-ssh-tui-rs.log"));

    let mut app = app::init_app().unwrap();

    match prepare_rsync_binary() {
        Ok(rsync_path) => app.rsync_path = Some(rsync_path),
        Err(e) => { 
            app.status_msg = StatusMsg { level: Warn, msg: format!("failed to prepare bundeled rsync binary -> use host rsync error: {}", e) };
            error!("failed to prepare bundeled rsync binary -> use host rsync error: {}", e)
        },
    }

    
    let mut terminal = setup_terminal(app.ssh_hosts.len())?;
       
    loop {
        draw_ui(&mut terminal, &app);

    
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if let Some(action) = key_to_action(key) {
                    actions::handle_action(&mut app, action);
                }
            }
        }    

        process_msgs_on_channels(&mut app);

        process_app_commands(&mut app, &mut terminal)?;
        
    }
}

fn process_app_commands(app: &mut App, mut terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        while let Some(cmd) = app.commands.pop_front() {
            match cmd {
                AppCommand::Quit => {
                    restore_terminal_to_normal_mode(&mut terminal)?;
                    process::exit(0);
                },
                AppCommand::StartSsh => {
                    restore_terminal_to_normal_mode(&mut terminal)?;
                    
                    start_ssh_process(app.selected_ssh_host.clone());
                    process::exit(0);
                },
            }
        }
        
        Ok(())
}

fn process_msgs_on_channels(app: &mut App) {
        if let Ok(remote_suggestions) = app.remote_autocomplet_rx.try_recv() {            
            actions::process_remote_suggestions(remote_suggestions.folders, remote_suggestions.files, app);
        }

        if let Ok(rsync_status) = app.rsync_rx.try_recv() {
            match rsync_status {
                RsyncStatus::Progress(progress_msg) => {
                    app.status_msg = StatusMsg { level: Info, msg: progress_msg };

                },
                RsyncStatus::Completed(duration) => {
                    app.status_msg = StatusMsg { level: Info, msg: format!("rsync finished in {}ms", duration.as_millis()) };

                },
                RsyncStatus::Failed(err_msg, duration) => {
                    app.status_msg = StatusMsg { level: Error, msg: format!("rsync failed after {}ms with error: {}", duration.as_millis(), err_msg) };
                }
            }
        }

        if let Ok(ssh_login_output) = app.ssh_portable_pty_output_rx.try_recv() {
            match ssh_login_output {
                SshEstablishControlMaster::Succsess => {
                    app.app_mode = AppMode::Rsync;
                },
                SshEstablishControlMaster::Failure => {
                    app.app_mode = AppMode::SelectHost;
                },
                SshEstablishControlMaster::PasswordPromt(text) => {
                    app.ssh_login_output.push_str(&text);
                },
                SshEstablishControlMaster::UserInputReqired => {
                    app.app_mode = AppMode::SshPasswordPromt;
                }
            }
        }
       
        if let Ok(status_msg) = app.status_msgs_rx.try_recv() {
            app.status_msg = status_msg;
        }      
}


fn prepare_rsync_binary() -> Result<PathBuf>{
    let cache_dir = dirs_next::cache_dir().unwrap_or(PathBuf::from("/tmp")).join("simple.ssh-tui-rs");
    if let Err(e) = fs::create_dir_all(&cache_dir) {
        error!("failed to create rsync cache_dir: {} error: {}", cache_dir.display(), e);
    }

    let rsync_path = cache_dir.join("rsync");
    if !rsync_path.exists() {
        if let Err(e) = fs::write(&rsync_path, RSYNC_BYTES) {
            error!("failed to coppy rsync to: {} error: {}", &rsync_path.display(), e);
        }
        
        #[cfg(unix)]
        {
            let mut permissions = fs::metadata(&rsync_path)?.permissions();
            permissions.set_mode(0o755);
            if let Err(e) = fs::set_permissions(&rsync_path, permissions) {
                error!("failed to set rsync permisions: {}", e);
            }
        }
    }

    info!("rsync path: {}", rsync_path.display());
    Ok(rsync_path)
}
