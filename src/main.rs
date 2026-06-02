use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fs::{self, OpenOptions},
    path::PathBuf,
    sync::OnceLock,
};
use strum::Display;

use color_eyre::{Result, eyre::eyre};
use ratatui::widgets::ListState;
use serde::{Deserialize, Serialize};
mod ui;

static TEMP_FILE_PATH: OnceLock<String> = OnceLock::new();

pub fn get_temp_path() -> &'static str {
    TEMP_FILE_PATH.get_or_init(|| {
        #[cfg(target_os = "windows")]
        return std::env::var("TEMP").unwrap_or("C:\\Windows\\Temp".into()) + "\\prev_search.txt";

        #[cfg(not(target_os = "windows"))]
        return "/tmp/prev_search.txt".to_string();
    })
}
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
    HEADING,
    TITLE,
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
    pub cmds_dir: PathBuf,
    pub chains_dir: PathBuf,
}

impl App {
    pub fn new(
        entries: Vec<Entry>,
        chains: Vec<Chain>,
        cmds_dir: PathBuf,
        chains_dir: PathBuf,
    ) -> Self {
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
            query: fs::read_to_string(get_temp_path())
                .unwrap_or(String::new())
                .lines()
                .nth(0)
                .unwrap_or("")
                .to_owned(),
            mode: match fs::read_to_string(get_temp_path())
                .unwrap_or(String::new())
                .lines()
                .nth(1)
                .unwrap_or("ALL")
            {
                "CMD" => SearchMode::CMD,
                "HEADING" => SearchMode::HEADING,
                "TITLE" => SearchMode::TITLE,
                _ => SearchMode::ALL,
            },
            top_tab: 0,
            list_state,
            results: vec![],
            cursor_index: fs::read_to_string(get_temp_path())
                .unwrap_or(String::new())
                .lines()
                .nth(0)
                .unwrap_or("")
                .len(),
            chains,
            entry_index,
            is_chain_edit_mode: false,
            prev_selected_entry_id: String::from(""),
            current_chain_index: 0,
            cmds_dir,
            chains_dir,
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

    pub fn sanitize_source_path(&self, raw: &PathBuf) -> PathBuf {
        let filename = raw
            .file_name()
            .unwrap_or_else(|| OsStr::new("unknown-CMDs.json"));
        self.cmds_dir.join(filename)
    }

    pub fn write_entries_to_json(&self) -> Result<()> {
        let mut entries_by_filename: HashMap<PathBuf, EntriesFile> = HashMap::new();

        for entry in &self.entries {
            let safe_path = self.sanitize_source_path(&entry.source_file);
            entries_by_filename
                .entry(safe_path)
                .or_insert(EntriesFile { entries: vec![] })
                .entries
                .push(entry.clone());
        }

        for (filepath, ef) in &entries_by_filename {
            // println!("Writing to {}", filepath.to_string_lossy().as_ref());
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(filepath)?;
            serde_json::to_writer_pretty(&mut file, &ef)?;
        }

        if !entries_by_filename.is_empty() {
            for dir_entry in fs::read_dir(&self.cmds_dir)? {
                let path = dir_entry?.path();
                if path.extension() != Some(OsStr::new("json")) {
                    continue;
                }
                if !entries_by_filename.contains_key(&path) {
                    fs::remove_file(&path)?;
                }
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
                    let safe_path = self.sanitize_source_path(&entry.source_file);
                    let stem = safe_path.file_stem().unwrap_or_default().to_string_lossy();
                    self.chains_dir.join(format!("{}-chains.json", stem))
                }
                None => self.chains_dir.join("orphaned-chains.json"),
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

        if !chains_by_filename.is_empty() && self.chains_dir.exists() {
            for dir_entry in fs::read_dir(&self.chains_dir)? {
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
            .collect()
    }

    pub fn resolve_chain_steps<'a>(&'a self, chain: &Chain) -> Vec<&'a Entry> {
        chain
            .steps
            .iter()
            .filter_map(|entry_id| {
                self.entry_index
                    .get(entry_id)
                    .and_then(|i| self.entries.get(*i))
            })
            .collect()
    }

    pub fn save_prev_search(&self) {
        let _ = fs::write(get_temp_path(), format!("{}\n{}", self.query, self.mode));
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let exe_path = std::env::current_exe()?;
    let root = exe_path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .ok_or_else(|| eyre!("Could not determine root path from executable location"))?
        .canonicalize()?;

    let cmds_dir = root.join("JSONs/cmds");
    let chains_dir = root.join("JSONs/chains");

    if !(cmds_dir.exists() && chains_dir.exists()) {
        return Err(eyre!(
            "Cannot find JSON dirs:\n  {}\n  {}",
            cmds_dir.display(),
            chains_dir.display()
        ));
    }

    let mut entries: Vec<Entry> = Vec::new();
    for dir_entry in fs::read_dir(&cmds_dir)? {
        let path = dir_entry?.path();
        if path.extension() != Some(OsStr::new("json")) {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        let ef: EntriesFile = serde_json::from_str(&text)?;
        for mut e in ef.entries {
            // Always override source_file with the canonical path we just read from.
            e.source_file = path.clone();
            entries.push(e);
        }
    }

    let mut chains: Vec<Chain> = Vec::new();
    for dir_entry in fs::read_dir(&chains_dir)? {
        let path = dir_entry?.path();
        if path.extension() != Some(OsStr::new("json")) {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        let cf: ChainsFile = serde_json::from_str(&text)?;
        chains.extend(cf.chains);
    }

    let valid_ids: HashSet<String> = entries.iter().map(|e| e.id.clone()).collect();
    chains.retain(|chain| chain.steps.iter().any(|id| valid_ids.contains(id)));

    let mut app = App::new(entries, chains, cmds_dir, chains_dir);

    ratatui::run(|terminal| ui::run_event_loop(terminal, &mut app))?;

    app.write_entries_to_json()?;
    app.write_chains_to_json()?;
    app.save_prev_search();

    Ok(())
}
