use rand::{Rng, RngExt};
use std::fs;
use std::io::stdout;
use std::path::{Path, PathBuf};

use crate::{App, Entry};
use color_eyre::Result;
use color_eyre::eyre::eyre;
use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use nucleo::Config;
use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs};
use ratatui::{DefaultTerminal, Frame};

const TEMP_FILE_PATH: &str = "/tmp/temp.txt";

enum Section {
    None,
    Title,
    HeadingPath,
    Description,
    Commands,
    SourceFile,
}

pub fn run_event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| render(frame, app))?;
        if handle_key_event(app, terminal)? {
            break Ok(());
        }
    }
}

fn entry_to_template(entry: &Entry) -> String {
    let mut out = String::new();

    out.push_str("--- TITLE ---\n");
    out.push_str(&entry.title);
    out.push('\n');

    out.push_str("--- HEADING_PATH ---\n");
    out.push_str(&entry.heading_path.join(" > "));
    out.push('\n');

    out.push_str("--- DESCRIPTION ---\n");
    out.push_str(&entry.description);
    out.push('\n');

    out.push_str("--- SOURCE-FILE (.json) ---\n");
    out.push_str(&entry.description);
    out.push('\n');

    out.push_str("--- COMMANDS ---\n");
    out.push_str("# One command per block. Blocks separated by \"===\".\n");
    out.push_str("# Format:\n");
    out.push_str("# [description]\n");
    out.push_str("# <command body on one or more lines>\n");
    out.push_str(&entry.cmd);
    out.push('\n');

    out
}

