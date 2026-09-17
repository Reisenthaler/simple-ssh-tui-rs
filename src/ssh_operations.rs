use std::{ 
    fs, 
    io::{ BufRead, BufReader }, 
    path::{ PathBuf}, 
    process::{ Command, Stdio }, 
    sync::{ mpsc::{self, Receiver, Sender}, Arc, atomic::{ AtomicBool, Ordering } }, 
    thread, 
    time::{ Instant, Duration }
};
use tracing::{ info, error };
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use crate::app::{
    PathSuggestions, 
    RemoteLsResult, 
    SshEstablishControlMaster::Failure, 
    StatusMsg, 
    StatusMsgLevel::{Error, Info} 
};
use crate::ssh_config::SshHost;
use crate::RsyncStatus;
use crate::app::{ SshEstablishControlMaster };

#[derive(Clone, Copy)]
pub enum TransferDirection {
    Download,
    Upload
}

const SSH_CONNECTION_ERROR: i32 = 255;

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}


fn tmux_session_name(seed: u64) -> String {
   format!("tui-{}", seed)
}

fn build_ssh_command(ssh_host: &SshHost, seed: u64) -> Command {
    let session_name = tmux_session_name(seed);

    let tmux_args: Vec<String> = vec![    
        "set-option".into(), "-g".into(), "prefix".into(), "C-a".into(), "\\;".into(),
        "bind".into(), "C-a".into(), "send-prefix".into(), "\\;".into(),
        "unbind".into(), "C-b".into(), "\\;".into(),

        // --- quickly enabeling mouse or allowing copying
        "bind".into(), "m".into(), "set-option -g mouse ; display-message \"Mouse: #{?mouse,ON,OFF}\"".into(), "\\;".into(),

        // --- latency / nested tmux ---
        "set-option".into(), "-sg".into(), "escape-time".into(), "0".into(), "\\;".into(),
        "set-option".into(), "-g".into(), "focus-events".into(), "on".into(), "\\;".into(),
        "set-window-option".into(), "-g".into(), "aggressive-resize".into(), "on".into(), "\\;".into(),
        "set-option".into(), "-g".into(), "allow-passthrough".into(), "on".into(), "\\;".into(),

        // --- color ---
        "set-option".into(), "-g".into(), "default-terminal".into(), "tmux-256color".into(), "\\;".into(),
        "set-option".into(), "-ga".into(), "terminal-overrides".into(), ",*256col*:Tc".into(), "\\;".into(),

        // --- ergonomics ---
        "set-option".into(), "-g".into(), "history-limit".into(), "50000".into(), "\\;".into(),
        "set-option".into(), "-g".into(), "mouse".into(), "on".into(), "\\;".into(),
        "set-option".into(), "-g".into(), "set-clipboard".into(), "on".into(), "\\;".into(),
        "set-option".into(), "-g".into(), "exit-empty".into(), "on".into(), "\\;".into(),

        // --- inactivity cleanup (3h after last client detaches) ---
        "set-option".into(), "-g".into(), "destroy-unattached".into(), "off".into(), "\\;".into(),
        "set-hook".into(), "-g".into(), "client-detached".into(),
        format!(
            "run-shell -b \"sleep 10800; tmux has-session -t {n} 2>/dev/null && tmux list-clients -t {n} 2>/dev/null | grep -q . || tmux kill-session -t {n}\"",
            n = session_name
        ),
        "\\;".into(),

        // --- the actual session ---
        "new-session".into(), "-A".into(), "-s".into(), session_name.clone(),
    ];
    
    let remote_tmux_command = tmux_args
        .iter()
        .map(|arg| 
            if arg == "\\;" { 
                arg.clone() 
            } else {
            shell_quote(arg)
            })
        .collect::<Vec<_>>()
        .join(" ");

    let inner = format!(        
        "if command -v tmux >/dev/null 2>&1; then exec env -u TMUX tmux -u {}; else exec \"$SHELL\" -l; fi",
        remote_tmux_command
    );

    let remote_command = format!("sh -c {}", shell_quote(&inner));
    info!("remote_command: {}", remote_command);
    
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_base_args(ssh_host));
    cmd.arg("-tt"); // force pty
    cmd.arg(&ssh_host.host);
    cmd.arg("--");
    cmd.arg(remote_command);
    cmd.stdin(Stdio::inherit()) 
        .stdout(Stdio::inherit()) 
        .stderr(Stdio::inherit());

    cmd
}

