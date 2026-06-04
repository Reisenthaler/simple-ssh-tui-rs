use std::{ fs, io::{ Error, ErrorKind, } };
use tracing::{ debug, error };

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone)]
pub struct SshHost {
    pub(crate) host: String,
    pub(crate) host_name: Option<String>,
    pub(crate) port: Option<String>,
    pub(crate) user: Option<String>,
    pub(crate) proxy_jump: Option<String>
}

pub fn parse_ssh_config() -> Result<Vec<SshHost>> {
    let mut ssh_hosts = Vec::<SshHost>::new();
    let raw_ssh_config_result = get_ssh_config().unwrap();

    let mut current_ssh_host: SshHost = SshHost 
        { 
            host: "*".to_string(), 
            host_name: None, 
            port: None, 
            user: None, 
            proxy_jump: None 
        };
    
    for line in raw_ssh_config_result.lines() {
        let trimmed_line = line.trim();
        if trimmed_line.starts_with('#') || trimmed_line.is_empty() {
            continue;
        }

        debug!(trimmed_line);
        
        let parts: Vec<&str> = trimmed_line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        debug!("part[0]: {:?}", parts[0]);
        debug!("part[1]: {:?}", parts[1]);
        debug!("current_ssh_host: {:?}", current_ssh_host);
        
        match parts[0].to_lowercase().as_str() {
            "host" => {
                if current_ssh_host.host != parts[1] && current_ssh_host.host != "*" {
                    ssh_hosts.push(current_ssh_host.clone());
                }

                current_ssh_host.host = parts[1].to_string();
            },
            "hostname" => {
                current_ssh_host.host_name = Some(parts[1].to_string());

            }, 
            "port" => {
                current_ssh_host.port = Some(parts[1].to_string());

            }, 
            "user" => {
                current_ssh_host.user = Some(parts[1].to_string());

            },
            "proxyjump" => {
                current_ssh_host.proxy_jump = Some(parts[1].to_string());

            },
            _ => {
                debug!("unhandeled key in ssh config: {:?}", parts);
            }
            
        }
        
    }

    Ok(ssh_hosts)
}

fn get_ssh_config() -> std::result::Result<String, std::io::Error> {  
    let home_dir = dirs::home_dir();

    match home_dir {
        Some(home_dir) => {
            let ssh_config_path = home_dir.join(".ssh/config");

           return fs::read_to_string(ssh_config_path);
        },
        None => {
            error!("Failed to get home dir");
            return Err(Error::new(ErrorKind::NotFound, "Failed to get home dir"));
        }
        
    }
}