fn parse_template(entry_id: &str) -> Result<Entry> {
    let contents = fs::read_to_string(TEMP_FILE_PATH)?;
    let mut section = Section::None;

    let mut title = String::new();
    let mut heading_raw = String::new();
    let mut description = String::new();
    let mut cmd = String::new();
    let mut source_file = String::new();

    for line in contents.lines() {
        match line.trim() {
            "--- TITLE ---" => {
                section = Section::Title;
                continue;
            }
            "--- HEADING_PATH ---" => {
                section = Section::HeadingPath;
                continue;
            }
            "--- DESCRIPTION ---" => {
                section = Section::Description;
                continue;
            }
            "--- COMMANDS ---" => {
                section = Section::Commands;
                continue;
            }
            "--- SOURCE-FILE (.json) ---" => {
                section = Section::SourceFile;
                continue;
            }
            _ => {}
        }

        match section {
            Section::Title => {
                title.push_str(line);
                title.push('\n');
            }
            Section::HeadingPath => {
                heading_raw.push_str(line);
                heading_raw.push('\n');
            }
            Section::Description => {
                description.push_str(line);
                description.push('\n');
            }
            Section::Commands => {
                if line.trim_start().starts_with('#') {
                    continue;
                } // Skip comments in template
                cmd.push_str(line);
                cmd.push('\n');
            }
            Section::SourceFile => {
                if line.trim_start().starts_with('#') {
                    continue;
                }

                let mut file = line.trim().to_string();

                if !file.ends_with(".json") {
                    file.push_str(".json");
                }

                let path = Path::new(&file);
                let full_path = if path.starts_with("JSONs") {
                    path.to_path_buf()
                } else {
                    Path::new("JSONs").join(path)
                };

                source_file.push_str(full_path.to_string_lossy().as_ref());
                source_file.push('\n');
            }
            Section::None => {} // lines before any section marker — ignore
        }
    }

    if title.trim().is_empty() {
        return Err(eyre!("missing or empty TITLE section"));
    }
    if heading_raw.trim().is_empty() {
        return Err(eyre!("missing or empty HEADING_PATH section"));
    }
    if description.trim().is_empty() {
        return Err(eyre!("missing or empty DESCRIPTION section"));
    }
    if cmd.trim().is_empty() {
        return Err(eyre!("missing or empty COMMANDS section"));
    }

    let new_entry = Entry {
        id: entry_id.to_string(),
        title: title.trim().to_string(),
        cmd: cmd.trim().to_string(),
        description: description.trim().to_string(),
        heading_path: heading_raw
            .trim()
            .split(" > ")
            .map(|s| s.trim().to_string())
            .collect(),
        source_file: PathBuf::from(source_file.trim()),
    };
    Ok(new_entry)
}
fn handle_key_event(app: &mut App, terminal: &mut DefaultTerminal) -> Result<bool> {
    if let Event::Key(key) = event::read()? {
        if key.kind != KeyEventKind::Press {
            return Ok(false);
        }
        match key.code {
            KeyCode::Esc => return Ok(true),

            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.list_state
                    .selected()
                    .and_then(|filtered_index| app.results.get(filtered_index))
                    .and_then(|&i| Some(app.entries.remove(i)));
            }
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let mut entry = Entry::new();

                let mut rng = rand::rng();
                let id = format!("{:08x}", rng.random::<u32>());
                entry.id = id;

                // Disable raw mode and leave alternate screen
                disable_raw_mode()?;
                execute!(stdout(), LeaveAlternateScreen, Show)?;

                let out = entry_to_template(&entry);
                fs::write(TEMP_FILE_PATH, out)?;

                std::process::Command::new("sh")
                    .arg("-c")
                    .arg("vim /tmp/temp.txt")
                    .status()
                    .expect("Failed to execute vim");
                let updated_entry = parse_template(&entry.id)?;

                app.entries.push(updated_entry);

                // Re-enable raw mode and re-enter alternate screen
                enable_raw_mode()?;
                execute!(stdout(), EnterAlternateScreen, Hide)?;
                terminal.clear()?;
            }
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(entry) = app.selected_entry() {
                    let Some(selected_index) = app.selected_entry_index() else {
                        return Ok(false);
                    };

                    // Disable raw mode and leave alternate screen
                    disable_raw_mode()?;
                    execute!(stdout(), LeaveAlternateScreen, Show)?;
                    let out = entry_to_template(&entry);
                    fs::write(TEMP_FILE_PATH, out)?;

                    std::process::Command::new("sh")
                        .arg("-c")
                        .arg("vim /tmp/temp.txt")
                        .status()
                        .expect("Failed to execute vim");
                    let updated_entry = parse_template(&entry.id)?;
                    app.entries[selected_index] = updated_entry;

                    // Re-enable raw mode and re-enter alternate screen
                    enable_raw_mode()?;
                    execute!(stdout(), EnterAlternateScreen, Hide)?;
                    terminal.clear()?;
                }
            }

            KeyCode::Enter => {
                if let Some(entry) = app.selected_entry() {
                    if let Ok(mut cb) = arboard::Clipboard::new() {
                        let _ = cb.set_text(entry.cmd.clone());
                    }
                }
                return Ok(true);
            }
            KeyCode::Tab => {
                app.mode = (app.mode + 1) % 4;
            }

            KeyCode::Char('[') => {
                app.top_tab = if app.top_tab == 0 { 1 } else { 0 };
            }
            KeyCode::Char(']') => {
                app.top_tab = (app.top_tab + 1) % 2;
            }

            KeyCode::Down => {
                let len = app.results.len();
                if len > 0 {
                    let i = app
                        .list_state
                        .selected()
                        .map(|i| if i == len - 1 { 0 } else { i + 1 })
                        .unwrap_or(0);
                    app.list_state.select(Some(i));
                }
            }
            KeyCode::Up => {
                let len = app.entries.len();
                if len > 0 {
                    let i = app
                        .list_state
                        .selected()
                        .map(|i| if i == 0 { len - 1 } else { i - 1 })
                        .unwrap_or(0);
                    app.list_state.select(Some(i));
                }
            }

            KeyCode::Backspace => {
                app.query.pop();
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                app.query.push(c);
                search(app);
            }
            _ => {}
        }
    }
    Ok(false)
}

pub fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // top tabs (Search / Methodology)
        Constraint::Length(1), // spacer
        Constraint::Length(3), // search input (bordered = 3 rows)
        Constraint::Min(0),    // main content
        Constraint::Length(1), // footer
    ])
    .split(frame.area());

    render_top_tabs(frame, chunks[0], app);
    // chunks[1] is intentional whitespace
    render_search_input(frame, chunks[2], app);
    render_main(frame, chunks[3], app);
}

