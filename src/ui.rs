use std::io::Stdout;
use ratatui::{ 
    Terminal, 
    backend::CrosstermBackend, 
    layout::{ Constraint, Direction, Layout, Position }, 
    style::{ Color, Modifier, Style }, 
    text::{ Span, Line }, 
    widgets::{ Block, Borders, List, ListItem, Paragraph, Wrap }
    };
use tracing::error;
use crate::{AppMode, RsyncActiveInput, app::StatusMsgLevel, ssh_config::SshHost};
use crate::app::App;

pub fn draw_ui(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &App) {
    let terminal_draw_result = terminal.draw(|f| {

        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(4),
                Constraint::Length(1),
            ])
            .split(f.area());
        
        match app.app_mode {
            AppMode::SelectHost => {
                let chunks = if !app.search_query.is_empty() {
                    Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Max(1),
                        Constraint::Min(2),
                        Constraint::Length(8),
                    ])
                    .split(main_chunks[0])
                } else {
                    Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(0),
                            Constraint::Min(2),
                            Constraint::Length(8),
                        ])
                        .split(main_chunks[0])
                };

                if !app.search_query.is_empty() {
                    let search_query_paragraph = Paragraph::new(format!("search: {} ", app.search_query))
                        .block(Block::default()
                        .borders(Borders::NONE));
                    f.render_widget(search_query_paragraph, chunks[0]);
                }

                let items: Vec<ListItem> = filer_ssh_hosts_for_search(&app);
                
        
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
        
                f.render_stateful_widget(list, chunks[1], &mut app.ssh_hosts_list_state.clone());
                f.render_widget(ssh_host_details, chunks[2]);
            }
            AppMode::Rsync => {
                let vertical_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1),
                        Constraint::Length(3),
                        Constraint::Min(0),
                    ])
                    .split(main_chunks[0]);


                let paths_ssh_host_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1),                        
                        Constraint::Length(1),
                        Constraint::Length(1),
                    ])
                    .split(vertical_chunks[1]);


                let key_combination_style = Style::default().fg(Color::Blue);
                let action_style = Style::default().fg(Color::Magenta);
                let divider_style = Style::default().fg(Color::DarkGray);
                
                let info_line = Line::from(vec![
                    Span::styled("| ", divider_style),
                    Span::styled("Download: ", action_style),
                    Span::styled("CTRL + d", key_combination_style),
                    Span::styled(" | ", divider_style),

                    Span::styled("Download Sync: ", action_style),
                    Span::styled("CTRL + s", key_combination_style),
                    Span::styled(" | ", divider_style),  

                    Span::styled("Upload: ", action_style),
                    Span::styled("CTRL + u", key_combination_style),
                    Span::styled(" | ", divider_style),  

                    Span::styled("Upload Sync: ", action_style),
                    Span::styled("CTRL + z", key_combination_style),
                    Span::styled(" | ", divider_style),                ]);
                let info_paragraph = Paragraph::new(info_line)
                    .block(Block::default()
                    .borders(Borders::NONE));

                f.render_widget(info_paragraph, vertical_chunks[0]);
                
                let local_path_input = Paragraph::new(format!("ssh host:    {} ", app.selected_ssh_host.host))
                    .block(Block::default()
                    .borders(Borders::NONE));

                f.render_widget(local_path_input, paths_ssh_host_chunks[0]);        

                let local_path_input = Paragraph::new(format!("Local Path:  {} ", app.rsync_local_path.to_string()))
                    .block(Block::default()
                        .borders(Borders::NONE));
                f.render_widget(local_path_input, paths_ssh_host_chunks[01]);
                
             
                let remote_path_input = Paragraph::new(format!("Remote Path: {} ", app.rsync_remote_path.to_string()))
                    .block(Block::default()
                    .borders(Borders::NONE));

                f.render_widget(remote_path_input, paths_ssh_host_chunks[2]);


                let active_chunk = match app.rsync_active_input {
                    RsyncActiveInput::Local => paths_ssh_host_chunks[1],
                    RsyncActiveInput::Remote => paths_ssh_host_chunks[2],
                };

                let current_input_path_text_len = match app.rsync_active_input {
                    RsyncActiveInput::Local => app.rsync_local_path_cursor_pos,
                    RsyncActiveInput::Remote => app.rsync_remote_path_cursor_pos,
                };

                f.set_cursor_position(Position::new(
                    active_chunk.x + 13 + current_input_path_text_len as u16,
                    active_chunk.y
                ));

                match app.rsync_active_input {
                    RsyncActiveInput::Local => {
                        let path_suggestions_paragraph = construct_path_suggestions_paragraph(&app.local_suggestions.folders, &app.local_suggestions.files);
                        f.render_widget(path_suggestions_paragraph, vertical_chunks[2]);     
                    },
                    RsyncActiveInput::Remote => {  
                        let path_suggestions_paragraph = construct_path_suggestions_paragraph(&app.remote_suggestions.folders, &app.remote_suggestions.files);
                        f.render_widget(path_suggestions_paragraph, vertical_chunks[2]);        
                    }
                }
              

                
            },
            AppMode::SshPasswordPromt => {        
                let vertical_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2),
                    Constraint::Min(0),
                    Constraint::Length(2),
                ])
                .split(main_chunks[0]);
                
                let title_paragraph = Paragraph::new(app.selected_ssh_host.host.clone())
                    .block(Block::default()
                    .borders(Borders::NONE));
    
                f.render_widget(title_paragraph, vertical_chunks[0]);

                let ssh_output_paragraph = Paragraph::new(app.ssh_login_output.clone())
                    .block(Block::default()
                    .borders(Borders::TOP)
                    .title("--- ssh output"));
    
                f.render_widget(ssh_output_paragraph, vertical_chunks[1]);

                let ssh_input_paragraph = Paragraph::new(app.ssh_login_input.clone())
                    .block(Block::default()
                    .borders(Borders::TOP)
                    .title("--- your input (press ENTER to send)"));
    
                f.render_widget(ssh_input_paragraph, vertical_chunks[2]);
            },
             
        }
    
        let status_msg_color =  match app.status_msg.level {
            StatusMsgLevel::Info => Color::Green,
            StatusMsgLevel::Warn => Color::Yellow,
            StatusMsgLevel::Error => Color::Red,
        };
        
        let status_msg_paragraph = Paragraph::new(app.status_msg.msg.clone())
            .block(Block::default()
            .borders(Borders::NONE))
            .style(Style::default().fg(status_msg_color));
    
        f.render_widget(status_msg_paragraph, main_chunks[1]);
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


