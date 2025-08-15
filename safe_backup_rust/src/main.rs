use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf, Component};

/// ---------- Path helpers (simple & robust on Windows/macOS/Linux) ----------

/// Resolve a user-supplied filename safely under the current working directory.
/// Rules:
/// - reject empty names
/// - reject absolute paths
/// - reject any parent traversal ("..") anywhere in the input
/// - otherwise, join under the CWD (no canonicalization needed → avoids Windows false-positives)
fn resolve_safe_path(input: &str) -> io::Result<PathBuf> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Empty filename"));
    }
    if trimmed.contains('\0') {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid character in filename"));
    }

    let p = Path::new(trimmed);

    // 1) No absolute paths (prevents /etc/passwd or C:\Windows\... etc.)
    if p.is_absolute() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Absolute paths are not allowed"));
    }

    // 2) No traversal components anywhere (prevents escaping the working directory)
    for comp in p.components() {
        if matches!(comp, Component::ParentDir) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Parent directory traversal is not allowed",
            ));
        }
    }

    // 3) Join syntactically under the current working directory
    let cwd = std::env::current_dir()?;
    Ok(cwd.join(p))
}

/// Create the backup path: "file.ext" -> "file.ext.bak", "file" -> "file.bak"
fn backup_path_for(file: &Path) -> PathBuf {
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

/// ---------- Operations (backup/restore/delete) ----------

fn backup_file(filename: &str) -> io::Result<()> {
    let path = resolve_safe_path(filename)?;
    if !path.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Source file does not exist"));
    }
    if !path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Source path is not a regular file",
        ));
    }

    let backup = backup_path_for(&path);
    fs::copy(&path, &backup)?;
    println!("Your backup created: {}", backup.display());
    log_action(&format!("backup | {} | success", filename))?;
    Ok(())
}

fn restore_file(filename: &str) -> io::Result<()> {
    let path = resolve_safe_path(filename)?;
    let backup = backup_path_for(&path);
    if !backup.exists() {
        println!("Backup file not found.");
        log_action(&format!("restore | {} | failure | no backup", filename))?;
        return Ok(());
    }
    fs::copy(&backup, &path)?;
    println!("File restored from: {}", backup.display());
    log_action(&format!("restore | {} | success", filename))?;
    Ok(())
}

fn delete_file(filename: &str) -> io::Result<()> {
    let path = resolve_safe_path(filename)?;
    if !path.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "File does not exist"));
    }
    if !path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Target is not a regular file",
        ));
    }

    print!("Are you sure you want to delete '{}'? (yes/no): ", filename);
    io::stdout().flush()?;
    let mut confirm = String::new();
    io::stdin().read_line(&mut confirm)?;
    if confirm.trim().eq_ignore_ascii_case("yes") {
        fs::remove_file(&path)?;
        println!("File deleted.");
        log_action(&format!("delete | {} | success", filename))?;
    } else {
        println!("Deletion cancelled.");
        log_action(&format!("delete | {} | cancelled", filename))?;
    }
    Ok(())
}

/// ---------- Logging ----------

fn log_action(line: &str) -> io::Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open("logfile.txt")?;
    writeln!(f, "{}", line)?;
    Ok(())
}

/// ---------- CLI ----------

fn main() {
    println!("safe_backup (Rust) — type 'exit' to quit");

    loop {
        // filename
        print!("Please enter your file name: ");
        io::stdout().flush().expect("flush stdout");
        let mut filename = String::new();
        if io::stdin().read_line(&mut filename).is_err() {
            eprintln!("Failed to read filename");
            continue;
        }
        let filename = filename.trim();
        if filename.eq_ignore_ascii_case("exit") {
            println!("Exiting.");
            break;
        }

        // command
        print!("Please enter your command (backup, restore, delete, exit): ");
        io::stdout().flush().expect("flush stdout");
        let mut command = String::new();
        if io::stdin().read_line(&mut command).is_err() {
            eprintln!("Failed to read command");
            continue;
        }
        let command = command.trim().to_lowercase();
        if command == "exit" {
            println!("Exiting.");
            break;
        }

        // execute
        let result = match command.as_str() {
            "backup" => backup_file(filename),
            "restore" => restore_file(filename),
            "delete" => delete_file(filename),
            _ => {
                println!("Unknown command. Allowed: backup | restore | delete | exit");
                Ok(())
            }
        };

        if let Err(e) = result {
            eprintln!("Operation failed: {}", e);
            let _ = log_action(&format!("{} | {} | failure | {}", command, filename, e));
        }
    }
}
