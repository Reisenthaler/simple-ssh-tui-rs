use std::io::{Stdout, Write};
use crossterm::{
    cursor::MoveTo, event::{ self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode }, execute, terminal::{ Clear, ClearType, disable_raw_mode, enable_raw_mode }};
use ratatui::{ 
    Terminal, TerminalOptions, Viewport, 
    backend::CrosstermBackend, 
    widgets::{ ListState  }
    };
use crate::Result;
    
pub fn setup_terminal(ssh_hosts_count: usize) -> std::result::Result<Terminal<CrosstermBackend<Stdout>>, std::io::Error> {
    
    enable_raw_mode()?;       

    let mut stdout = std::io::stdout();
    
       execute!(stdout, EnableMouseCapture)?;

       let backend = CrosstermBackend::new(stdout);

       let terminal = Terminal::with_options(backend, 
           TerminalOptions {
               viewport: Viewport::Inline((ssh_hosts_count + 8).try_into().unwrap_or(20)),
       });

       
       return terminal;
}

pub fn restore_terminal_to_normal_mode(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()>{
    terminal.clear()?;
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), DisableMouseCapture)?;
    terminal.show_cursor()?;   

    Ok(())
}