fn construct_path_suggestions_paragraph(folders: &[String], files: &[String] ) -> Paragraph<'static> {
    let mut folder_spans: Vec<Span<'static>> = folders
        .iter()
        .flat_map(|folder| {
            [
                Span::styled(format!(" {}", folder), Style::default().fg(Color::Blue)),
                Span::styled("/", Style::default().fg(Color::Gray))
            ]
        })
        .collect();

    let file_spans: Vec<Span<'static>> = files
        .iter()
        .flat_map(|file| {
            [
                Span::styled(format!(" {}", file), Style::default().fg(Color::Gray)),
            ]
        })
        .collect();

    folder_spans.extend(file_spans);
    
    Paragraph::new(Line::from(folder_spans))    
        .block(Block::default()
        .borders(Borders::ALL).title(" local "))
        .wrap(Wrap { trim: true })
}


fn filer_ssh_hosts_for_search<'a>(app: &App) -> Vec<ListItem<'a>> {
    app.get_filtered_ssh_hosts()
    .iter()
    .map(|host| {
        let username = &host.user.clone().unwrap_or("".to_string());
        let host_name = &host.host_name.clone().unwrap_or("".to_string());
        let port = &host.port.clone().unwrap_or("".to_string());
        
    
    ListItem::from(Line::from(vec![
        Span::styled(host.host.clone(), Style::default().fg(Color::Magenta)),
        Span::styled(" (", Style::default().fg(Color::DarkGray)),
        Span::styled(username.clone(), Style::default().fg(Color::Green)),
        Span::styled("@", Style::default().fg(Color::DarkGray)),
        Span::styled(host_name.clone(), Style::default().fg(Color::LightBlue)),
        Span::styled(":", Style::default().fg(Color::DarkGray)),
        Span::styled(port.clone(), Style::default().fg(Color::Yellow)),
        Span::styled(")", Style::default().fg(Color::DarkGray)),
    ]))
    })
    .collect()
}