pub fn start_ssh_process(ssh_host: SshHost, seed: u64) {
        info!("starting ssh");

        loop {
            let connection_attemt_start =  Instant::now();

            let status = match build_ssh_command(&ssh_host, seed).spawn() {
                Ok(mut child) => child.wait(),
                Err(e) => {
                    error!("failed to spawn ssh process: {}", e);
                    return;
                }
            };

            match status {
                Ok(status) if status.code().unwrap_or(-1) == SSH_CONNECTION_ERROR => {
                    info!("ssh exited with code: {} -> reconnect", status.code().unwrap_or(-1));
                },
                Ok(status) => {
                    info!("ssh exited with code: {} -> don't reconnect", status.code().unwrap_or(-1));
                    return;
                },
                Err(e) => {
                    error!("waiting for ssh failed: {}", e);
                    return;
                }
            }

            thread::sleep(Duration::from_secs(30).saturating_sub(connection_attemt_start.elapsed()));
        }
}

pub fn run_rsync_process(rsync_path: Option<PathBuf>, ssh_host: SshHost, local_path: String, remote_path: String, transfer_direction: TransferDirection, tx: mpsc::Sender<RsyncStatus>) {
    thread::spawn(move || {
        run_rsync(rsync_path.clone(), ssh_host, local_path, remote_path, transfer_direction, tx);
    });
}

pub fn run_rsync_proccess_continuously(rsync_path: Option<PathBuf>, ssh_host: SshHost, local_path: String, remote_path: String, transfer_direction: TransferDirection, tx: mpsc::Sender<RsyncStatus>, sync_active: Arc<AtomicBool>) {
    thread::spawn(move || {
        
        while sync_active.load(Ordering::Relaxed) {
            let start_time = Instant::now();
            run_rsync(rsync_path.clone(), ssh_host.clone(), local_path.clone(), remote_path.clone(), transfer_direction, tx.clone());        
            
            while start_time.elapsed() < Duration::from_secs(3) {
                thread::sleep(Duration::from_millis(10));
            }
        }
    });
}

fn run_rsync(rsync_path: Option<PathBuf>, ssh_host: SshHost, local_path: String, remote_path: String, transfer_direction: TransferDirection, tx: mpsc::Sender<RsyncStatus>) {
        let start_time = Instant::now();

        let rsyc_binary = if let Some(path) = rsync_path {
            &path.to_string_lossy().to_string()
        } else {
            "rsync"
        };

        let ssh_rsh = format!("ssh {} -o BatchMode=yes", ssh_base_args(&ssh_host).join(" "));

        let (source, destination) = match transfer_direction {
            TransferDirection::Download => {
                (format!("{}:{}", ssh_host.host, remote_path), local_path)
            },
            TransferDirection::Upload => {
                (local_path, format!("{}:{}", ssh_host.host, remote_path))
            }
        };

        
        let child = Command::new(rsyc_binary)
            .env("RSYNC_RSH", ssh_rsh)
            .arg("-avz")
            .arg("--no-perms")
            .arg("--no-owner")
            .arg("--no-group")
            .arg("--info=progress2")
            .arg(&source)
            .arg(&destination)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(RsyncStatus::Failed(e.to_string(), start_time.elapsed()));
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
                let _ = tx.send(RsyncStatus::Completed(start_time.elapsed()));
            },
            Ok(status) => {
                let _ = tx.send(RsyncStatus::Failed(format!("Rsync failed: {}", rsync_exit_msg(status.code().unwrap_or(100))), start_time.elapsed()));
            },
            _ => {
                let _ = tx.send(RsyncStatus::Failed("Rsync failed with an error, failed to get exit code".to_string(), start_time.elapsed()));
            }   
        }
}

