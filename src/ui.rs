use rand::RngExt;
use ratatui::widgets::Wrap;
use regex::Regex;
use std::ffi::OsStr;
use std::fs;
use std::io::stdout;
use std::path::{Path, PathBuf};

use crate::{App, Chain, Entry, SearchMode};
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
use ratatui::layout::{Alignment, Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs};
use ratatui::{DefaultTerminal, Frame};
use std::process::Command;
use std::sync::OnceLock;

const C_BORDER: Color = Color::Rgb(140, 150, 170); // muted blue-gray for borders
const C_DIM: Color = Color::Rgb(100, 110, 130); // dim text / breadcrumbs
const C_FG_BRIGHT: Color = Color::Rgb(220, 228, 245); // bright/primary text
const C_ACCENT: Color = Color::Rgb(92, 196, 255); // cyan accent (tabs, mode badge)
const C_ACCENT_BG: Color = Color::Rgb(14, 24, 38); // dark bg for accent badge text
const C_HIGHLIGHT_BG: Color = Color::Rgb(20, 30, 40); // list selection highlight
const C_TITLE: Color = Color::Rgb(175, 185, 209); // description / title text
const C_DESC: Color = Color::Rgb(140, 150, 170); // description body text

static TEMP_FILE_PATH: OnceLock<String> = OnceLock::new();

fn atoms_present(query: &str, haystacks: &[&str]) -> bool {
    let lowered: Vec<String> = haystacks.iter().map(|h| h.to_lowercase()).collect();
    query.split_whitespace().all(|atom| {
        let a = atom.to_lowercase();
        lowered.iter().any(|h| h.contains(&a))
    })
}

pub fn get_temp_path() -> &'static str {
    TEMP_FILE_PATH.get_or_init(|| {
        #[cfg(target_os = "windows")]
        return std::env::var("TEMP").unwrap_or("C:\\Windows\\Temp".into()) + "\\temp.txt";

        #[cfg(not(target_os = "windows"))]
        return "/tmp/temp.txt".to_string();
    })
}

enum Section {
    None,
    Title,
    HeadingPath,
    Description,
    Commands,
    SourceFile,
}

pub fn run_event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    search(app, false);
    loop {
        terminal.draw(|frame| render(frame, app))?;
        if handle_key_event(app, terminal)? {
            break Ok(());
        }
    }
}

fn copy_to_clipboard(text: &str) {
    #[cfg(target_os = "windows")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};
        if let Ok(mut child) = Command::new("clip").stdin(Stdio::piped()).spawn() {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        }
    }

    #[cfg(target_os = "macos")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};
        if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};
        if let Ok(mut child) = Command::new("xsel")
            .args(["--clipboard", "--input"])
            .stdin(Stdio::piped())
            .spawn()
        {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
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

    out.push_str("--- SOURCE-FILE ---\n");
    out.push_str(&entry.source_file.to_str().unwrap_or_default());
    out.push('\n');

    out.push_str("--- COMMANDS ---\n");
    out.push_str(&entry.cmd);
    out.push('\n');

    out
}

