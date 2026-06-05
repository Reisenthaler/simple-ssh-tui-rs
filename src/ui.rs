use std::{fmt::format, io::Stdout};
use ratatui::{ 
    Terminal, 
    backend::CrosstermBackend, 
    layout::{ Constraint, Direction, Layout, Position }, 
    style::{Modifier, Style }, 
    widgets::{ Block, Borders, List, ListItem, ListState, Paragraph, Wrap }
    };
use tracing::{error, info};
use crate::{AppMode, RsyncActiveInput, RsyncStatus, split_path, ssh_config::SshHost};

pub fn draw_ui(terminal: &mut Terminal<CrosstermBackend<Stdout>>, ssh_hosts: &Vec<SshHost>, selected_ssh_host: SshHost, mut list_state: &mut ListState,
    app_mode: &AppMode, rsync_active_input: &RsyncActiveInput, rsync_local_path: &mut String, mut rsycn_remote_path: &mut String,
    is_fetching: bool, local_suggestions: &mut Vec<String>, remote_suggestions: &mut Vec<String>, sync_message: &String) {
    let terminal_draw_result = terminal.draw(|f| {

        match app_mode {
            AppMode::SelectHost => {
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
            }
            AppMode::Rsync => {
                let vertical_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Min(0),
                        Constraint::Length(1),
                    ])
                    .split(f.area());
                let horizontal_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(33),                        
                        Constraint::Percentage(34),
                        Constraint::Percentage(33),
                    ])
                    .split(vertical_chunks[0]);

                let local_path_input = Paragraph::new(rsync_local_path.to_string())
                    .block(Block::default()
                        .borders(Borders::ALL).title(" Local Path "));
                f.render_widget(local_path_input, horizontal_chunks[0]);
                
                let local_path_input = Paragraph::new(format!(" {}", selected_ssh_host.host))
                    .block(Block::default()
                    .borders(Borders::ALL).title(" ssh host "));

                f.render_widget(local_path_input, horizontal_chunks[1]);        

                let remote_path_input = Paragraph::new(rsycn_remote_path.to_string())
                    .block(Block::default()
                    .borders(Borders::ALL).title(" Remote Path "));

                f.render_widget(remote_path_input, horizontal_chunks[2]);


                let active_chunk = match rsync_active_input {
                    RsyncActiveInput::Left => horizontal_chunks[0],
                    RsyncActiveInput::Right => horizontal_chunks[2],
                };

                let current_input_path_text_len = match rsync_active_input {
                    RsyncActiveInput::Left => rsync_local_path.len(),
                    RsyncActiveInput::Right => rsycn_remote_path.len(),
                };

                f.set_cursor_position(Position::new(
                    active_chunk.x + 1 + current_input_path_text_len as u16,
                    active_chunk.y + 1
                ));

                match rsync_active_input {
                    RsyncActiveInput::Left => {
                        if !local_suggestions.is_empty() {
                            if local_suggestions.len() == 1 {
                                let (parent_dir, _) = split_path(&rsync_local_path);
        
                                *rsync_local_path = format!("{}{}", parent_dir, local_suggestions[0]);
        
                                local_suggestions.clear();
                            }
                        }
                        
                        let path_suggestions = Paragraph::new(format!("{}", local_suggestions.join(" ")))
                            .block(Block::default()
                            .borders(Borders::ALL).title(" local "))
                            .wrap(Wrap { trim: true });
                        f.render_widget(path_suggestions, vertical_chunks[1]);     
                    },
                    RsyncActiveInput::Right => {  
                        if !remote_suggestions.is_empty() {
                            if remote_suggestions.len() == 1 {
                                let (parent_dir, _) = split_path(&rsycn_remote_path);
        
                                *rsycn_remote_path = format!("{}{}", parent_dir, remote_suggestions[0]);
        
                                remote_suggestions.clear();
                            }
                        }
                        
                        let path_suggestions = Paragraph::new(format!("{}", remote_suggestions.join(" ")))
                            .block(Block::default()
                            .borders(Borders::ALL).title(" ssh host "))
                            .wrap(Wrap { trim: true });
                        f.render_widget(path_suggestions, vertical_chunks[1]);        
                    }
                }
              
                let rsync_status = Paragraph::new(sync_message.to_string())
                    .block(Block::default()
                    .borders(Borders::NONE));

                f.render_widget(rsync_status, vertical_chunks[2]);

                
            }
    }
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
