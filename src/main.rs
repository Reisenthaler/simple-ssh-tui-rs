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

mod ssh_config;
mod ssh_operations;
mod ui;
mod terminal;
use ssh_config::{ parse_ssh_config };
use ui::draw_ui;
use ssh_operations::{ start_ssh_process,  run_rsync_process, start_background_ssh };
use terminal::{ setup_terminal, restore_terminal_to_normal_mode };
use crate::ssh_config::SshHost;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(PartialEq)]
enum AppMode {
    SelectHost,
    Rsync
}

#[derive(PartialEq)]
enum RsyncActiveInput {
    Local,
    Remote
}


#[derive(Debug, PartialEq)]
pub enum RsyncStatus {
    Progress(String),
    Completed(std::process::ExitStatus),
    Failed(String),
}

fn main() -> Result<()> {
    beautiful_log::init_logging("INFO");
    
    let ssh_hosts = parse_ssh_config()?;
    
    let mut list_state = ListState::default();
    list_state.select(Some(0));

    let mut selected_ssh_host: SshHost = ssh_hosts[0].clone();

    let mut app_mode = AppMode::SelectHost;
    let mut rsync_active_input = RsyncActiveInput::Local;
    // get current path
    let mut rsync_local_path: String = env::current_dir()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut rsync_remote_path = "/".to_string();

    let mut local_suggestions = Vec::<String>::new();
    let mut remote_suggestions = Vec::<String>::new();
    let mut is_fetching = false;

    let (tx, rx) = mpsc::channel::<Vec<String>>();

    let (rsync_tx, rsync_rx) = mpsc::channel::<RsyncStatus>();
    let mut is_syncing = false;
    let mut sync_message = String::new();
    sync_message = "no sync started".to_string();
    
    let mut terminal = setup_terminal(ssh_hosts.len())?;
       
    loop {
        draw_ui(&mut terminal, &ssh_hosts, selected_ssh_host.clone(), &mut list_state, 
            &app_mode, &rsync_active_input, &mut rsync_local_path,  &mut rsync_remote_path,
            is_fetching, &mut local_suggestions, &mut remote_suggestions, &sync_message);

        if let Ok(all_folders) = rx.try_recv() {
            let (_, prefix) = split_path(&rsync_remote_path);
            
            remote_suggestions = all_folders
                .into_iter()
                .filter(|folder| folder.to_lowercase().starts_with(&prefix.to_lowercase()))
                .collect();
            
            is_fetching = false;
        }

        if let Ok(rsync_status) = rsync_rx.try_recv() {
            match rsync_status {
                RsyncStatus::Progress(progress_msg) => {
                  sync_message = progress_msg;  
                },
                RsyncStatus::Completed(exit_status) => {
                    sync_message = format!("rsync finished with status: {}", exit_status);
                },
                RsyncStatus::Failed(err_msg) => {
                    sync_message = format!("rsync failed with error: {}", err_msg);
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
                        match app_mode {
                            AppMode::SelectHost => {
                                app_mode = AppMode::Rsync;
                                    start_background_ssh(selected_ssh_host.clone(), ssh_hosts.len(), &mut terminal);
                            },
                            AppMode::Rsync => app_mode = AppMode::SelectHost,
                        }
               },
               KeyCode::Char(c) => {
                   if app_mode == AppMode::Rsync {
                       match rsync_active_input {
                           RsyncActiveInput::Local => rsync_local_path.push(c),
                           RsyncActiveInput::Remote => rsync_remote_path.push(c),
                       }
                   }  
               },
               KeyCode::Backspace => {
                   if app_mode == AppMode::Rsync {
                       match rsync_active_input {
                           RsyncActiveInput::Local => rsync_local_path.pop(),
                           RsyncActiveInput::Remote => rsync_remote_path.pop(),
                       };
                   }   
               },
               KeyCode::Tab => {
                 if app_mode == AppMode::Rsync && !is_fetching {
                     match rsync_active_input {
                        RsyncActiveInput::Local => {
                            let (parent_dir, prefix) = split_path(&rsync_local_path);
                            
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

                            local_suggestions = folder_list;
                            
                        },
                        RsyncActiveInput::Remote => {  
                             is_fetching = true;

                            let tx_clone = tx.clone();
                            let host = selected_ssh_host.host.clone();
                            let path_to_search = rsync_remote_path.clone();
        
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
                   match app_mode {
                       AppMode::SelectHost => {
                        let i = match list_state.selected() {
                            Some(i) => {
                                if i >= ssh_hosts.len() - 1 {
                                    0
                                }
                                else {
                                    i + 1
                                }
                            },
                            None => 0
                        };
                        list_state.select(Some(i));  
                       },
                       AppMode::Rsync => {
                           rsync_active_input = RsyncActiveInput::Remote;

                       }
                   }
              
               },
               KeyCode::Up => {
                   match app_mode {
                       AppMode::SelectHost => {
                            let i = match list_state.selected() {
                            Some(i) => {
                                if i == 0 {
                                    ssh_hosts.len() - 1
                                }
                                else {
                                    i - 1
                                }
                            },
                            None => 0
                        };
                        list_state.select(Some(i));  
                       },
                       AppMode::Rsync => {
                           rsync_active_input = RsyncActiveInput::Local;

                       }
                   }
            
               },
               KeyCode::Enter => {
                   if app_mode == AppMode::Rsync {
                       if !is_syncing {
                           is_syncing = true;
                           sync_message = "Syncing...".to_string();

                           run_rsync_process(selected_ssh_host.clone(), rsync_local_path.clone(), rsync_remote_path.clone(), rsync_tx.clone());
                       }
                   } else {
                       break; 
                   }
               },
               _ => {}
           }
           
       } 
       if let Some(index) = list_state.selected() {
           if index < ssh_hosts.len() {
               selected_ssh_host = ssh_hosts[index].clone()
           }
       }  
    }
    }

    restore_terminal_to_normal_mode(&mut terminal)?;
    
    start_ssh_process(selected_ssh_host);
    
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
