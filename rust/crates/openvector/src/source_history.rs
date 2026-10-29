//! Recent source metadata. Paths retain their caller-supplied spelling.
use serde_json::{Value, json};

const FILE_CAPACITY: usize = 3;
const DIRECTORY_CAPACITY: usize = 4;

pub fn format_file_size(bytes: u64) -> Result<String, &'static str> {
    if bytes > 9_007_199_254_740_991 {
        return Err("File size must be a nonnegative safe integer.");
    }
    let mut value = bytes as f64;
    let mut index = 0;
    // The native third scaled unit is named TB, not GB. Its threshold is strict.
    while value > 1000.0 && index < 3 {
        value *= 0.001;
        index += 1;
    }
    Ok(format!("{value:.1} {}", ["B", "kB", "MB", "TB"][index]))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentFile {
    pub filename: String,
    pub description: String,
    pub thumbnail_dimensions: [u32; 2],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceHistory {
    files: Vec<RecentFile>,
}

impl SourceHistory {
    pub fn from_snapshot(snapshot: &Value) -> Result<Self, &'static str> {
        let files = snapshot["files"].as_array()
            .filter(|files| files.len() <= FILE_CAPACITY)
            .ok_or("Invalid recent-file history.")?;
        if snapshot["capacity"].as_u64() != Some(FILE_CAPACITY as u64) {
            return Err("Invalid recent-file history.");
        }
        let mut history = Self::default();
        for entry in files {
            let filename = entry["filename"].as_str()
                .filter(|name| !name.is_empty())
                .ok_or("Invalid recent-file entry.")?;
            if history.files.iter().any(|file| file.filename == filename) {
                return Err("Invalid recent-file entry.");
            }
            let description = entry["description"].as_str()
                .ok_or("Invalid recent-file entry.")?;
            let dimensions = entry["thumbnailDimensions"].as_array()
                .filter(|dimensions| dimensions.len() == 2)
                .ok_or("Invalid recent-file entry.")?;
            let mut thumbnail_dimensions = [0; 2];
            for (target, value) in thumbnail_dimensions.iter_mut().zip(dimensions) {
                *target = value.as_u64().filter(|value| *value <= 32)
                    .ok_or("Invalid recent-file thumbnail dimensions.")? as u32;
            }
            if if thumbnail_dimensions.contains(&0) {
                thumbnail_dimensions != [0, 0]
            } else {
                thumbnail_dimensions.iter().max() != Some(&32)
            } {
                return Err("Invalid recent-file thumbnail dimensions.");
            }
            history.files.push(RecentFile {
                filename: filename.to_owned(),
                description: description.to_owned(),
                thumbnail_dimensions,
            });
        }
        Ok(history)
    }

    pub fn thumbnail_dimensions(width: u32, height: u32) -> Result<[u32; 2], &'static str> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err("Image dimensions must be positive signed 32-bit integers.");
        }
        let dimensions = if width >= height {
            [32, (u64::from(height) * 32 / u64::from(width)) as u32]
        } else {
            [(u64::from(width) * 32 / u64::from(height)) as u32, 32]
        };
        Ok(if dimensions.contains(&0) { [0, 0] } else { dimensions })
    }

    pub fn snapshot(&self) -> Value {
        json!({
            "capacity": FILE_CAPACITY,
            "files": self.files.iter().map(|file| json!({
                "filename": file.filename,
                "description": file.description,
                "thumbnailDimensions": file.thumbnail_dimensions,
            })).collect::<Vec<_>>(),
        })
    }

    pub fn add(&mut self, filename: &str, width: u32, height: u32, bytes: u64)
        -> Result<Value, &'static str>
    {
        if filename.is_empty() {
            return Err("Choose a filename for the recent image.");
        }
        // Validate before mutating the prior history.
        let thumbnail_dimensions = Self::thumbnail_dimensions(width, height)?;
        let description = format!("{width}x{height}, {}", format_file_size(bytes)?);
        self.files.retain(|file| file.filename != filename);
        self.files.insert(0, RecentFile {
            filename: filename.to_owned(), description, thumbnail_dimensions,
        });
        self.files.truncate(FILE_CAPACITY);
        Ok(self.snapshot())
    }

    pub fn remove(&mut self, filename: &str) -> Value {
        self.files.retain(|file| file.filename != filename);
        self.snapshot()
    }

    pub fn clear(&mut self) -> Value {
        self.files.clear();
        self.snapshot()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceDirectories {
    directories: Vec<String>,
}

impl SourceDirectories {
    pub fn from_snapshot(snapshot: &Value) -> Result<Self, &'static str> {
        let directories = snapshot["directories"].as_array()
            .filter(|directories| directories.len() <= DIRECTORY_CAPACITY)
            .ok_or("Invalid recent-directory history.")?;
        if snapshot["capacity"].as_u64() != Some(DIRECTORY_CAPACITY as u64) {
            return Err("Invalid recent-directory history.");
        }
        let mut history = Self::default();
        for directory in directories {
            let directory = directory.as_str().filter(|name| !name.is_empty())
                .ok_or("Invalid recent-directory history.")?;
            if history.directories.iter().any(|name| name == directory) {
                return Err("Invalid recent-directory history.");
            }
            history.directories.push(directory.to_owned());
        }
        Ok(history)
    }

    pub fn add(&mut self, directory: &str) -> Result<Value, &'static str> {
        if directory.is_empty() {
            return Err("Choose a recent image directory.");
        }
        self.directories.retain(|name| name != directory);
        self.directories.insert(0, directory.to_owned());
        self.directories.truncate(DIRECTORY_CAPACITY);
        Ok(self.snapshot())
    }

    pub fn snapshot(&self) -> Value {
        json!({"capacity": DIRECTORY_CAPACITY, "directories": self.directories})
    }

    pub fn clear(&mut self) -> Value {
        self.directories.clear();
        self.snapshot()
    }
}