fn parse_template(entry_id: &str, app: &App) -> Result<Entry> {
    let contents = fs::read_to_string(get_temp_path())?;
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
            "--- SOURCE-FILE ---" => {
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

                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // Extract just the filename, discarding any directory components
                let json_pattern = Regex::new(r"(?i)\.json").unwrap();
                let cmds_pattern = Regex::new("(?i)-CMDs").unwrap();

                let mut filename = Path::new(trimmed)
                    .file_name()
                    .unwrap_or_else(|| OsStr::new(trimmed))
                    .to_string_lossy()
                    .to_string();

                filename = cmds_pattern
                    .replace_all(&json_pattern.replace_all(&filename, "").to_string(), "")
                    .to_string();

                filename = format!("{}-CMDs.json", filename);

                // Always anchor under JSONs/cmds/
                let full_path = app.cmds_dir.join(filename);

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

fn open_editor(path: &str) -> std::io::Result<std::process::ExitStatus> {
    #[cfg(target_os = "windows")]
    {
        Command::new("nvim").arg(path).status()
    }

    #[cfg(not(target_os = "windows"))]
    {
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nvim".to_string());
        Command::new("sh")
            .arg("-c")
            .arg(format!("{} {}", editor, path))
            .status()
    }
}

fn handle_key_event(app: &mut App, terminal: &mut DefaultTerminal) -> Result<bool> {
    if let Event::Key(key) = event::read()? {
        if key.kind != KeyEventKind::Press {
            return Ok(false);
        }
        match key.code {
            KeyCode::Esc => return Ok(true),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(entry_index) = app.selected_entry_index() {
                    let removed_id = app.entries[entry_index].id.clone();
                    app.entries.remove(entry_index);

                    app.rebuild_entry_index();

                    for chain in &mut app.chains {
                        chain.steps.retain(|step_id| step_id != &removed_id);
                    }
                    app.chains.retain(|c| c.steps.len() >= 2);

                    search(app, true);

                    if app.results.is_empty() {
                        app.list_state.select(None);
                    } else {
                        let current = app.list_state.selected().unwrap_or(0);
                        let new_sel = current.min(app.results.len() - 1);
                        app.list_state.select(Some(new_sel));
                    }
                }
                app.current_chain_index = 0;
            }
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let mut entry = Entry::new();

                let mut rng = rand::rng();
                let id = format!("{:08x}", rng.random::<u32>());
                entry.id = id;

                // Disable raw mode and leave alternate screen
                disable_raw_mode()?;
                execute!(stdout(), LeaveAlternateScreen, Show)?;

                // Toggle for prefilled template
                let out = entry_to_template(&entry);
                fs::write(get_temp_path(), out)?;

                open_editor(get_temp_path()).expect("Failed to execute editor");
                let updated_entry = parse_template(&entry.id, &app)?;

                fs::remove_file(get_temp_path())?;

                app.entries.push(updated_entry);
                app.rebuild_entry_index();
                search(app, false);

                let new_entry_idx = app.entries.len() - 1;
                if let Some(filtered_pos) = app.results.iter().position(|&i| i == new_entry_idx) {
                    app.list_state.select(Some(filtered_pos));
                }
                app.current_chain_index = 0;

                // Re-enable raw mode and re-enter alternate screen
                enable_raw_mode()?;
                execute!(stdout(), EnterAlternateScreen, Hide)?;
                terminal.clear()?;
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if !app.is_chain_edit_mode {
                    if let Some(entry) = app.selected_entry() {
                        app.prev_selected_entry_id = entry.id.clone();
                    }
                }
                app.is_chain_edit_mode = !app.is_chain_edit_mode;
                app.query.clear();
                app.cursor_index = 0;
                search(app, false);
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
                    fs::write(get_temp_path(), out)?;

                    let _ = open_editor(get_temp_path());

                    let updated_entry = parse_template(&entry.id, &app)?;
                    app.entries[selected_index] = updated_entry;

                    fs::remove_file(get_temp_path())?;

                    // Re-enable raw mode and re-enter alternate screen
                    enable_raw_mode()?;
                    execute!(stdout(), EnterAlternateScreen, Hide)?;
                    terminal.clear()?;
                }
            }
            KeyCode::Enter if app.is_chain_edit_mode => {
                let prev_id = app.prev_selected_entry_id.clone();

                if let Some(selected) = app.selected_entry() {
                    let selected_id = selected.id.clone();

                    if let Some(chain) = app.find_chain_for_entry_mut(&prev_id) {
                        if !chain.steps.contains(&selected_id) {
                            chain.steps.push(selected_id);
                        }
                    } else {
                        // Create new chain
                        let mut rng = rand::rng();
                        let chain_id = format!("{:08x}", rng.random::<u32>());

                        app.chains.push(Chain {
                            id: chain_id,
                            steps: vec![prev_id, selected_id],
                            name: String::from("new-chain"),
                            description: String::from("new-chain"),
                        });
                    }
                }
                app.current_chain_index = 0;
                app.is_chain_edit_mode = false;
                app.query.clear();
                app.cursor_index = 0;
                search(app, true);
            }
            KeyCode::Enter => {
                if let Some(entry) = app.selected_entry() {
                    copy_to_clipboard(&entry.cmd);
                }
                return Ok(true);
            }
            KeyCode::BackTab => {
                app.mode = match app.mode {
                    SearchMode::HEADING => SearchMode::CMD,
                    SearchMode::TITLE => SearchMode::HEADING,
                    SearchMode::ALL => SearchMode::TITLE,
                    SearchMode::CMD => SearchMode::ALL,
                };
                search(app, true);
            }
            KeyCode::Tab => {
                app.mode = match app.mode {
                    SearchMode::CMD => SearchMode::HEADING,
                    SearchMode::HEADING => SearchMode::TITLE,
                    SearchMode::TITLE => SearchMode::ALL,
                    SearchMode::ALL => SearchMode::CMD,
                };
                search(app, true);
            }

            KeyCode::Char('[') => {
                app.top_tab = if app.top_tab == 0 { 1 } else { 0 };
            }
            KeyCode::Char(']') => {
                app.top_tab = (app.top_tab + 1) % 2;
                // Render new UI
            }

            KeyCode::Down => {
                let len = app.results.len();
                if len > 0 {
                    let i = app
                        .list_state
                        .selected()
                        .map(|i| if i == len - 1 { len - 1 } else { i + 1 })
                        .unwrap_or(0);
                    app.list_state.select(Some(i));
                }
                app.current_chain_index = 0;
            }
            KeyCode::Up => {
                let len = app.results.len();
                if len > 0 {
                    let i = app
                        .list_state
                        .selected()
                        .map(|i| if i == 0 { 0 } else { i - 1 })
                        .unwrap_or(0);
                    app.list_state.select(Some(i));
                }
                app.current_chain_index = 0;
            }
            KeyCode::Left => {
                app.cursor_index = app.cursor_index.saturating_sub(1);
            }
            KeyCode::Right => {
                if app.cursor_index < app.query.len() {
                    app.cursor_index += 1;
                }
            }
            KeyCode::Backspace => {
                if app.cursor_index > 0 {
                    app.cursor_index -= 1;
                    app.query.remove(app.cursor_index);
                    search(app, true);
                }
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                app.query.insert(app.cursor_index, c);
                app.cursor_index += 1;
                search(app, true);
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
        .style(Style::default().fg(C_DIM))
        .highlight_style(
            Style::default()
                .fg(C_ACCENT)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
        .divider(" ")
        .padding("  ", "  ");

    frame.render_widget(tabs, area);
}

fn render_search_input(frame: &mut Frame, area: Rect, app: &App) {
    let mode_title = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            format!(" {} ", app.mode.to_string()),
            Style::default()
                .bg(C_ACCENT)
                .fg(C_ACCENT_BG)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ]);

    let mut block = Block::default()
        .title_top(mode_title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(C_BORDER));

    if app.is_chain_edit_mode {
        block = block.title_bottom(Line::from("CHAIN_EDIT_MODE").left_aligned());
    }

    let line = if app.query.is_empty() {
        Line::from(vec![Span::raw("  ")])
    } else {
        Line::from(vec![
            Span::raw("  "),
            Span::styled(app.query.as_str(), Style::default().fg(C_FG_BRIGHT)),
        ])
    };

    let input = Paragraph::new(line).block(block);

    frame.set_cursor_position(Position::new(
        area.x + 1 + 2 + app.cursor_index as u16, // border + padding + index
        area.y + 1,                               // border
    ));
    frame.render_widget(input, area);
}

fn render_main(frame: &mut Frame, area: Rect, app: &mut App) {
    let cols =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).split(area);

    render_results(frame, cols[0], app);

    let right_rows = Layout::vertical([Constraint::Percentage(60), Constraint::Min(0)]).split(cols[1]);

    render_detail(frame, right_rows[0], app);

    let entry_id = match app.selected_entry() {
        Some(e) => e.id.clone(),
        None => return,
    };

    let chains = app.find_chains_for_entry(&entry_id);

    let chain_entries: Vec<Vec<&Entry>> = chains
        .iter()
        .map(|chain| app.resolve_chain_steps(chain))
        .filter(|steps| !steps.is_empty())
        .collect();

    let current_chain = chain_entries
        .get(app.current_chain_index)
        .map(|v| v.as_slice())
        .unwrap_or(&[]);

    render_chain(frame, right_rows[1], current_chain, &entry_id);
}