fn render_top_tabs(frame: &mut Frame, area: Rect, app: &App) {
    let tabs = Tabs::new(vec!["Search", "Methodology"])
        .select(app.top_tab)
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
        .divider(" ")
        .padding("  ", "  ");

    frame.render_widget(tabs, area);
}

fn render_search_input(frame: &mut Frame, area: Rect, app: &App) {
    let mode_label = match app.mode {
        0 => "CMD",
        1 => "TITLE",
        2 => "HEADING",
        3 => "ALL",
        _ => "?",
    };

    let mode_title = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            format!(" {} ", mode_label),
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(mode_title);

    let line = if app.query.is_empty() {
        Line::from(vec![Span::raw("  ")])
    } else {
        Line::from(vec![
            Span::raw("  "),
            Span::styled(app.query.as_str(), Style::default().fg(Color::White)),
        ])
    };

    let input = Paragraph::new(line).block(block);
    frame.render_widget(input, area);
}

fn render_main(frame: &mut Frame, area: Rect, app: &mut App) {
    let cols =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).split(area);

    render_results(frame, cols[0], app);
    render_detail(frame, cols[1], app);
}

fn search(app: &mut App) {
    if app.query.trim().is_empty() {
        app.results = (0..app.entries.len()).collect();
        return;
    }

    let mut matcher = nucleo::Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(&app.query, CaseMatching::Ignore, Normalization::Smart);

    let haystacks: Vec<String> = match app.mode {
        0 => app.entries.iter().map(|e| e.cmd.clone()).collect(), // CMD
        1 => app.entries.iter().map(|e| e.title.clone()).collect(), // TITLE
        2 => app
            .entries
            .iter()
            .map(|e| e.heading_path.join(" ").clone())
            .collect(),

        3 => app
            .entries
            .iter() // ALL
            .map(|e| format!("{} {} {}", e.cmd, e.title, e.heading_path.join(" ")))
            .collect(),
        _ => app.entries.iter().map(|e| e.cmd.clone()).collect(), // fallback = CMD
    };

    let mut scored: Vec<(usize, u32)> = Vec::new();

    for (i, haystack) in haystacks.iter().enumerate() {
        let mut buf = Vec::new();
        let hay = nucleo::Utf32Str::new(&haystack, &mut buf);
        if let Some(score) = pattern.score(hay, &mut matcher) {
            scored.push((i, score));
        }
    }
    scored.sort_by(|a, b| b.1.cmp(&a.1));
    app.results = scored.into_iter().map(|(i, _)| i).collect();

    if app.results.is_empty() {
        app.list_state.select(None);
    } else {
        app.list_state.select(Some(0));
    }
}

fn render_results(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title_bottom(format!(" RESULTS  {} entries ", app.entries.len()))
        .border_style(Style::default().fg(Color::DarkGray));

    let items: Vec<ListItem> = app
        .results
        .iter()
        .filter_map(|&i| app.entries.get(i))
        .map(|e| {
            let breadcrumb = e.heading_path.join(" › ");
            let lines = vec![
                Line::from(Span::styled(
                    breadcrumb,
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(vec![
                    Span::styled("  $ ", Style::default().fg(Color::DarkGray)),
                    Span::styled(&e.cmd, Style::default().fg(Color::White)),
                ]),
                Line::from(""), // blank separator
            ];
            ListItem::new(lines)
        })
        .collect();

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(Color::Rgb(20, 30, 40))
            .add_modifier(Modifier::BOLD),
    );

    frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn render_detail(frame: &mut Frame, area: Rect, app: &App) {
    let selected = app.selected_entry();
    let Some(entry) = selected else {
        let p = Paragraph::new("Select an entry")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(" Details "));

        frame.render_widget(p, area);
        return;
    };

    let cards = Layout::vertical([
        Constraint::Length(6), // heading + title + command
        Constraint::Length(5), // description
        Constraint::Min(0),    // rest
    ])
    .split(area);

    // Top card: breadcrumb + title + primary command
    let breadcrumb = entry.heading_path.join(" › ");
    let top = Paragraph::new(vec![
        Line::from(Span::styled(
            breadcrumb,
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            entry.title.as_str(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(top, cards[0]);
}
