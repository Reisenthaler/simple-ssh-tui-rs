use std::io::Stdout;
use ratatui::{ 
    Terminal, 
    backend::CrosstermBackend, 
    layout::{ Constraint, Direction, Layout, Position }, 
    style::{Modifier, Style }, 
    widgets::{ Block, Borders, List, ListItem, Paragraph, Wrap }
    };
use tracing::error;
use crate::{AppMode, RsyncActiveInput, split_path, ssh_config::SshHost};
use crate::app::App;

pub fn draw_ui(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) {
    let terminal_draw_result = terminal.draw(|f| {

        match app.app_mode {
            AppMode::SelectHost => {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Min(2),
                        Constraint::Length(8),
                    ])
                    .split(f.area());
        
        
                let items: Vec<ListItem> = app.ssh_hosts
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
        
                let host_details: Vec<ListItem> = get_host_detail_list(&app.selected_ssh_host);
                
                let ssh_host_details = List::new(host_details)
                    .block(Block::default().borders(Borders::TOP).title("------- SSH Details "));
        
                
                f.render_stateful_widget(list, chunks[0], &mut app.ssh_hosts_list_state);
                f.render_stateful_widget(ssh_host_details, chunks[1], &mut app.ssh_hosts_list_state);
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
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1),                        
                        Constraint::Length(1),
                        Constraint::Length(1),
                    ])
                    .split(vertical_chunks[0]);

                  let local_path_input = Paragraph::new(format!("ssh host:    {} ", app.selected_ssh_host.host))
                    .block(Block::default()
                    .borders(Borders::NONE));

                f.render_widget(local_path_input, horizontal_chunks[0]);        

                let local_path_input = Paragraph::new(format!("Local Path:  {} ", app.rsync_local_path.to_string()))
                    .block(Block::default()
                        .borders(Borders::NONE));
                f.render_widget(local_path_input, horizontal_chunks[01]);
                
             
                let remote_path_input = Paragraph::new(format!("Remote Path: {} ", app.rsync_remote_path.to_string()))
                    .block(Block::default()
                    .borders(Borders::NONE));

                f.render_widget(remote_path_input, horizontal_chunks[2]);


                let active_chunk = match app.rsync_active_input {
                    RsyncActiveInput::Local => horizontal_chunks[1],
                    RsyncActiveInput::Remote => horizontal_chunks[2],
                };

                let current_input_path_text_len = match app.rsync_active_input {
                    RsyncActiveInput::Local => app.rsync_local_path.len(),
                    RsyncActiveInput::Remote => app.rsync_remote_path.len(),
                };

                f.set_cursor_position(Position::new(
                    active_chunk.x + 13 + current_input_path_text_len as u16,
                    active_chunk.y
                ));

                match app.rsync_active_input {
                    RsyncActiveInput::Local => {
                        if !app.local_suggestions.is_empty() {
                            if app.local_suggestions.len() == 1 {
                                let (parent_dir, _) = split_path(&app.rsync_local_path);
        
                                app.rsync_local_path = format!("{}{}", parent_dir, app.local_suggestions[0]);
        
                                app.local_suggestions.clear();
                            }
                        }
                        
                        let path_suggestions = Paragraph::new(format!("{}", app.local_suggestions.join(" ")))
                            .block(Block::default()
                            .borders(Borders::ALL).title(" local "))
                            .wrap(Wrap { trim: true });
                        f.render_widget(path_suggestions, vertical_chunks[1]);     
                    },
                    RsyncActiveInput::Remote => {  
                        if !app.remote_suggestions.is_empty() {
                            if app.remote_suggestions.len() == 1 {
                                let (parent_dir, _) = split_path(&app.rsync_remote_path);
        
                                app.rsync_remote_path = format!("{}{}", parent_dir, app.remote_suggestions[0]);
        
                                app.remote_suggestions.clear();
                            }
                        }
                        
                        let path_suggestions = Paragraph::new(format!("{}", app.remote_suggestions.join(" ")))
                            .block(Block::default()
                            .borders(Borders::ALL).title(" ssh host "))
                            .wrap(Wrap { trim: true });
                        f.render_widget(path_suggestions, vertical_chunks[1]);        
                    }
                }
              
                let rsync_status = Paragraph::new(app.status_msg.to_string())
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

fn get_host_detail_list(ssh_host: &SshHost) -> Vec<ListItem<'_>> {
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
