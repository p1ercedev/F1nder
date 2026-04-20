use std::{
    collections::HashMap,
    ffi::OsStr,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};

use color_eyre::Result;
use ratatui::{layout::Position, widgets::ListState};
use serde::{Deserialize, Serialize};

mod ui;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub cmd: String,
    pub description: String,
    pub source_file: PathBuf,
    pub heading_path: Vec<String>,
}
impl Entry {
    pub fn new() -> Self {
        Self {
            id: String::new(),
            title: String::new(),
            cmd: String::new(),
            description: String::new(),
            source_file: PathBuf::new(),
            heading_path: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntriesFile {
    pub entries: Vec<Entry>,
}

pub struct App {
    pub top_tab: usize,
    pub entries: Vec<Entry>,
    pub query: String,
    pub mode: usize,
    pub selected: usize,
    pub should_quit: bool,
    pub list_state: ListState,
    pub results: Vec<usize>,
    pub cursor_index: usize,
}

impl App {
    pub fn new(entries: Vec<Entry>) -> Self {
        let mut list_state = ListState::default();
        if !entries.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            entries,
            query: String::new(),
            mode: 0,
            selected: 0,
            should_quit: false,
            top_tab: 0,
            list_state,
            results: vec![],
            cursor_index: 0,
        }
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.list_state
            .selected()
            .and_then(|filtered_index| self.results.get(filtered_index))
            .and_then(|&i| self.entries.get(i))
    }
    pub fn selected_entry_index(&self) -> Option<usize> {
        self.list_state
            .selected()
            .and_then(|i| self.results.get(i).copied())
    }

    pub fn write_entries_to_json(&self) -> Result<()> {
        let mut entries_by_filename: HashMap<PathBuf, EntriesFile> = HashMap::new();
        for entry in &self.entries {
            entries_by_filename
                .entry(entry.source_file.clone())
                .or_insert(EntriesFile {
                    entries: Vec::<Entry>::new(),
                })
                .entries
                .push(entry.clone());
        }

        for (filepath, ef) in &entries_by_filename {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(filepath)?;
            serde_json::to_writer_pretty(&mut file, &ef)?;
        }
        Ok(())
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let mut entries: Vec<Entry> = Vec::new();

    for dir_entry in fs::read_dir("JSONs")? {
        let path = dir_entry?.path();
        if path.extension() != Some(OsStr::new("json")) {
            continue;
        }

        let text = fs::read_to_string(&path)?;
        let ef: EntriesFile = serde_json::from_str(&text)?;

        for mut e in ef.entries {
            e.source_file = path.clone();
            entries.push(e);
        }
    }

    let mut app = App::new(entries);

    ratatui::run(|terminal| ui::run_event_loop(terminal, &mut app))?;

    app.write_entries_to_json()?;

    Ok(())
}
