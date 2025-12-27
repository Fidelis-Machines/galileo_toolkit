use chrono::DateTime;
use chrono::Utc;
use std::fs;
use std::path::PathBuf;

pub fn generate_file_name(prefix: &str, suffix: &str) -> String {
    let current_utc: DateTime<Utc> = Utc::now();
    let rfc3339_name: String = current_utc.to_rfc3339();
    
    let tmp_name = format!("{}.{}.{}", prefix, rfc3339_name.replace(":", "-"), suffix);

    tmp_name
}

struct FileRemover {
    path: PathBuf,
}

impl Drop for FileRemover {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
