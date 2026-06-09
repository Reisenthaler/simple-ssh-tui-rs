use std::{ time::Duration,  io::Stdout};

use crossterm::event::{ self, Event };
use beautiful_log;

mod actions;
mod app;
mod events;
mod ssh_config;
mod ssh_operations;
mod ui;
mod terminal;
use events::key_to_action;
use ui::draw_ui;
use ssh_operations::{ start_ssh_process,  run_rsync_process, start_background_ssh };
use terminal::{ setup_terminal, restore_terminal_to_normal_mode };
use ratatui::{ Terminal, backend::CrosstermBackend };
use crate::app::{ AppCommand };
use crate::ssh_config::SshHost;
use app::{ App, AppMode, RsyncStatus, RsyncActiveInput, SshEstablishControlMaster };
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;


fn main() -> Result<()> {
    beautiful_log::init_logging("INFO", beautiful_log::LogTarget::File, Some("simple-ssh-tui-rs.log"));

    let mut app = app::init_app().unwrap();
    
    let mut terminal = setup_terminal(app.ssh_hosts.len())?;
       
    loop {
        draw_ui(&mut terminal, &mut app);

        process_msgs_on_channels(&mut app);

        process_app_commands(&mut app, &mut terminal)?;
        
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if let Some(action) = key_to_action(key) {
                    actions::handle_action(&mut app, action);
                }
            }
        }
    }
}

fn process_app_commands(app: &mut App, mut terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        while let Some(cmd) = app.commands.pop_front() {
            match cmd {
                AppCommand::Quit => {
                    restore_terminal_to_normal_mode(&mut terminal)?;
                    return Ok(());
                },
                AppCommand::StartSsh => {
                    restore_terminal_to_normal_mode(&mut terminal)?;
                    
                    start_ssh_process(app.selected_ssh_host.clone());
                    return Ok(());
                }
                AppCommand::StartRsync => {
                    run_rsync_process(app.selected_ssh_host.clone(), app.rsync_local_path.clone(), app.rsync_remote_path.clone(), app.rsync_tx.clone());
                },
                AppCommand::StartSshControlMaster(ssh_host) => {
                    app.app_mode = AppMode::SshPasswordPromt;
                    let (ssh_portable_pty_input_tx, ssh_portable_pty_output_rx) = start_background_ssh(ssh_host.clone());
                    app.ssh_portable_pty_input_tx = ssh_portable_pty_input_tx;
                    app.ssh_portable_pty_output_rx = ssh_portable_pty_output_rx;
                }
            }
            
        }
        
        Ok(())
}

fn process_msgs_on_channels(app: &mut App) {
        if let Ok(all_folders) = app.remote_autocomplet_rx.try_recv() {
            let (_, prefix) = split_path(&app.rsync_remote_path);
            
            app.remote_suggestions = all_folders
                .into_iter()
                .filter(|folder| folder.to_lowercase().starts_with(&prefix.to_lowercase()))
                .collect();
        }

        if let Ok(rsync_status) = app.rsync_rx.try_recv() {
            match rsync_status {
                RsyncStatus::Progress(progress_msg) => {
                  app.status_msg = progress_msg;  
                },
                RsyncStatus::Completed(exit_status) => {
                    app.status_msg = format!("rsync finished with status: {}", exit_status);
                },
                RsyncStatus::Failed(err_msg) => {
                    app.status_msg = format!("rsync failed with error: {}", err_msg);
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
                }
            }
        } 
}

fn split_path(input: &str) -> (String, String) {
    if let Some(index) = input.rfind('/') {
        let parent_dir = &input[..=index];
        let prefix = &input[index + 1..];

        let parent_dir = if parent_dir.is_empty() { "/".to_string() } else {
            parent_dir.to_string()
        };
        (parent_dir, prefix.to_string())
    } else {
        ("./".to_string(), input.to_string())
    }
}
