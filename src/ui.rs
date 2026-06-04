use std::io::Stdout;
use ratatui::{ 
    Terminal, 
    backend::CrosstermBackend, 
    layout::{ Constraint, Direction, Layout, Position }, 
    style::{Modifier, Style }, 
    widgets::{ Block, Borders, List, ListItem, ListState }
    };
use tracing::error;
use crate::ssh_config::SshHost;


pub fn draw_ui(terminal: &mut Terminal<CrosstermBackend<Stdout>>, ssh_hosts: &Vec<SshHost>, selected_ssh_host: SshHost, mut list_state: &mut ListState) {
    let terminal_draw_result = terminal.draw(|f| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(2),
                Constraint::Length(8),
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

        let host_details: Vec<ListItem> = get_host_detail_list(&selected_ssh_host);
        
        let ssh_host_details = List::new(host_details)
            .block(Block::default().borders(Borders::TOP).title("------- SSH Details "));

        
        f.render_stateful_widget(list, chunks[0], &mut list_state);
        f.render_stateful_widget(ssh_host_details, chunks[1], &mut list_state);

    });


    match terminal_draw_result {
        Ok(_) => {}
        Err(e) => {
            error!("error drawing to terminal: {}", e);
        }
    }
}

fn get_host_detail_list(ssh_host: &SshHost) -> Vec<ListItem> {
    let mut host_details: Vec<ListItem> = Vec::new();

    host_details.push(ListItem::new(format!("Alias:     {}", ssh_host.host)));

    if let Some(ref host_name) = ssh_host.host_name {
        host_details.push(ListItem::new(format!("Host:      {}", host_name)));
    }
    if let Some(ref port) = ssh_host.port {
        host_details.push(ListItem::new(format!("Port:      {}", port)));
    }
    if let Some(ref user) = ssh_host.user {
        host_details.push(ListItem::new(format!("User:      {}", user)));
    }
    if let Some(ref proxy_jump) = ssh_host.proxy_jump {
        host_details.push(ListItem::new(format!("ProxyJump: {}", proxy_jump)));
    }

    return host_details;
}
