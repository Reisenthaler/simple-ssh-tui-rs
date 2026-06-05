use std::fmt::format;
use std::mem::transmute;
use std::os::unix::process;
use std::path::Path;
use std::process::Stdio;
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
mod ui;
use ssh_config::{ parse_ssh_config };
use ui::draw_ui;

use crate::ssh_config::SshHost;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(PartialEq)]
enum AppMode {
    SelectHost,
    Rsync
}

#[derive(PartialEq)]
enum RsyncActiveInput {
    Left,
    Right
}


#[derive(Debug, PartialEq)]
enum RsyncStatus {
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
    let mut rsync_active_input = RsyncActiveInput::Left;
    // get current path
    let mut rsync_local_path: String = env::current_dir()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut rsync_remote_path = String::new();

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
                            AppMode::SelectHost => app_mode = AppMode::Rsync,
                            AppMode::Rsync => app_mode = AppMode::SelectHost,
                        }
               },
               KeyCode::Left => {
                   if app_mode == AppMode::Rsync {
                       rsync_active_input = RsyncActiveInput::Left;
                   }
               },
               KeyCode::Right => {
                   if app_mode == AppMode::Rsync {
                       rsync_active_input = RsyncActiveInput::Right;
                   }
               },
               KeyCode::Char(c) => {
                   if app_mode == AppMode::Rsync {
                       match rsync_active_input {
                           RsyncActiveInput::Left => rsync_local_path.push(c),
                           RsyncActiveInput::Right => rsync_remote_path.push(c),
                       }
                   }  
               },
               KeyCode::Backspace => {
                   if app_mode == AppMode::Rsync {
                       match rsync_active_input {
                           RsyncActiveInput::Left => rsync_local_path.pop(),
                           RsyncActiveInput::Right => rsync_remote_path.pop(),
                       };
                   }   
               },
               KeyCode::Tab => {
                 if app_mode == AppMode::Rsync && !is_fetching {
                     match rsync_active_input {
                        RsyncActiveInput::Left => {
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
                        RsyncActiveInput::Right => {  
                             is_fetching = true;

                            let tx_clone = tx.clone();
                            let host = selected_ssh_host.host.clone();
                            let path_to_search = rsync_remote_path.clone();
        
                            thread::spawn(move || {
        
                                let (parent_dir, prefix) = split_path(&path_to_search);
                                
                                let mut folder_list = Vec::new();
        
                                let output = Command::new("ssh")
                                    .arg("-o").arg("ControlMaster=auto")
                                    .arg("-o").arg("ControlPath=/tmp/tssh-%r@%h:%p")
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
               KeyCode::Up => {
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

fn start_ssh_process(ssh_host: SshHost) {
        info!("starting ssh");

        let mut child = Command::new("ssh");
        child.arg(ssh_host.host);

        child.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        match child.spawn() {
            Ok(mut process) => {
                let _ = process.wait();
            },
            Err(e) => {
                error!("starting ssh failed: {}", e);
            }
            
        }
}

fn run_rsync_process(ssh_host: SshHost, local_paht: String, remote_path: String, tx: mpsc::Sender<RsyncStatus>) {
    thread::spawn(move || {
        let destination = format!("{}:{}", ssh_host.host, remote_path);

        let homebrew_rsync_path = "/opt/homebrew/bin/rsync";
        let rsyc_binary = if Path::new(homebrew_rsync_path).exists() {
            homebrew_rsync_path
        } else {
            "rsync"
        };
        
        let mut child = Command::new(rsyc_binary)
            .arg("-avz")
            .arg("--no-perms")
            .arg("--no-owner")
            .arg("--no-group")
            .arg("--info=progress2")
            .arg(&local_paht)
            .arg(&destination)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(RsyncStatus::Failed(e.to_string()));
                return;
            }
        };

        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(std::result::Result::ok)  {
                let trimmed = line.trim();

                if trimmed.contains('%') {
                    let cleaned_progress = trimmed
                        .split_whitespace()
                        .collect::<Vec<&str>>()
                        .join(" ");

                    let _ = tx.send(RsyncStatus::Progress(cleaned_progress));
                }
                
            }
        }


        match child.wait() {
            Ok(status) if status.success() => {
                let _ = tx.send(RsyncStatus::Completed(status));
            },
            _ => {
                let _ = tx.send(RsyncStatus::Failed("Rsync failed with an error".to_string()));
            }
            
            
        }

    });
}



fn setup_terminal(ssh_hosts_count: usize) -> std::result::Result<Terminal<CrosstermBackend<Stdout>>, std::io::Error> {
    
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    
       execute!(stdout, EnableMouseCapture)?;

       let backend = CrosstermBackend::new(stdout);

       let terminal = Terminal::with_options(backend, 
           TerminalOptions {
               viewport: Viewport::Inline((ssh_hosts_count + 8).try_into().unwrap()),
       });

       return terminal;
}

fn restore_terminal_to_normal_mode(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()>{
    terminal.clear()?;
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), DisableMouseCapture)?;
    terminal.show_cursor()?;   

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
