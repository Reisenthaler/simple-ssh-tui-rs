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

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    beautiful_log::init_logging("INFO");
    
    let ssh_hosts = parse_ssh_config()?;
    
    println!("simple-ssh-tui-rs :)");

    let mut list_state = ListState::default();
    list_state.select(Some(0));

    let mut selected_ssh_host: Option<String> = None;

    let mut terminal = setup_terminal()?;
       
    loop {
        draw_ui(&mut terminal, &ssh_hosts, &mut list_state);

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

    restore_terminal_to_normal_mode(&mut terminal)?;
    
    start_ssh_process(selected_ssh_host);
    
    Ok(())
}

fn start_ssh_process(ssh_host: Option<String>) {
    if let Some(host) = ssh_host {
        info!("starting ssh");

        let mut child = Command::new("ssh");
        child.arg(host);

        let error = child.exec();

        error!("starting ssh failed with: {}", error);
    } 
}


fn setup_terminal() -> std::result::Result<Terminal<CrosstermBackend<Stdout>>, std::io::Error> {
    
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    
       execute!(stdout, EnableMouseCapture)?;

       let backend = CrosstermBackend::new(stdout);

       let terminal = Terminal::with_options(backend, 
           TerminalOptions {
               viewport: Viewport::Inline(20),
       });

       return terminal;
}

fn restore_terminal_to_normal_mode(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()>{
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), DisableMouseCapture)?;
    terminal.show_cursor()?;   

    Ok(())
}
