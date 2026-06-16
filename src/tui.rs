use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Terminal,
};
use std::io;

pub struct ProviderSummary {
    pub name: String,
    pub sessions_count: usize,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost: f64,
}

pub struct DashboardData {
    pub period: String,
    pub total_cost: f64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub carbon_footprint_gco2eq: f64,
    pub providers: Vec<ProviderSummary>,
}

pub fn run_app(data: DashboardData) -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state and run event loop
    let res = run_loop(&mut terminal, &data);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    data: &DashboardData,
) -> io::Result<()>
where
    io::Error: From<<B as Backend>::Error>,
{
    loop {
        terminal.draw(|f| ui(f, data))?;

        if let Event::Key(key) = event::read()? {
            if let KeyCode::Char('q') = key.code {
                return Ok(());
            }
        }
    }
}

fn ui(f: &mut ratatui::Frame, data: &DashboardData) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Length(3), // Header
                Constraint::Min(5),    // Table
                Constraint::Length(3), // Footer
            ]
            .as_ref(),
        )
        .split(f.area());

    // Header
    let header_text = vec![
        Line::from(vec![
            Span::styled(format!("Niriksh Cost Report (Period: {})", data.period), Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(
                " | Cost: ${:.6} | Input: {} | Output: {} | CO2eq: {:.3}g",
                data.total_cost, data.total_input_tokens, data.total_output_tokens, data.carbon_footprint_gco2eq
            )),
        ]),
    ];
    let header = Paragraph::new(header_text)
        .block(Block::default().borders(Borders::ALL).title("Summary"));
    f.render_widget(header, chunks[0]);

    // Provider Table
    let header_cells = ["Provider", "Sessions", "Input Tokens", "Output Tokens", "Cost (USD)"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Cyan)));
    let table_header = Row::new(header_cells)
        .style(Style::default().add_modifier(Modifier::BOLD))
        .height(1)
        .bottom_margin(1);

    let rows = data.providers.iter().map(|p| {
        let cells = vec![
            Cell::from(p.name.clone()),
            Cell::from(p.sessions_count.to_string()),
            Cell::from(p.total_input_tokens.to_string()),
            Cell::from(p.total_output_tokens.to_string()),
            Cell::from(format!("${:.6}", p.total_cost)),
        ];
        Row::new(cells).height(1)
    });

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(20),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(25),
        ],
    )
    .header(table_header)
    .block(Block::default().borders(Borders::ALL).title("Breakdown by Provider"));

    f.render_widget(table, chunks[1]);

    // Footer
    let footer = Paragraph::new("Press 'q' to quit")
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[2]);
}