fn search(app: &mut App, reset_selection: bool) {
    app.current_chain_index = 0;
    let previous_selection = app.list_state.selected();
 
    if app.query.trim().is_empty() {
        app.results = (0..app.entries.len()).collect();
        return;
    }
 
    let mut matcher = nucleo::Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(&app.query, CaseMatching::Ignore, Normalization::Smart);
 
    let mut scored: Vec<(usize, u32)> = Vec::new();
 
    for (i, entry) in app.entries.iter().enumerate() {
        match app.mode {
            SearchMode::CMD => {
                // every atom must be a literal substring of the command.
                if !atoms_present(&app.query, &[entry.cmd.as_str()]) {
                    continue;
                }
                let mut buf = Vec::new();
                let haystack = nucleo::Utf32Str::new(entry.cmd.as_str(), &mut buf);
                if let Some(score) = pattern.score(haystack, &mut matcher) {
                    scored.push((i, score));
                }
            }
            SearchMode::TITLE => {
                if !atoms_present(&app.query, &[entry.title.as_str()]) {
                    continue;
                }
                let mut buf = Vec::new();
                let haystack = nucleo::Utf32Str::new(entry.title.as_str(), &mut buf);
                if let Some(score) = pattern.score(haystack, &mut matcher) {
                    scored.push((i, score));
                }
            }
            SearchMode::HEADING => {
                let temp_string = entry.heading_path.join(" > ");
                if !atoms_present(&app.query, &[temp_string.as_str()]) {
                    continue;
                }
                let mut buf = Vec::new();
                let haystack = nucleo::Utf32Str::new(&temp_string, &mut buf);
                if let Some(score) = pattern.score(haystack, &mut matcher) {
                    scored.push((i, score));
                }
            }
            SearchMode::ALL => {
                let heading_str = entry.heading_path.join(" > ");
                if !atoms_present(
                    &app.query,
                    &[heading_str.as_str(), entry.title.as_str(), entry.cmd.as_str()],
                ) {
                    continue;
                }
 
                let mut h_buf = Vec::new();
                let h_hay = nucleo::Utf32Str::new(&heading_str, &mut h_buf);
                let h_score = pattern.score(h_hay, &mut matcher).unwrap_or(0);
 
                let mut t_buf = Vec::new();
                let t_hay = nucleo::Utf32Str::new(entry.title.as_str(), &mut t_buf);
                let t_score = pattern.score(t_hay, &mut matcher).unwrap_or(0);
 
                let mut c_buf = Vec::new();
                let c_hay = nucleo::Utf32Str::new(entry.cmd.as_str(), &mut c_buf);
                let c_score = pattern.score(c_hay, &mut matcher).unwrap_or(0);
 
                let combined = (h_score.saturating_mul(3))
                    .saturating_add(t_score.saturating_mul(2))
                    .saturating_add(c_score);
 
                scored.push((i, combined.max(1)));
            }
        }
    }
 
    scored.sort_by(|a, b| b.1.cmp(&a.1));
 
    // Drop results scoring below 40% of the top hit — kills the fuzzy noise
    if let Some(&(_, top_score)) = scored.first() {
        if top_score > 0 {
            let threshold = top_score * 2 / 5;
            scored.retain(|&(_, s)| s >= threshold);
        }
    }
 
    app.results = scored.into_iter().map(|(i, _)| i).collect();
 
    if app.results.is_empty() {
        app.list_state.select(None);
    } else if reset_selection {
        app.list_state.select(Some(0));
    } else {
        match previous_selection {
            None => app.list_state.select(None),
            Some(i) => app.list_state.select(Some(i.min(app.results.len() - 1))),
        }
    }
}
fn render_results(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title_bottom(format!(" RESULTS  {} entries ", app.entries.len()))
        .border_style(Style::default().fg(C_BORDER));

    let inner_width = area.width.saturating_sub(2) as usize;
    let cmd_width = inner_width.saturating_sub(4);

    let items: Vec<ListItem> = app
        .results
        .iter()
        .filter_map(|&i| app.entries.get(i))
        .map(|e| {
            let breadcrumb = e.heading_path.join(" › ");

            let mut lines: Vec<Line> = Vec::new();

            for chunk in textwrap::wrap(&breadcrumb, inner_width.max(1)) {
                lines.push(Line::from(Span::styled(
                    chunk.into_owned(),
                    Style::default().fg(C_DIM),
                )));
            }

            let wrapped = textwrap::wrap(&e.cmd, cmd_width.max(1));
            for (idx, chunk) in wrapped.iter().enumerate() {
                let prefix = if idx == 0 { "  $ " } else { "    " };
                lines.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(C_DIM)),
                    Span::styled(chunk.to_string(), Style::default().fg(C_FG_BRIGHT)),
                ]));
            }

            lines.push(Line::from(""));

            ListItem::new(lines)
        })
        .collect();

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(C_HIGHLIGHT_BG)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_stateful_widget(list, area, &mut app.list_state);
}
fn render_chain(frame: &mut Frame, area: Rect, chain_entries: &[&Entry], selected_entry_id: &str) {
    if chain_entries.is_empty() {
        let p = Paragraph::new("No chain for this command")
            .style(Style::default().fg(C_DIM))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(C_BORDER))
                    .title(" ATTACK CHAIN ")
                    .title_alignment(Alignment::Center),
            );

        frame.render_widget(p, area);
        return;
    };

    let lines: Vec<Line> = chain_entries
        .iter()
        .flat_map(|chain_entry| {
            let style = if selected_entry_id == chain_entry.id {
                Style::default()
                    .fg(C_FG_BRIGHT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(C_DIM)
            };
            vec![
                Line::from(""),
                Line::from(Span::styled(chain_entry.cmd.as_str(), style)),
                Line::from(""),
            ]
        })
        .collect();

    let chain_widget: Paragraph<'_> = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .title_top(" ATTACK CHAIN ")
            .title_alignment(Alignment::Center)
            .border_style(Style::default().fg(C_BORDER)),
    );

    frame.render_widget(chain_widget, area);
}

fn render_detail(frame: &mut Frame, area: Rect, app: &App) {
    let selected = app.selected_entry();

    let Some(entry) = selected else {
        let p = Paragraph::new(vec![Line::from(""), Line::from("Select an entry")])
            .style(Style::default().fg(C_DIM))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(C_BORDER))
                    .title(" DESCRIPTION ")
                    .title_alignment(Alignment::Center),
            );

        frame.render_widget(p, area);
        return;
    };

    let lines_iter = entry.description.lines().map(|l|  Line::from(Span::styled(
            l,
            Style::default().fg(C_DESC),
        )));

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(entry.title.as_str(), Style::default().fg(C_TITLE))),
    ];

    lines.extend(lines_iter);

    // Top card: breadcrumb + title + primary command
    // let breadcrumb = entry.heading_path.join(" › ");
    let top = Paragraph::new(lines)
    .wrap(Wrap { trim: false })
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(C_BORDER))
            .title(" DESCRIPTION ")
            .title_alignment(Alignment::Center),
    );
    frame.render_widget(top, area);
}
