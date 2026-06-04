use std::io::Stdout;
use ratatui::{ 
    Terminal, 
    backend::{ CrosstermBackend }, 
    layout::{ Constraint, Direction, Layout }, 
    style::{Modifier, Style }, 
    widgets::{ Block, Borders, List, ListItem, ListState }
    };
use tracing::error;
use crate::ssh_config::SshHost;


pub fn draw_ui(terminal: &mut Terminal<CrosstermBackend<Stdout>>, ssh_hosts: &Vec<SshHost>, mut list_state: &mut ListState) {
    let terminal_draw_result = terminal.draw(|f| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(10),
                Constraint::Length(7),
            ])
            .split(f.area());


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


    match terminal_draw_result {
        Ok(_) => {}
        Err(e) => {
            error!("error drawing to terminal: {}", e);
        }
    }
}
