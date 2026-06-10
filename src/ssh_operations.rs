use std::{ fs, io::{ BufRead, BufReader }, path::{Path, PathBuf}, process::{ Command, Stdio }, sync::mpsc::{self, Receiver, Sender}, thread, time::Instant };
use tracing::{ info, error };
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use crate::app::{StatusMsg, StatusMsgLevel::{Error, Info}};
use crate::ssh_config::SshHost;
use crate::RsyncStatus;
use crate::app::{ SshEstablishControlMaster };

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn start_ssh_process(ssh_host: SshHost) {
        info!("starting ssh");

        let mut child = Command::new("ssh");
        child.args(ssh_base_args(&ssh_host));
        child.args([
            &ssh_host.host
        ]);

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

pub fn run_rsync_process(ssh_host: SshHost, local_paht: String, remote_path: String, tx: mpsc::Sender<RsyncStatus>) {
    thread::spawn(move || {
        let destination = format!("{}:{}", ssh_host.host, remote_path);

        let homebrew_rsync_path = "/opt/homebrew/bin/rsync";
        let rsyc_binary = if Path::new(homebrew_rsync_path).exists() {
            homebrew_rsync_path
        } else {
            "rsync"
        };

        let ssh_rsh = format!("ssh {}", ssh_base_args(&ssh_host).join(" "));
        
        let child = Command::new(rsyc_binary)
            .env("RSYNC_RSH", ssh_rsh)
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

pub fn run_ls_over_ssh(ssh_host: SshHost, path_to_search: String, tx:  Sender<Vec<String>>, status_msg_tx: Sender<StatusMsg>) {
    thread::spawn(move || {
        let start = Instant::now();
        let (parent_dir, _) = split_path(&path_to_search);
        
        let mut folder_list = Vec::new();

        let output = Command::new("ssh")
            .args(ssh_base_args(&ssh_host))
            .arg(&ssh_host.host)
            .arg(format!("ls -p {}", parent_dir))
            .output();

        if let Ok(out) = output && out.status.success() {
                let stdout_str = String::from_utf8_lossy(&out.stdout);
                folder_list = stdout_str.lines()
                    .filter(|line| line.ends_with('/'))
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
        
        let _ = tx.send(folder_list);
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
        ssh_portable_pty_output_tx.send(SshEstablishControlMaster::Succsess);
        return (ssh_portable_pty_input_tx, ssh_portable_pty_output_rx);
    } else {
        remove_control_master(&ssh_host);
    }
    
    thread::spawn(move || {
        let pty_system = native_pty_system();
        let mut pair = match pty_system.openpty(PtySize {
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
        
        pair.slave.spawn_command(cmd);
    
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
                            ssh_portable_pty_output_tx.send(SshEstablishControlMaster::UserInputReqired);
                        }
                        ssh_portable_pty_output_tx.send(SshEstablishControlMaster::PasswordPromt(text));
                    },
                    Err(e) => {
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
