use std::{ fmt::format, fs, io::{ Error, ErrorKind, }, os::unix::process::CommandExt, path::Path, process::Command };
use crossterm::{
    event::{ self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode }, 
    
    execute, 
    terminal::{disable_raw_mode, enable_raw_mode}};
use serde::{ Deserialize, Serialize,  };
use beautiful_log;
use tracing::{ debug, error, info, warn };
use ratatui::{ 
    Terminal, TerminalOptions, Viewport, backend::{ Backend, CrosstermBackend }, layout::{ Constraint, Direction, Layout }, macros::ratatui_core::backend, style::{Modifier, Style, Stylize}, widgets::{ Block, Borders, List, ListItem, ListState, Paragraph }
    };

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone)]
struct SshHost {
    host: String,
    host_name: Option<String>,
    port: Option<String>,
    user: Option<String>,
    proxy_jump: Option<String>
}

fn main() -> Result<()> {
    beautiful_log::init_logging("INFO");
    let ssh_hosts_result =  parse_ssh_config();
    let mut ssh_hosts: Vec<SshHost>;
    
    match ssh_hosts_result {
        Ok(hosts) => {
            ssh_hosts = hosts;
            debug!("ssh_config: {:?}", ssh_hosts);
        },
        Err(e) => {
            return Err(e);
        }
        
    }

    println!("simple-ssh-tui-rs :)");

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    
       execute!(stdout, EnableMouseCapture)?;

       let backend = CrosstermBackend::new(stdout);

       let mut terminal = Terminal::with_options(backend, 
           TerminalOptions {
               viewport: Viewport::Inline(20),
       })?;

       let mut list_state = ListState::default();
       list_state.select(Some(0));

       let mut selected_ssh_host: Option<String> = None;
       
    loop {
       terminal.draw(|f| {
           let chunks = Layout::default()
               .direction(Direction::Vertical)
               .constraints([
                   Constraint::Min(10),
                   Constraint::Length(7),
               ])
               .split(f.size());


           let items: Vec<ListItem> = ssh_hosts
            .iter()
            .map(|host| {
                let display_text = match &host.host_name {
                    Some(ip) => format!("{} ({})", host.host, ip),
                    None => format!("{}", host.host)
                };
           ListItem::new(display_text)
            })
            .collect();


           let list = List::new(items)
            .block(Block::default().borders(Borders::NONE))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("->");

           f.render_stateful_widget(list, chunks[0], &mut list_state);
       });

       if let Event::Key(key) = event::read()? {
           match key.code {
               KeyCode::Esc | KeyCode::Char('c') => break,
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
                               ssh_hosts.len()
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
                 if let Some(index) = list_state.selected() {
                     selected_ssh_host = Some(ssh_hosts[index].host.clone())
                 }  
                 break;
               },
               _ => {}
           }
           
       } 
    }

    disable_raw_mode();
    execute!(terminal.backend_mut(), DisableMouseCapture)?;
    terminal.show_cursor()?;


    if let Some(host) = selected_ssh_host {
        info!("starting ssh");

        let mut child = Command::new("ssh");
        child.arg(host);

        let error = child.exec();

        error!("starting ssh failed with: {}", error);
    } 

    
    Ok(())
}


fn parse_ssh_config() -> Result<Vec<SshHost>> {
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
                warn!("unhandeled key in ssh config");
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
