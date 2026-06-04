use std::os::unix::process;
use std::process::Stdio;
use std::{ os::unix::process::CommandExt, process::Command };
use std::io::Stdout;
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

fn main() -> Result<()> {
    beautiful_log::init_logging("INFO");
    
    let ssh_hosts = parse_ssh_config()?;
    
    let mut list_state = ListState::default();
    list_state.select(Some(0));

    let mut selected_ssh_host: SshHost = ssh_hosts[0].clone();

    let mut app_mode = AppMode::SelectHost;
    let mut rsync_active_input = RsyncActiveInput::Left;
    let mut rsync_local_path = String::new();    
    let mut rsync_remote_path = String::new();

    let mut terminal = setup_terminal(ssh_hosts.len())?;
       
    loop {
        draw_ui(&mut terminal, &ssh_hosts, selected_ssh_host.clone(), &mut list_state, &app_mode, &rsync_active_input, &rsync_local_path, &rsync_remote_path);

       if let Event::Key(key) = event::read()? {
           match key.code {
               KeyCode::Esc | KeyCode::Char('c') => {
                   restore_terminal_to_normal_mode(&mut terminal)?;
                   return Ok(())
               },
               KeyCode::Char('r') |  KeyCode::Char('R') => {
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
               }
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
                 break;
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