pub fn run_ls_over_ssh(ssh_host: SshHost, path_to_search: String, tx:  Sender<RemoteLsResult>, status_msg_tx: Sender<StatusMsg>) {
    thread::spawn(move || {
        let start = Instant::now();
        let (parent_dir, _) = split_path(&path_to_search);
        
        let mut folder_list = Vec::new();
        let mut file_list = Vec::new();

        let output = Command::new("ssh")
            .args(ssh_base_args(&ssh_host))
            .args([
                "-o", 
                "BatchMode=yes",
            ])
            .arg(&ssh_host.host)
            .arg(format!("ls -Ap {}", parent_dir))
            .output();

        if let Ok(out) = output && out.status.success() {
                let stdout_str = String::from_utf8_lossy(&out.stdout);

                folder_list = stdout_str.lines()
                    .filter(|line| line.ends_with('/'))
                    .filter_map(|line| line.strip_suffix('/').map(|striped| striped.to_string()))
                        .collect();

                file_list = stdout_str.lines()
                    .filter(|line| !line.ends_with('/'))
                        .map(|line| line.to_string())
                        .collect();
                
            match status_msg_tx.send(StatusMsg { level: Info, msg: format!("getting remote folders took: {}ms", start.elapsed().as_millis()) }) {
                Ok(_) => {
                    info!("sending status msg: \"{}\" was succsesfull", format!("getting remote folders took: {}ms", start.elapsed().as_millis()));
                },
                Err(e) => {
                    error!("sending status msg: \"{}\" failed with: {}", format!("getting remote folders took: {}ms", start.elapsed().as_millis()), e);
                }
            } 
        }
        else {
            match status_msg_tx.send(StatusMsg { level: Error, msg: format!("getting remote folders failed after: {}ms", start.elapsed().as_millis()) }) {
                Ok(_) => {
                    info!("sending status msg: \"{}\" was succsesfull", format!("getting remote folders failed after: {}ms", start.elapsed().as_millis()));
                },
                Err(e) => {
                    error!("sending status msg: \"{}\" failed with: {}", format!("getting remote folders failed after: {}ms", start.elapsed().as_millis()), e);
                }
            } 
        }
        info!("remote suggestions: folders {:?} files {:?}",folder_list, file_list);
        let _ = tx.send(RemoteLsResult { path: path_to_search, suggestions: PathSuggestions { folders: folder_list, files: file_list } });
    });
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

pub fn start_background_ssh(ssh_host: SshHost) -> (Sender<Vec<u8>>, Receiver<SshEstablishControlMaster>) {
    let host = ssh_host.host.clone();
    let (ssh_portable_pty_output_tx, ssh_portable_pty_output_rx) = mpsc::channel::<SshEstablishControlMaster>();
    let (ssh_portable_pty_input_tx, ssh_portable_pty_input_rx) = mpsc::channel::<Vec<u8>>();

    if check_control_master(&ssh_host) {
        if let Err(e) = ssh_portable_pty_output_tx.send(SshEstablishControlMaster::Succsess) {
            error!("failed to send msg on mpsc channel: {}", e)
        }
        return (ssh_portable_pty_input_tx, ssh_portable_pty_output_rx);
    } else {
        remove_control_master(&ssh_host);
    }
    
    thread::spawn(move || {
        let pty_system = native_pty_system();
        let pair = match pty_system.openpty(PtySize {
            rows: 24, 
            cols: 80, 
            pixel_width: 0, 
            pixel_height: 0
        })
        {
            Ok(p) => p,
            Err(e) => { 
                error!("error creating portable_pty: {}", e);
                return;
            },
        };
    
        let mut ssh_args = ssh_base_args(&ssh_host);
        ssh_args.extend([
            "-o".to_string(), "ExitOnForwardFailure=yes".to_string(),
            "-MN".to_string(),
            host,
        ]);
        
        let mut cmd = CommandBuilder::new("ssh");
        cmd.args(ssh_args);
        
        if let Err(e) =  pair.slave.spawn_command(cmd) {
            error!("failed to spawn ssh command on portable_pty: {}", e);
            if let Err(e) = ssh_portable_pty_output_tx.send(Failure) {
                error!("failed to send failure msg on mpsc channel: {}", e);
            }
        
            return;
        }
    
        let mut reader =  match pair.master.try_clone_reader() {
            Ok(p) => p,
            Err(e) => { 
                error!("error getting reader from portable_pty: {}", e);
                return;
            },
        };

        thread::spawn(move || {
            loop {
                let mut buf = [0u8; 4096];
                match reader.read(&mut buf) {
                    Ok(0) => {
                        break;
                    },
                    Ok(n) => {
                        let text: String = String::from_utf8_lossy(&buf[..n]).to_string();
                        if contains_paasswd_promt(&text) {
                            if let Err(e) = ssh_portable_pty_output_tx.send(SshEstablishControlMaster::UserInputReqired) {
                                error!("failed to send msg on mpsc channel: {}", e)
                            }
                        }
                        if let Err(e) = ssh_portable_pty_output_tx.send(SshEstablishControlMaster::PasswordPromt(text)) {
                            error!("failed to send msg on mpsc channel: {}", e)
                        }
                    },
                    Err(e) => {
                        error!("failed read from portable_pty reader: {}", e);
                        break;
                    }
                } 
            }  
        });

        let mut writer = match pair.master.take_writer() {
            Ok(writ) => writ,
            Err(e) => { 
                error!("error getting reader from portable_pty: {}", e);
                return;
            },
        };

        while let Ok(msg) = ssh_portable_pty_input_rx.recv() {
                    let _ = writer.write_all(&msg);
                    let _ = writer.flush();
        }
    
    });

    (ssh_portable_pty_input_tx, ssh_portable_pty_output_rx)
}

pub fn check_control_master(ssh_host: &SshHost) -> bool {
    let control_path = control_path(&ssh_host);
    if !control_path.exists() {
        return false;
    }

    let output = Command::new("ssh")
        .args([
            "-o", &format!("ControlPath={}", control_path.display()),
            "-O", "check",
            &ssh_host.host,
        ])
        .output();

    match output {
        Ok(output) => {
            output.status.success()
        },
        Err(_) => false
    }
}

fn remove_control_master(ssh_host: &SshHost) {
    let control_path = control_path(&ssh_host);

    match fs::remove_file(&control_path) {
        Ok(_) => {
           info!("succsesfully removed control master file: {}", &control_path.display()); 
        },
        Err(e) => {
            error!("error wile removing control master file: {}   error: {}", &control_path.display(), e);
        }
    }
}

fn ssh_base_args(ssh_host: &SshHost) -> Vec<String> {

    let control_master = control_path(&ssh_host.clone()).to_string_lossy().to_string();
    vec![
        "-o".to_string(), "ControlMaster=auto".to_string(),
        "-o".to_string(), format!("ControlPath={}", control_master),
        "-o".to_string(), "ControlPersist=90m".to_string(),
        "-o".to_string(), "ConnectTimeout=15".to_string(),
        "-o".to_string(), "ConnectionAttempts=2".to_string(),
        "-o".to_string(), "ServerAliveInterval=30".to_string(),
        "-o".to_string(), "ServerAliveCountMax=3".to_string(),
        "-o".to_string(), "Compression=yes".to_string(),
        "-o".to_string(), "IPQoS=throughput".to_string(),
        "-o".to_string(), "StrictHostKeyChecking=accept-new".to_string(),
    ]
}

fn control_path(ssh_host: &SshHost) -> PathBuf {
    let host = &ssh_host.host;

    PathBuf::from(format!("/tmp/simple-ssh-tui-rs-{}", host.replace(":", "-")))
}


fn contains_paasswd_promt(text: &String) -> bool {
    text.contains("pass") || text.contains("Pass")
}


fn rsync_exit_msg(exit_code: i32) -> &'static str {
    match exit_code {
        0 => "Success",
        1 => "Syntax or usage error",
        2 => "Protocol incompatibility",
        3 => "Errors selecting input/output files, dirs",
        4 => "Requested action not supported",
        5 => "Error starting client-server protocol",
        6 => "Daemon unable to append to log-file",
        10 => "Error in socket I/O",
        11 => "Error in file I/O",
        12 => "Error in rsync protocol data stream",
        13 => "Errors with program diagnostics",
        14 => "Error in IPC code",
        20 => "Received SIGUSR1 or SIGINT",
        21 => "Some error returned by waitpid()",
        22 => "Error allocating core memory buffers",
        23 => "Partial transfer due to error",
        24 => "Partial transfer due to vanished source files",
        25 => "The --max-delete limit stopped deletions",
        30 => "Timeout in data send/receive",
        35 => "Timeout waiting for daemon connection",
        _ => "Unknown rsync exit code",
    }
}
