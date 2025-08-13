use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use chrono::Local;

/// CLI entry point
fn main() {
    println!("safe_backup (Rust) — type 'exit' to quit");
    if let Err(e) = run_cli() {
        eprintln!("Fatal error: {}", e);
    }
}

/// Interactive loop with input validation and logging
fn run_cli() -> Result<(), String> {
    loop {
        let filename = prompt("Please enter your file name: ")?;
        if filename.eq_ignore_ascii_case("exit") { break; }

        let command = prompt("Please enter your command (backup, restore, delete, exit): ")?;

        if command.eq_ignore_ascii_case("exit") {
            break;
        }

        match command.to_lowercase().as_str() {
            "backup" => {
                match do_backup(&filename) {
                    Ok(backup_path) => {
                        println!("Your backup created: {}", backup_path.display());
                        let _ = log_action("backup", &filename, "success", None);
                    }
                    Err(e) => {
                        eprintln!("Backup failed: {}", e);
                        let _ = log_action("backup", &filename, "failure", Some(&e));
                    }
                }
            }
            "restore" => {
                match do_restore(&filename) {
                    Ok(_) => {
                        println!("File restored from backup.");
                        let _ = log_action("restore", &filename, "success", None);
                    }
                    Err(e) => {
                        eprintln!("Restore failed: {}", e);
                        let _ = log_action("restore", &filename, "failure", Some(&e));
                    }
                }
            }
            "delete" => {
                // Ask for confirmation
                let confirm = prompt("Type 'yes' to confirm deletion: ")?;
                if confirm.to_lowercase().trim() != "yes" {
                    println!("Deletion cancelled.");
                    continue;
                }
                match do_delete(&filename) {
                    Ok(_) => {
                        println!("Deleted: {}", filename);
                        let _ = log_action("delete", &filename, "success", None);
                    }
                    Err(e) => {
                        eprintln!("Delete failed: {}", e);
                        let _ = log_action("delete", &filename, "failure", Some(&e));
                    }
                }
            }
            _ => {
                eprintln!("Unknown command. Allowed: backup | restore | delete | exit");
            }
        }
    }
    Ok(())
}

/// Prompt helper
fn prompt(msg: &str) -> Result<String, String> {
    print!("{}", msg);
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut s = String::new();
    io::stdin().read_line(&mut s).map_err(|e| e.to_string())?;
    Ok(s.trim().to_string())
}

/// Basic path "sandboxing": only relative paths within cwd; reject traversal outside
pub fn sanitize_path(input: &str) -> Result<PathBuf, String> {
    if input.is_empty() {
        return Err("Empty filename".into());
    }
    if input.len() > 255 {
        return Err("Filename too long".into());
    }
    if input.contains('\0') {
        return Err("Invalid character in filename".into());
    }

    let p = Path::new(input);

    if p.is_absolute() {
        return Err("Absolute paths are not allowed".into());
    }

    // Join to cwd and canonicalize parent to avoid traversal
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let joined = cwd.join(p);

    let parent = joined.parent().ok_or("Invalid path")?;
    let parent_can = parent.canonicalize().map_err(|_| "Invalid parent directory".to_string())?;
    let final_path = parent_can.join(
        joined.file_name().ok_or("Invalid file name")?
    );

    if !final_path.starts_with(&cwd) {
        return Err("Path escapes working directory".into());
    }
    Ok(final_path)
}

/// Derive backup filename: sample.txt -> sample.txt.bak, file -> file.bak
pub fn backup_path_for(file: &Path) -> PathBuf {
    let mut pb = PathBuf::from(file);
    let new_ext = match file.extension() {
        Some(ext) => {
            let mut s = ext.to_string_lossy().to_string();
            s.push_str(".bak");
            s
        }
        None => "bak".to_string(),
    };
    pb.set_extension(new_ext);
    pb
}

/// Backup operation: copy file to .bak variant
pub fn do_backup(filename: &str) -> Result<PathBuf, String> {
    let path = sanitize_path(filename)?;
    if !path.exists() {
        return Err("Source file does not exist".into());
    }
    if !path.is_file() {
        return Err("Source path is not a regular file".into());
    }

    let backup = backup_path_for(&path);
    fs::copy(&path, &backup).map_err(|e| e.to_string())?;
    Ok(backup)
}

/// Restore operation: copy .bak over the original file (if backup exists)
pub fn do_restore(filename: &str) -> Result<(), String> {
    let path = sanitize_path(filename)?;
    let backup = backup_path_for(&path);
    if !backup.exists() {
        return Err("Backup file does not exist".into());
    }
    fs::copy(&backup, &path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete operation with safety checks
pub fn do_delete(filename: &str) -> Result<(), String> {
    let path = sanitize_path(filename)?;
    if !path.exists() {
        return Err("File does not exist".into());
    }
    if !path.is_file() {
        return Err("Target is not a regular file".into());
    }
    fs::remove_file(&path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Append a structured log line to logfile.txt
pub fn log_action(action: &str, filename: &str, status: &str, err: Option<&str>) -> Result<(), String> {
    let ts = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut line = format!("{ts} | {action} | {filename} | {status}");
    if let Some(e) = err {
        line.push_str(&format!(" | error={}", e.replace('\n', " ")));
    }
    line.push('\n');
    let mut path = std::env::current_dir().map_err(|e| e.to_string())?;
    path.push("logfile.txt");
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    use std::io::Write as _;
    f.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn backup_restore_delete_happy_path() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        // Create a sample file
        let mut f = fs::File::create("sample.txt").unwrap();
        writeln!(f, "hello world").unwrap();

        // Backup
        let b = do_backup("sample.txt").unwrap();
        assert!(b.exists());

        // Modify original then restore
        fs::write("sample.txt", "modified").unwrap();
        do_restore("sample.txt").unwrap();
        let content = fs::read_to_string("sample.txt").unwrap();
        assert!(content.contains("hello world"));

        // Delete
        do_delete("sample.txt").unwrap();
        assert!(!Path::new("sample.txt").exists());

        std::env::set_current_dir(cwd).unwrap();
    }

    #[test]
    fn reject_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let err = sanitize_path("../etc/passwd").err().unwrap();
        assert!(err.contains("Absolute paths") == false); // different error pathway
        // Either invalid parent or escape prevention
        std::env::set_current_dir(cwd).unwrap();
    }
}
