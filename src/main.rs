use std::{
    collections::{HashMap, HashSet},
    error::Error,
    ffi::OsStr,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};
use strum::Display;

use color_eyre::{Result, eyre::eyre};
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
pub struct EntriesFile {
    pub entries: Vec<Entry>,
}
impl Default for Entry {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntryRecord {
    id: String,
    title: String,
    cmd: String,
    description: String,
    source_file: PathBuf,
    heading_path: String, // semicolon-delimited for CSV
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChainRecord {
    id: String,
    name: String,
    description: String,
    steps: String, // semicolon-delimited entry IDs for CSV
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

fn serialize_steps<S>(steps: &[String], serializer: S) -> std::result::Result<S::Ok, S::Error>
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

#[derive(Debug, Display)]
pub enum SearchMode {
    CMD,
    HEADING,
    TITLE,
    ALL,
}

fn array_to_field(arr: &[String]) -> String {
    arr.iter()
        .map(|s| to_csv_field(s)) // escape newlines etc. for EACH element before joining
        .collect::<Vec<_>>()
        .join(";")
}
fn field_to_array(s: &str) -> Vec<String> {
    s.split(';')
        .map(from_csv_field) // unescape after splitting to get original strings back, including any semicolons that were escaped
        .collect()
}
fn to_csv_field(s: &str) -> String {
    s.replace('\\', "\\\\") // escape backslashes FIRST or you'll double-escape
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn from_csv_field(s: &str) -> String {
    s.replace("\\n", "\n")
        .replace("\\r", "\r")
        .replace("\\\\", "\\") // unescape backslashes LAST pleaseeee
}
pub fn json_to_csv(path: &Path) -> Result<(), Box<dyn Error>> {
    let text = std::fs::read_to_string(path)?;
    let entries_file: EntriesFile = serde_json::from_str(&text)?;
    let records: Vec<EntryRecord> = entries_file
        .entries
        .into_iter()
        .map(|e| EntryRecord {
            id: e.id,
            title: e.title,
            cmd: to_csv_field(&e.cmd),
            description: to_csv_field(&e.description),
            source_file: e.source_file,
            heading_path: array_to_field(&e.heading_path),
        })
        .collect();
    let output_path = path.with_extension("csv");
    let mut wtr = csv::Writer::from_path(output_path)?;
    for record in records {
        wtr.serialize(record)?;
    }
    wtr.flush()?;
    Ok(())
}

pub fn csv_to_json(path: &Path) -> Result<(), Box<dyn Error>> {
    let mut rdr = csv::Reader::from_path(path)?;
    let mut entries: Vec<Entry> = Vec::new();
    for result in rdr.deserialize() {
        let mut record: EntryRecord = result?;
        record.cmd = from_csv_field(&record.cmd);
        record.description = from_csv_field(&record.description);
        let entry = Entry {
            id: record.id,
            title: record.title,
            cmd: record.cmd,
            description: record.description,
            source_file: record.source_file,
            heading_path: field_to_array(&record.heading_path),
        };
        entries.push(entry);
    }
    let entries_file = EntriesFile { entries };
    let json_data = serde_json::to_string_pretty(&entries_file)?;
    let output_path = path.with_extension("json");
    std::fs::write(output_path, json_data)?;
    Ok(())
}
fn chain_json_to_csv(path: &Path) -> Result<(), Box<dyn Error>> {
    // Parse ChainsFile, map each Chain → ChainRecord (steps via array_to_field)
    let json_data = std::fs::read_to_string(path)?;
    let chains_file: ChainsFile = serde_json::from_str(&json_data)?;
    let records: Vec<ChainRecord> = chains_file
        .chains
        .into_iter()
        .map(|chain| ChainRecord {
            id: chain.id,
            name: chain.name,
            description: to_csv_field(&chain.description),
            steps: array_to_field(&chain.steps),
        })
        .collect();
    // Write CSV
    let output_path = path.with_extension("csv");
    let mut wtr = csv::Writer::from_path(output_path)?;
    for record in records {
        wtr.serialize(record)?;
    }
    wtr.flush()?;
    Ok(())
}

fn chain_csv_to_json(path: &Path) -> Result<(), Box<dyn Error>> {
    // Read CSV into ChainRecord -> map back to Chain with deserialization of steps field via field_to_array
    let mut rdr = csv::Reader::from_path(path)?;
    let mut chains: Vec<Chain> = Vec::new();
    for result in rdr.deserialize() {
        let record: ChainRecord = result?;
        let chain = Chain {
            id: record.id,
            name: record.name,
            description: from_csv_field(&record.description),
            steps: field_to_array(&record.steps),
        };
        chains.push(chain);
    }
    let chains_file = ChainsFile { chains };
    let json_data = serde_json::to_string_pretty(&chains_file)?;
    let output_path = path.with_extension("json");
    // Write JSON
    std::fs::write(output_path, json_data)?;
    Ok(())
}

fn ensure_cmd_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut json_paths: HashSet<PathBuf> = HashSet::new();

    for dir_entry in fs::read_dir(dir)? {
        let path = dir_entry?.path();
        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => {
                if !path.with_extension("csv").exists() {
                    json_to_csv(&path).map_err(|e| eyre!("{e}"))?;
                }
                json_paths.insert(path);
            }
            Some("csv") => {
                let json_path = path.with_extension("json");
                if !json_path.exists() {
                    csv_to_json(&path).map_err(|e| eyre!("{e}"))?;
                }
                json_paths.insert(json_path);
            }
            _ => continue,
        }
    }

    Ok(json_paths.into_iter().collect())
}

fn ensure_chain_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut json_paths: HashSet<PathBuf> = HashSet::new();

