use std::{ io::{ BufRead, BufReader, Stdout }, path::Path, process::{ Command, Stdio }, sync::mpsc, thread };
use tracing::{ info, error };
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::{ 
    Terminal, 
    backend::{ CrosstermBackend }
    };
use crate::terminal::restore_terminal_to_normal_mode;
use crate::ssh_config::SshHost;
use crate::RsyncStatus;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn start_ssh_process(ssh_host: SshHost) {
        info!("starting ssh");

        let mut child = Command::new("ssh");
        child.args(ssh_base_args());
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

        let ssh_rsh = format!("ssh {}", ssh_base_args().join(" "));
        
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



pub fn start_background_ssh(ssh_host: SshHost, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    let host = ssh_host.host.clone();

    // master connection exists
    let check = Command::new("ssh")
        .args(ssh_base_args())
        .args([
            "-O",
            "check",
            &host,
        ])
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .status();

    if matches!(check, Ok(status) if status.success()) {
        return Ok(());
    }

    // try non-interactive login
    let non_interactive_login = Command::new("ssh")
        .args(ssh_base_args())
        .args([
            "-o", "PasswordAuthentication=no",
            "-o", "KbdInteractiveAuthentication",
            "-o", "ChallengeResponseAuthentication",
            "-o", "BatchMode=yes",
            "-o", "ConnectTimeout=10",
            "-o", "ExitOnForwardFailure=yes",
            "-MNf",
            &host,
        ])
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !non_interactive_login {
        disable_raw_mode()?;
        restore_terminal_to_normal_mode(terminal)?;
        println!("Please complete SSH login in the terminal");
   
    
        // try interactiv login
        let status = Command::new("ssh")
            .args(ssh_base_args())
            .args([
                "-o", "ExitOnForwardFailure=yes",
                "-MNf",
                &host,
            ])
            .stdin(Stdio::inherit())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status();

        let _ = enable_raw_mode()?;
   
    
        let status = status?;

        if !status.success() {
            return Err("failed to start ssh master connection".into());
        }
    }
    
    let verify = Command::new("ssh")
        .args(ssh_base_args())
        .args([
            "-O",
            "check",
            &host,
        ])
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .status()?;

    if !verify.success() {
        return Err("ssh master connection did not start".into());
    }
    Ok(())
}


fn ssh_base_args() -> Vec<&'static str> {
    vec![
        "-o", "ControlMaster=auto",
        "-o", "ControlPath=/tmp/simple-ssh-tui-rs-%C",
        "-o", "ControlPersist=90m",
        "-o", "ConnectTimeout=15",
        "-o", "ConnectionAttempts=2",
        "-o", "ServerAliveInterval=30",
        "-o", "ServerAliveCountMax=3",
        "-o", "Compression=yes",
        "-o", "IPQoS=throughput",
        "-o", "StrictHostKeyChecking=accept-new",
    ]
}
