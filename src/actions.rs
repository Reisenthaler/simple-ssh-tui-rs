use std::{ process::Command, thread, fs };
use crate::app::{App, AppCommand::{self, Quit, StartRsync, StartSsh}, AppMode, RsyncActiveInput};

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
            app.commands.push_front(AppCommand::StartSshControlMaster(app.selected_ssh_host.clone()));
        },
        AppMode::Rsync => app.app_mode = AppMode::SelectHost,
    }
}

fn handle_enter(app: &mut App) {
    match app.app_mode {
        AppMode::SelectHost => {
            app.commands.push_back(StartSsh);
        },
        AppMode::Rsync => {
            app.commands.push_back(StartRsync);
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

        }
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

        }
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
    if app.app_mode == AppMode::Rsync {
        match app.rsync_active_input {
            RsyncActiveInput::Local => app.rsync_local_path.push(c),
            RsyncActiveInput::Remote => app.rsync_remote_path.push(c),
        }
    }
}

fn handle_backspace(app: &mut App) {
    if app.app_mode == AppMode::Rsync {
        match app.rsync_active_input {
            RsyncActiveInput::Local => { app.rsync_local_path.pop(); },
            RsyncActiveInput::Remote =>{ app.rsync_remote_path.pop(); },
        }
    }
}

fn handle_tab(app: &mut App) {
    if app.app_mode == AppMode::Rsync {
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
                let tx_clone = app.remote_autocomplet_tx.clone();
                let host = app.selected_ssh_host.host.clone();
                let path_to_search = app.rsync_remote_path.clone();

                thread::spawn(move || {
        
                    let (parent_dir, _) = split_path(&path_to_search);
                    
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
