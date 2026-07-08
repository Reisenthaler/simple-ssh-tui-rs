use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;
use std::{ env, sync::{ mpsc }, collections::VecDeque };
use std::path::PathBuf;
use ratatui::{ widgets::ListState };
use crate::SshHost;
use crate::app::StatusMsgLevel::Info;
use crate::ssh_config::{ parse_ssh_config };
use crate::Result;

#[derive(PartialEq)]
pub enum AppMode {
    SelectHost,
    Rsync,
    SshPasswordPromt,
}

#[derive(PartialEq)]
pub enum RsyncActiveInput {
    Local,
    Remote
}


#[derive(Debug, PartialEq)]
pub enum RsyncStatus {
    Progress(String),
    Completed(Duration),
    Failed(String, Duration),
}

pub enum AppCommand {
    Quit,
    StartSsh,
}


pub enum SshEstablishControlMaster {
    UserInputReqired,
    PasswordPromt(String),
    Succsess,
    Failure,
}

pub enum StatusMsgLevel {
    Info,
    Warn,
    Error,
}
pub struct StatusMsg {
    pub level: StatusMsgLevel,
    pub msg: String,
}

pub struct PathSuggestions {
    pub folders: Vec<String>,
    pub files: Vec<String>
}

pub struct App {
    pub app_mode: AppMode,
    pub rsync_active_input: RsyncActiveInput,
    pub ssh_hosts: Vec<SshHost>,
    pub selected_ssh_host: SshHost,
    pub ssh_hosts_list_state: ListState,
    pub rsync_local_path: String,
    pub rsync_local_path_cursor_pos: usize,
    pub rsync_remote_path: String,
    pub rsync_remote_path_cursor_pos: usize,
    pub local_suggestions: PathSuggestions,
    pub remote_suggestions: PathSuggestions,
    pub status_msgs_tx: Sender<StatusMsg>,
    pub status_msgs_rx: Receiver<StatusMsg>,
    pub status_msg: StatusMsg,
    pub remote_autocomplet_tx: Sender<PathSuggestions>,
    pub remote_autocomplet_rx: Receiver<PathSuggestions>,
    pub rsync_tx: Sender<RsyncStatus>,
    pub rsync_rx: Receiver<RsyncStatus>,
    pub commands: VecDeque<AppCommand>,
    pub ssh_portable_pty_output_tx: Sender<SshEstablishControlMaster>,
    pub ssh_portable_pty_output_rx: Receiver<SshEstablishControlMaster>,
    pub ssh_portable_pty_input_tx: Sender<Vec<u8>>,
    pub ssh_portable_pty_input_rx: Receiver<Vec<u8>>,
    pub ssh_login_output: String,
    pub ssh_login_input: String,
    pub sync_active: Arc<AtomicBool>,
    pub search_query: String,
    pub rsync_path: Option<PathBuf>
}
impl App {
    pub fn get_filtered_ssh_hosts(&self) -> Vec<&SshHost>{
        self.ssh_hosts
            .iter()
            .filter(|host| {
                if self.search_query.is_empty() {
                    true
                } else {
                    match &host.host_name {
                        Some(host_name) => host.host.contains(&self.search_query) || host_name.contains(&self.search_query),
                        None => host.host.contains(&self.search_query),
                    }
                    
                }
            })
            .collect()
    }
}

pub fn init_app() -> Result<App> {
    let ssh_hosts =  parse_ssh_config()?;
    let selected_ssh_host: SshHost = ssh_hosts[0].clone();

    let mut list_state = ListState::default();
    list_state.select(Some(0));

    let (remote_autocomplet_tx, remote_autocomplet_rx) = mpsc::channel::<PathSuggestions>();
    let (rsync_tx, rsync_rx) = mpsc::channel::<RsyncStatus>();
    let (ssh_portable_pty_output_tx, ssh_portable_pty_output_rx) = mpsc::channel::<SshEstablishControlMaster>();
    let (ssh_portable_pty_input_tx, ssh_portable_pty_input_rx) = mpsc::channel::<Vec<u8>>();
    let (status_msgs_tx, status_msgs_rx) = mpsc::channel::<StatusMsg>();

    let status_msg = StatusMsg { level: Info, msg: "".to_string() };

    let local_suggestions = PathSuggestions { folders: Vec::<String>::new(), files: Vec::<String>::new() };
    let remote_suggestions = PathSuggestions { folders: Vec::<String>::new(), files: Vec::<String>::new() };
    
    Ok(App {
        app_mode: AppMode::SelectHost,
        rsync_active_input: RsyncActiveInput::Local,
        ssh_hosts: ssh_hosts,
        selected_ssh_host: selected_ssh_host,
        ssh_hosts_list_state: list_state,
        rsync_local_path: env::current_dir().unwrap().to_string_lossy().into_owned(),
        rsync_local_path_cursor_pos: env::current_dir().unwrap().to_string_lossy().len(),
        rsync_remote_path: "/".to_string(),
        rsync_remote_path_cursor_pos: 1,
        status_msgs_tx: status_msgs_tx,
        status_msgs_rx: status_msgs_rx,
        status_msg: status_msg,
        local_suggestions: local_suggestions,
        remote_suggestions: remote_suggestions,
        remote_autocomplet_tx: remote_autocomplet_tx,
        remote_autocomplet_rx: remote_autocomplet_rx,
        rsync_tx: rsync_tx,
        rsync_rx: rsync_rx,
        commands: VecDeque::<AppCommand>::new(),
        ssh_portable_pty_output_tx: ssh_portable_pty_output_tx,
        ssh_portable_pty_output_rx: ssh_portable_pty_output_rx,
        ssh_portable_pty_input_tx: ssh_portable_pty_input_tx,
        ssh_portable_pty_input_rx: ssh_portable_pty_input_rx,
        ssh_login_output: "".to_string(),
        ssh_login_input: "".to_string(),
        sync_active: Arc::new(AtomicBool::new(false)),
        search_query: "".to_string(),
        rsync_path: None,
    })
}
