use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};
use strum::Display;

use color_eyre::{Result, eyre::Ok};
use ratatui::widgets::ListState;
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
pub struct Chain {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(
        deserialize_with = "deserialize_steps",
        serialize_with = "serialize_steps"
    )]
    pub steps: Vec<String>,
}
fn deserialize_steps<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct Step {
        entry_id: String,
    }
    let steps = Vec::<Step>::deserialize(deserializer)?;
    std::result::Result::Ok(steps.into_iter().map(|s| s.entry_id).collect())
}

fn serialize_steps<S>(steps: &Vec<String>, serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    #[derive(Serialize)]
    struct Step {
        entry_id: String,
    }
    let wrapped: Vec<Step> = steps
        .iter()
        .map(|id| Step {
            entry_id: id.clone(),
        })
        .collect();
    wrapped.serialize(serializer)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainsFile {
    pub chains: Vec<Chain>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntriesFile {
    pub entries: Vec<Entry>,
}

#[derive(Debug, Display)]
pub enum SearchMode {
    CMD,
    TITLE,
    HEADING,
    ALL,
}
pub struct App {
    pub top_tab: usize,
    pub entries: Vec<Entry>,
    pub query: String,
    pub mode: SearchMode,
    pub list_state: ListState,
    pub results: Vec<usize>,
    pub cursor_index: usize,
    pub chains: Vec<Chain>,
    pub entry_index: HashMap<String, usize>,
    pub is_chain_edit_mode: bool,
    pub prev_selected_entry_id: String,
    pub current_chain_index: usize,
}

impl App {
    pub fn new(entries: Vec<Entry>, chains: Vec<Chain>) -> Self {
        let mut list_state = ListState::default();
        if !entries.is_empty() {
            list_state.select(Some(0));
        }
        let entry_index = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i))
            .collect();
        Self {
            entries,
            query: String::new(),
            mode: SearchMode::CMD,
            top_tab: 0,
            list_state,
            results: vec![],
            cursor_index: 0,
            chains,
            entry_index,
            is_chain_edit_mode: false,
            prev_selected_entry_id: String::from(""),
            current_chain_index: 0,
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
    pub fn rebuild_entry_index(&mut self) {
        self.entry_index = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i))
            .collect();
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

        for dir_entry in fs::read_dir("JSONs/cmds")? {
            let path = dir_entry?.path();
            if path.extension() != Some(OsStr::new("json")) {
                continue;
            }
            if !entries_by_filename.contains_key(&path) {
                fs::remove_file(&path)?;
            }
        }
        Ok(())
    }

    pub fn write_chains_to_json(&mut self) -> Result<()> {
        let mut chains_by_filename: HashMap<PathBuf, ChainsFile> = HashMap::new();

        for chain in &self.chains {
            let mut source_entry: Option<&Entry> = None;
            for entry_id in &chain.steps {
                if let Some(&index) = self.entry_index.get(entry_id) {
                    if let Some(entry) = self.entries.get(index) {
                        source_entry = Some(entry);
                        break;
                    }
                }
            }
            let out_path = match source_entry {
                Some(entry) => {
                    let stem = entry
                        .source_file
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy();
                    PathBuf::from(format!("JSONs/chains/{}-chains.json", stem))
                }
                None => PathBuf::from("JSONs/chains/orphaned-chains.json"),
            };

            chains_by_filename
                .entry(out_path)
                .or_insert(ChainsFile { chains: vec![] })
                .chains
                .push(chain.clone());
        }

        for (filepath, cf) in &chains_by_filename {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(filepath)?;
            serde_json::to_writer_pretty(&mut file, &cf)?;
        }

        if Path::new("JSONs/chains").exists() {
            for dir_entry in fs::read_dir("JSONs/chains")? {
                let path = dir_entry?.path();
                if path.extension() != Some(OsStr::new("json")) {
                    continue;
                }
                if !chains_by_filename.contains_key(&path) {
                    fs::remove_file(&path)?;
                }
            }
        }
        Ok(())
    }

    pub fn find_chain_for_entry_mut(&mut self, entry_id: &str) -> Option<&mut Chain> {
        self.chains.iter_mut().find(|c| {
            c.steps
                .iter()
                .any(|current_entry_id| current_entry_id == entry_id)
        })
    }

    pub fn find_chains_for_entry<'a>(&'a self, entry_id: &str) -> Vec<&'a Chain> {
        self.chains
            .iter()
            .filter(|c| c.steps.iter().any(|step| step == entry_id))
            .collect::<Vec<&Chain>>()
    }

    pub fn resolve_chain_steps<'a>(&'a self, chain: &Chain) -> Vec<&'a Entry> {
        chain
            .steps
            .iter()
            .filter_map(|entry_id| {
                self.entry_index
                    .get(entry_id)
                    .and_then(|chain_step_index| self.entries.get(*chain_step_index))
            })
            .collect()
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let mut entries: Vec<Entry> = Vec::new();

    for dir_entry in fs::read_dir("JSONs/cmds")? {
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

    let mut chains: Vec<Chain> = Vec::new();
    let chains_dir = Path::new("JSONs/chains");

    if chains_dir.exists() {
        for dir_entry in fs::read_dir(chains_dir)? {
            let path = dir_entry?.path();
            if path.extension() != Some(OsStr::new("json")) {
                continue;
            }

            let text = fs::read_to_string(&path)?;
            let cf: ChainsFile = serde_json::from_str(&text)?;

            chains.extend(cf.chains);
        }
    }
    let valid_ids: HashSet<String> = entries.iter().map(|e| e.id.clone()).collect();

    chains.retain(|chain| {
        chain
            .steps
            .iter()
            .any(|step_id| valid_ids.contains(step_id))
    });
    let mut app = App::new(entries, chains);

    ratatui::run(|terminal| ui::run_event_loop(terminal, &mut app))?;

    app.write_entries_to_json()?;
    app.write_chains_to_json()?;

    Ok(())
}
