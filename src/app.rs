use std::sync::mpsc::{Receiver, Sender};
use std::{ env, sync::mpsc };
use ratatui::{ widgets::ListState };
use crate::SshHost;
use crate::ssh_config::{ parse_ssh_config };
use crate::Result;

#[derive(PartialEq)]
pub enum AppMode {
    SelectHost,
    Rsync
}

#[derive(PartialEq)]
pub enum RsyncActiveInput {
    Local,
    Remote
}


#[derive(Debug, PartialEq)]
pub enum RsyncStatus {
    Progress(String),
    Completed(std::process::ExitStatus),
    Failed(String),
}
pub struct App {
    pub app_mode: AppMode,
    pub rsync_active_input: RsyncActiveInput,
    pub ssh_hosts: Vec<SshHost>,
    pub selected_ssh_host: SshHost,
    pub ssh_hosts_list_state: ListState,
    pub rsync_local_path: String,
    pub rsync_remote_path: String,
    pub local_suggestions: Vec<String>,
    pub remote_suggestions: Vec<String>,
    pub status_msg: String,
    pub remote_autocomplet_tx: Sender<Vec<String>>,
    pub remote_autocomplet_rx: Receiver<Vec<String>>,
    pub rsync_tx: Sender<RsyncStatus>,
    pub rsync_rx: Receiver<RsyncStatus>,
}


pub fn init_app() -> Result<App> {
    let ssh_hosts =  parse_ssh_config()?;
    let selected_ssh_host: SshHost = ssh_hosts[0].clone();

    let mut list_state = ListState::default();
    list_state.select(Some(0));

    let (remote_autocomplet_tx, remote_autocomplet_rx) = mpsc::channel::<Vec<String>>();
    let (rsync_tx, rsync_rx) = mpsc::channel::<RsyncStatus>();

    Ok(App {
        app_mode: AppMode::SelectHost,
        rsync_active_input: RsyncActiveInput::Local,
        ssh_hosts: ssh_hosts,
        selected_ssh_host: selected_ssh_host,
        ssh_hosts_list_state: list_state,
        rsync_local_path: env::current_dir().unwrap().to_string_lossy().into_owned(),
        rsync_remote_path: "/".to_string(),
        status_msg: "".to_string(),
        local_suggestions: Vec::<String>::new(),
        remote_suggestions: Vec::<String>::new(),
        remote_autocomplet_tx: remote_autocomplet_tx,
        remote_autocomplet_rx: remote_autocomplet_rx,
        rsync_tx: rsync_tx,
        rsync_rx: rsync_rx,
    })
}