    for dir_entry in fs::read_dir(dir)? {
        let path = dir_entry?.path();
        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => {
                if !path.with_extension("csv").exists() {
                    chain_json_to_csv(&path).map_err(|e| eyre!("{e}"))?;
                }
                json_paths.insert(path);
            }
            Some("csv") => {
                let json_path = path.with_extension("json");
                if !json_path.exists() {
                    chain_csv_to_json(&path).map_err(|e| eyre!("{e}"))?;
                }
                json_paths.insert(json_path);
            }
            _ => continue,
        }
    }

    Ok(json_paths.into_iter().collect())
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
            query: String::new(),
            mode: SearchMode::ALL,
            top_tab: 0,
            list_state,
            results: vec![],
            cursor_index: 0,
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

    pub fn sanitize_source_path(&self, raw: &Path) -> PathBuf {
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
                if let Some(&index) = self.entry_index.get(entry_id)
                    && let Some(entry) = self.entries.get(index)
                {
                    source_entry = Some(entry);
                    break;
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
    for json_path in ensure_cmd_files(&cmds_dir)? {
        let text = fs::read_to_string(&json_path)?;
        let ef: EntriesFile = serde_json::from_str(&text)?;
        for mut e in ef.entries {
            e.source_file = json_path.clone();
            entries.push(e);
        }
    }

    let mut chains: Vec<Chain> = Vec::new();
    for json_path in ensure_chain_files(&chains_dir)? {
        let text = fs::read_to_string(&json_path)?;
        let cf: ChainsFile = serde_json::from_str(&text)?;
        chains.extend(cf.chains);
    }

    let valid_ids: HashSet<String> = entries.iter().map(|e| e.id.clone()).collect();
    chains.retain(|chain| chain.steps.iter().any(|id| valid_ids.contains(id)));

    let mut app = App::new(entries, chains, cmds_dir, chains_dir);

    ratatui::run(|terminal| ui::run_event_loop(terminal, &mut app))?;

    app.write_entries_to_json()?;
    app.write_chains_to_json()?;

    Ok(())
}
