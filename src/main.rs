use std::fmt::format;
use std::mem::transmute;
use std::os::unix::process;
use std::path::Path;
use std::process::{Child, Stdio};
use std::ptr::fn_addr_eq;
use std::{ 
    process::Command,
    fs,
};
use std::io::{BufRead, BufReader, Stdout};
use std::sync::mpsc;
use std::{env, thread};
use std::time::Duration;

use crossterm::cursor::SavePosition;
use crossterm::event::KeyModifiers;
use crossterm::{
    event::{ self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode }, 
    execute, 
    terminal::{disable_raw_mode, enable_raw_mode}};
use beautiful_log;
use tracing::{ error, info };
use ratatui::{ 
    Terminal, TerminalOptions, Viewport, 
    backend::{ CrosstermBackend }, 
    widgets::ListState
    };

mod app;
mod ssh_config;
mod ssh_operations;
mod ui;
mod terminal;
use ssh_config::{ parse_ssh_config };
use ui::draw_ui;
use ssh_operations::{ start_ssh_process,  run_rsync_process, start_background_ssh };
use terminal::{ setup_terminal, restore_terminal_to_normal_mode };
use crate::ssh_config::SshHost;
use app::{ App, AppMode, RsyncStatus, RsyncActiveInput };
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;


fn main() -> Result<()> {
    beautiful_log::init_logging("INFO");

    let mut app = app::init_app().unwrap();
    
    let mut is_fetching = false;

    let mut is_syncing = false;
    
    let mut terminal = setup_terminal(app.ssh_hosts.len())?;
       
    loop {
        draw_ui(&mut terminal, &mut app);

        if let Ok(all_folders) = app.remote_autocomplet_rx.try_recv() {
            let (_, prefix) = split_path(&app.rsync_remote_path);
            
            app.remote_suggestions = all_folders
                .into_iter()
                .filter(|folder| folder.to_lowercase().starts_with(&prefix.to_lowercase()))
                .collect();
            
            is_fetching = false;
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

            is_syncing = false;
        }
        
        if event::poll(Duration::from_millis(30))? {
       if let Event::Key(key) = event::read()? {
           match key.code {
               KeyCode::Char('c')  if key.modifiers.contains(KeyModifiers::CONTROL) => {
                       restore_terminal_to_normal_mode(&mut terminal)?;
                       return Ok(())
               },
               KeyCode::Char('r') |  KeyCode::Char('R') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        match app.app_mode {
                            AppMode::SelectHost => {
                                app.app_mode = AppMode::Rsync;
                                    start_background_ssh(app.selected_ssh_host.clone(), app.ssh_hosts.len(), &mut terminal);
                            },
                            AppMode::Rsync => app.app_mode = AppMode::SelectHost,
                        }
               },
               KeyCode::Char(c) => {
                   if app.app_mode == AppMode::Rsync {
                       match app.rsync_active_input {
                           RsyncActiveInput::Local => app.rsync_local_path.push(c),
                           RsyncActiveInput::Remote => app.rsync_remote_path.push(c),
                       }
                   }  
               },
               KeyCode::Backspace => {
                   if app.app_mode == AppMode::Rsync {
                       match app.rsync_active_input {
                           RsyncActiveInput::Local => app.rsync_local_path.pop(),
                           RsyncActiveInput::Remote => app.rsync_remote_path.pop(),
                       };
                   }   
               },
               KeyCode::Tab => {
                 if app.app_mode == AppMode::Rsync && !is_fetching {
                     match app.rsync_active_input {
                        RsyncActiveInput::Local => {
                            let (parent_dir, prefix) = split_path(&app.rsync_local_path);
                            
                            let mut folder_list = Vec::new();

                            if let Ok(entries) = fs::read_dir(&parent_dir) {
                                for entry in entries.filter_map(std::result::Result::ok) {
                                    let file_name = entry.file_name().to_string_lossy().to_string();

                                    let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
                                    let display_name = if is_dir {
                                        format!("{}/", file_name)
                                    } else {
                                        file_name
                                    };
                                    
                                    if display_name.to_lowercase().starts_with(&prefix.to_lowercase()) {
                                        folder_list.push(display_name);
                                    }
                                }
                            }

                            app.local_suggestions = folder_list;
                            
                        },
                        RsyncActiveInput::Remote => {  
                             is_fetching = true;

                            let tx_clone = app.remote_autocomplet_tx.clone();
                            let host = app.selected_ssh_host.host.clone();
                            let path_to_search = app.rsync_remote_path.clone();
        
                            thread::spawn(move || {
        
                                let (parent_dir, prefix) = split_path(&path_to_search);
                                
                                let mut folder_list = Vec::new();
        
                                let output = Command::new("ssh")
                                    .arg("-o").arg("ControlMaster=auto")
                                    .arg("-o").arg("ControlPath=/tmp/simple-ssh-tui-rs-%C")
                                    .arg("-o").arg("ControlPersist=5m")
                                    .arg(&host)
                                    .arg(format!("ls -p {}", parent_dir))
                                    .output();
        
                                if let Ok(out) = output {
                                    if out.status.success() {
                                        let stdout_str = String::from_utf8_lossy(&out.stdout);
                                        folder_list = stdout_str.lines()
                                            .filter(|line| line.ends_with('/'))
                                                .map(|line| line.to_string())
                                                .collect();
                                    }
                                }
        
                                let _ = tx_clone.send(folder_list);
                            });
                        }  
                     }
                 }
               },
               
               KeyCode::Down => {
                   match app.app_mode {
                       AppMode::SelectHost => {
                        let i = match app.ssh_hosts_list_state.selected() {
                            Some(i) => {
                                if i >= app.ssh_hosts.len() - 1 {
                                    0
                                }
                                else {
                                    i + 1
                                }
                            },
                            None => 0
                        };
                        app.ssh_hosts_list_state.select(Some(i));  
                       },
                       AppMode::Rsync => {
                           app.rsync_active_input = RsyncActiveInput::Remote;

                       }
                   }
              
               },
               KeyCode::Up => {
                   match app.app_mode {
                       AppMode::SelectHost => {
                            let i = match app.ssh_hosts_list_state.selected() {
                            Some(i) => {
                                if i == 0 {
                                    app.ssh_hosts.len() - 1
                                }
                                else {
                                    i - 1
                                }
                            },
                            None => 0
                        };
                        app.ssh_hosts_list_state.select(Some(i));  
                       },
                       AppMode::Rsync => {
                           app.rsync_active_input = RsyncActiveInput::Local;

                       }
                   }
            
               },
               KeyCode::Enter => {
                   if app.app_mode == AppMode::Rsync {
                       if !is_syncing {
                           is_syncing = true;
                           app.status_msg = "start syncing...".to_string();

                           run_rsync_process(app.selected_ssh_host.clone(), app.rsync_local_path.clone(), app.rsync_remote_path.clone(), app.rsync_tx.clone());
                       }
                   } else {
                       break; 
                   }
               },
               _ => {}
           }
           
       } 
       if let Some(index) = app.ssh_hosts_list_state.selected() {
           if index < app.ssh_hosts.len() {
               app.selected_ssh_host = app.ssh_hosts[index].clone()
           }
       }  
    }
    }

    restore_terminal_to_normal_mode(&mut terminal)?;
    
    start_ssh_process(app.selected_ssh_host);
    
    Ok(())
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
