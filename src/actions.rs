use std::{ fs };
use crate::{app::{App, AppCommand::{self, Quit, StartSsh}, AppMode, RsyncActiveInput}, ssh_operations};

pub enum Action {
    Quit,
    ToggleAppMode,
    MoveUp,
    MoveDown,
    Tab,
    Enter,
    Backspace,
    Input(char),
}

pub fn handle_action(mut app: &mut App, action: Action) {
    match action {
        Action::Quit => handle_quit(&mut app),
        Action::ToggleAppMode => handle_toggle_app_mode(&mut app),
        Action::Enter => handle_enter(&mut app),
        Action::MoveUp => handle_move_up(&mut app),
        Action::MoveDown => handle_move_down(&mut app),
        Action::Input(c) => handle_input(&mut app, c),
        Action::Backspace => handle_backspace(&mut app),
        Action::Tab => handle_tab(&mut app),
    }
}

fn handle_quit(app: &mut App) {
    app.commands.push_back(Quit);
}

fn handle_toggle_app_mode(app: &mut App) {
    match app.app_mode {
        AppMode::SelectHost => {
            app.app_mode = AppMode::Rsync;
            let (ssh_portable_pty_input_tx, ssh_portable_pty_output_rx) = ssh_operations::start_background_ssh(app.selected_ssh_host.clone());
            app.ssh_portable_pty_input_tx = ssh_portable_pty_input_tx;
            app.ssh_portable_pty_output_rx = ssh_portable_pty_output_rx;
        },
        AppMode::Rsync => app.app_mode = AppMode::SelectHost,
        AppMode::SshPasswordPromt => {},
    }
}

fn handle_enter(app: &mut App) {
    match app.app_mode {
        AppMode::SelectHost => {
            app.commands.push_back(StartSsh);
        },
        AppMode::Rsync => {
            ssh_operations::run_rsync_process(app.selected_ssh_host.clone(), app.rsync_local_path.clone(), app.rsync_remote_path.clone(), app.rsync_tx.clone());
        },
        AppMode::SshPasswordPromt => {
            let ssh_input_tx = &app.ssh_portable_pty_input_tx;
            match ssh_input_tx.send(Vec::<u8>::from(format!("{}\n", app.ssh_login_input).into_bytes())) {
                Ok(_) => {
                    app.ssh_login_input = "".to_string();

                    std::thread::sleep(std::time::Duration::from_millis(500));
                    if ssh_operations::check_control_master(&app.selected_ssh_host) {
                        app.app_mode = AppMode::Rsync;
                    } else {
                        app.status_msg = "failed to establisch control master".to_string();
                        app.app_mode = AppMode::SelectHost;
                    } 
                },
                Err(e) => {
                    app.status_msg = format!("error while sending user input to portable_pty: {}", e.to_string());
                }
            }
            
        },
    }
}

fn handle_move_up(app: &mut App) {
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
            set_selected_host(app);
        },
        AppMode::Rsync => {
            app.rsync_active_input = RsyncActiveInput::Local;
        },
        AppMode::SshPasswordPromt => {},
    }
}

fn handle_move_down(app: &mut App) {
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
         set_selected_host(app);
        },
        AppMode::Rsync => {
            app.rsync_active_input = RsyncActiveInput::Remote;

        },
        AppMode::SshPasswordPromt => {},
    }
}


fn set_selected_host(app: &mut App) {
    if let Some(index) = app.ssh_hosts_list_state.selected() {
        if index < app.ssh_hosts.len() {
            app.selected_ssh_host = app.ssh_hosts[index].clone()
        }
    } 
}

fn handle_input(app: &mut App, c: char) {
   match app.app_mode {
       AppMode::SelectHost => {},
       AppMode::Rsync => {
            match app.rsync_active_input {
                RsyncActiveInput::Local => app.rsync_local_path.push(c),
                RsyncActiveInput::Remote => app.rsync_remote_path.push(c),
            }
       },
       AppMode::SshPasswordPromt => {
           app.ssh_login_input.push_str(&c.to_string());
       },
    }
}

fn handle_backspace(app: &mut App) {
    match app.app_mode {
        AppMode::SelectHost => {},
        AppMode::Rsync => {
            match app.rsync_active_input {
                RsyncActiveInput::Local => { app.rsync_local_path.pop(); },
                RsyncActiveInput::Remote =>{ app.rsync_remote_path.pop(); },
            }
        },
        AppMode::SshPasswordPromt => { app.ssh_login_input.pop(); }
    }
}

fn handle_tab(app: &mut App) {
    if app.app_mode == AppMode::Rsync {
        match app.rsync_active_input {
            RsyncActiveInput::Local => {         
                let (parent_dir, prefix) = split_path(&app.rsync_local_path);
            
                let mut folder_list = Vec::<String>::new();
    
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
                if !folder_list.is_empty() {
                    if folder_list.len() == 1 {
                        let (parent_dir, _) = split_path(&app.rsync_local_path);

                        app.rsync_local_path = format!("{}{}", parent_dir, folder_list[0]);

                        app.local_suggestions.clear();
                    }
                    else {
                        app.local_suggestions = folder_list;
                    }
                }
            },
            RsyncActiveInput::Remote => { 
                let tx_clone = app.remote_autocomplet_tx.clone();
                let ssh_host = app.selected_ssh_host.clone();
                let path_to_search = app.rsync_remote_path.clone();

                ssh_operations::run_ls_over_ssh(ssh_host, path_to_search, tx_clone);
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
