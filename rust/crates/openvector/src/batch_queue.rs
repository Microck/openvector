//! Native batch queue identity, selection and output-name planning.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRecord {
    pub filename: String,
    pub selected: bool,
    pub output_filename: String,
    pub should_process: bool,
    pub button_index: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSnapshot {
    pub export_directory: String,
    pub export_format: String,
    pub processing_index: i32,
    pub accepting_edits: bool,
    pub queue: Vec<BatchRecord>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverwriteDecision { Overwrite, Skip, Cancel }
impl TryFrom<&str> for OverwriteDecision {
    type Error = String;
    fn try_from(value: &str) -> Result<Self, String> {
        match value { "overwrite" => Ok(Self::Overwrite), "skip" => Ok(Self::Skip), "cancel" => Ok(Self::Cancel),
            _ => Err("Batch overwrite decision must be overwrite, skip or cancel.".into()) }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BatchPreparation { pub overwrites: Vec<String>, pub processable: usize }
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchDecision { pub should_start: bool, pub processable: usize }
#[derive(Clone, Debug)]
pub struct BatchQueue {
    records: Vec<BatchRecord>,
    pub working_directory: String,
    pub export_directory: String,
    pub export_format: String,
    pub processing_index: i32,
    accepting_edits: bool,
}
impl BatchQueue {
    pub fn new(application_directory: &str, working_directory: &str) -> Self {
        Self { records: Vec::new(), working_directory: working_directory.into(),
            export_directory: application_directory.replace('\\', "/"), export_format: "ai".into(),
            processing_index: 0, accepting_edits: true }
    }
    pub fn snapshot(&self) -> BatchSnapshot {
        let mut queue = self.records.clone();
        for (index, record) in queue.iter_mut().enumerate() { record.button_index = index; }
        BatchSnapshot { export_directory: self.export_directory.clone(), export_format: self.export_format.clone(),
            processing_index: self.processing_index, accepting_edits: self.accepting_edits, queue }
    }
    pub fn add_files(&mut self, filenames: &[String]) -> Vec<String> {
        let mut known: HashSet<String> = self.records.iter().map(|r| r.filename.clone()).collect();
        let mut duplicates = Vec::new();
        for filename in filenames {
            if !known.insert(filename.clone()) { duplicates.push(filename.clone()); continue; }
            self.records.push(BatchRecord { filename: filename.clone(), selected: false,
                output_filename: String::new(), should_process: false, button_index: self.records.len() });
            self.export_directory = windows_parent(&self.working_directory, filename);
        }
        duplicates
    }
    fn record_mut(&mut self, index: usize) -> Result<&mut BatchRecord, String> {
        self.records.get_mut(index).ok_or_else(|| "Batch record index is out of range.".into())
    }
    pub fn select(&mut self, index: usize, selected: bool) -> Result<(), String> {
        let editable = self.accepting_edits;
        let record = self.record_mut(index)?;
        if editable { record.selected = selected; }
        Ok(())
    }
    pub fn set_accepting_edits(&mut self, value: bool) {
        self.accepting_edits = value;
        if !value { for record in &mut self.records { record.selected = false; } }
    }
    pub fn remove(&mut self, index: usize) -> Result<(), String> {
        if self.record_mut(index)?.selected { self.remove_selected(); } else { self.records.remove(index); }
        Ok(())
    }
    pub fn remove_selected(&mut self) { self.records.retain(|r| !r.selected); }
    pub fn clear(&mut self) { self.records.clear(); }
    pub fn prepare(&mut self, mut exists: impl FnMut(&str) -> bool) -> BatchPreparation {
        let mut names = HashSet::new();
        let mut overwrites = Vec::new();
        for record in &mut self.records {
            let filename = record.filename.rsplit(['/', '\\']).next().unwrap_or("");
            let base = filename.rfind('.').map_or(filename, |dot| &filename[..dot]);
            let mut stem = base.to_owned();
            let mut suffix = 0;
            while !names.insert(stem.to_uppercase()) { suffix += 1; stem = format!("{base}_{suffix}"); }
            record.output_filename = format!("{}/{stem}.{}", self.export_directory, self.export_format);
            record.should_process = !exists(&record.output_filename);
            if !record.should_process { overwrites.push(record.output_filename.clone()); }
        }
        BatchPreparation { processable: self.records.len() - overwrites.len(), overwrites }
    }
    pub fn decide_overwrite(&mut self, decision: OverwriteDecision) -> BatchDecision {
        let mut processable = 0;
        for record in &mut self.records {
            if decision == OverwriteDecision::Overwrite { record.should_process = true; }
            processable += usize::from(record.should_process);
        }
        BatchDecision { should_start: decision != OverwriteDecision::Cancel && processable > 0, processable }
    }
}

// QFileInfo applies Windows roots and both separators even on non-Windows hosts.
fn windows_parent(base: &str, filename: &str) -> String {
    let base = base.replace('\\', "/");
    let filename = filename.replace('\\', "/");
    let drive = |s: &str| s.as_bytes().get(1) == Some(&b':');
    let joined = if filename.starts_with("//") || (drive(&filename) && filename.as_bytes().get(2) == Some(&b'/')) {
        filename
    } else if filename.starts_with('/') {
        if drive(&base) { format!("{}{filename}", &base[..2]) } else { filename }
    } else if drive(&filename) {
        if drive(&base) && base[..2].eq_ignore_ascii_case(&filename[..2]) {
            format!("{base}/{}", &filename[2..])
        } else { format!("{}/{}", &filename[..2], &filename[2..]) }
    } else { format!("{base}/{filename}") };
    let (prefix, rest, protected) = if joined.starts_with("//") { ("//".to_owned(), &joined[2..], 2) }
        else if drive(&joined) { (format!("{}/", &joined[..2]), joined[2..].trim_start_matches('/'), 0) }
        else if joined.starts_with('/') { ("/".to_owned(), joined.trim_start_matches('/'), 0) }
        else { (String::new(), joined.as_str(), 0) };
    let mut parts = Vec::new();
    for part in rest.split('/') {
        match part { "" | "." => {}, ".." if parts.len() > protected => { parts.pop(); }, ".." => {}, _ => parts.push(part) }
    }
    if parts.len() > protected { parts.pop(); }
    format!("{prefix}{}", parts.join("/"))
}
