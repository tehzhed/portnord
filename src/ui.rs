use std::{collections::BTreeMap, time::Duration};

use tui::{Terminal, backend::{CrosstermBackend, Backend}, widgets::{Block, Borders, Paragraph, ListItem, List}, text::{Span, Spans}, style::{Style, Modifier, Color}, layout::{Alignment, Rect, Layout, Direction, Constraint}, Frame};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::state;

use state::AppState;

pub struct UI<'a> {
    pub terminal: ThisTerminal,
    pub app_state: &'a mut AppState,
}

impl<'a> UI<'a> {
    
    pub fn new(app_state: &'a mut AppState) -> UI<'a> {
        let terminal: ThisTerminal = setup_terminal();
        UI { terminal, app_state, }
    }

    pub async fn update(&mut self) -> Result<bool, Box<dyn std::error::Error>>  {
        self.terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Percentage(90),
                        Constraint::Percentage(10)
                    ]
                    .as_ref(),
                )
                .split(f.size());
            build_services(f, chunks[0], &mut self.app_state);
            build_footer(f, chunks[1], &mut self.app_state);
        }).unwrap();

        handle_events(&mut self.terminal, &mut self.app_state).await
    }
}

type ThisTerminal = Terminal<CrosstermBackend<std::io::Stdout>>;

fn command_list() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("Arrows (←↑→↓)", "Move around"),
        ("Enter", "Toggle port forwarding"),
        ("q", "Quit"),
    ])
}

fn build_block(title: &str) -> Block {
    Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(
                title,
                Style::default().add_modifier(Modifier::BOLD),
            ))
}

fn build_key_bindings_paragraph<'a>() -> Paragraph<'a> {
    let commands = command_list();
    let command_spans: Vec<Span> = commands.into_iter().map(|command| {
        vec![
            Span::styled(command.0.to_owned(), Style::default().fg(Color::Green)),    
            Span::styled(": ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(command.1.to_owned(), Style::default().add_modifier(Modifier::ITALIC)),
            Span::raw("   "),
        ]
    }).flatten().collect();

    Paragraph::new(Spans::from(command_spans))
        .block(build_block("Key bindings"))
        .alignment(Alignment::Left)
        .wrap(tui::widgets::Wrap { trim: true })
}

fn build_namespace_paragraph<'a>(namespace_opt: Option<String>) -> Paragraph<'a> {
    let namespace_spans = vec![
        Span::styled(
            namespace_opt.unwrap_or("default".to_string()), 
            Style::default().add_modifier(Modifier::BOLD | Modifier::ITALIC).fg(Color::Cyan)),
    ];
    Paragraph::new(Spans::from(namespace_spans))
        .block(build_block("Namespace"))
        .alignment(Alignment::Left)
        .wrap(tui::widgets::Wrap { trim: true})
}

fn build_services_list<'a>(services: &'a Vec<String>, forwarded_ports: &Vec<state::ForwardedPort>) -> List<'a> {
    let items: Vec<ListItem> = services.iter().map(|service| 
        ListItem::new(vec![Spans::from(Span::styled(
            service,
            if forwarded_ports.iter().find(|fw_port| &fw_port.service == service).is_some() {
                Style::default().add_modifier(Modifier::ITALIC).add_modifier(Modifier::UNDERLINED)
            } else {
                Style::default().add_modifier(Modifier::ITALIC)
            }
        ))])
    ).collect();
    List::new(items)
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .block(build_block("Services"))
}

fn build_ports_list<'a>(ports: &'a Vec<i32>, forwarded_ports: &Vec<&state::ForwardedPort>) -> List<'a> {
    let items: Vec<ListItem> = ports.iter().map(|port| 
        ListItem::new(vec![Spans::from(Span::styled(
            port.to_string(),
            if forwarded_ports.iter().find(|fw_port| fw_port.port == port.to_owned() as u16).is_some() {
                Style::default().add_modifier(Modifier::ITALIC).add_modifier(Modifier::UNDERLINED)
            } else {
                Style::default().add_modifier(Modifier::ITALIC)
            }
        ))])
    ).collect();
    List::new(items)
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .block(build_block("Ports"))
}

fn build_services<B: Backend>(f: &mut Frame<B>, area: Rect, state: &mut AppState) {
    let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Percentage(60),
                        Constraint::Percentage(40)
                    ]
                    .as_ref(),
                )
                .split(area);           
    f.render_stateful_widget(build_services_list(&state.service_list(), &state.forwarded_ports), chunks[0], &mut state.service_selection);
    f.render_stateful_widget(build_ports_list(&state.port_list(), &state.forwarded_ports_for_selected_service()), chunks[1], &mut state.port_selection);
}

fn build_footer<B: Backend>(f: &mut Frame<B>, area: Rect, state: &mut AppState) {
    let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Percentage(90),
                        Constraint::Percentage(10)
                    ]
                    .as_ref(),
                )
                .split(area); 
    f.render_widget(build_key_bindings_paragraph(), chunks[0]);
    f.render_widget(build_namespace_paragraph(state.namespace_opt.to_owned()), chunks[1]);
}

fn setup_terminal() -> ThisTerminal {
    enable_raw_mode().unwrap();
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).unwrap();
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend).unwrap();
    return terminal;
}

fn destroy_terminal(terminal: &mut ThisTerminal) {
    disable_raw_mode().unwrap();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    ).unwrap();
    terminal.show_cursor().unwrap();
}

async fn handle_events<'a>(terminal: &mut ThisTerminal, state: &mut AppState) -> Result<bool, Box<dyn std::error::Error>> {
    if crossterm::event::poll(Duration::from_millis(250))? {
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => {
                    destroy_terminal(terminal);
                    return Ok(false);
                },
                KeyCode::Enter => {
                    state.toggle_port_forwarding().await?;
                    return Ok(true)
                },
                KeyCode::Left => {
                    state.deselect();
                    return Ok(true)
                },
                KeyCode::Right => {
                    state.select();
                    return Ok(true)
                },
                KeyCode::Down => {
                    state.next();
                    return Ok(true)
                },
                KeyCode::Up => {
                    state.previous();
                    return Ok(true)
                },
                _ => return Ok(true)
            }
        } else {
            return Ok(true)
        }
    } else {
        return Ok(true)
    }
}