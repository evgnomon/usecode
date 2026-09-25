// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::exit;

/// Whether `name` matches the glob `*_<direction>.sql`.
pub fn matches_direction(name: &str, direction: &str) -> bool {
    name.ends_with(&format!("_{direction}.sql"))
}

/// Sort migration names: ascending for 'up', descending for 'down'.
pub fn sort_for_direction(files: &mut [PathBuf], direction: &str) {
    files.sort();
    if direction == "down" {
        files.reverse();
    }
}

/// All `*_<direction>.sql` files in `directory`, in execution order.
/// Exits with status 1 when the directory is missing or not a directory.
pub fn get_migration_files(directory: &str, direction: &str) -> Vec<PathBuf> {
    let path = Path::new(directory);
    if !path.exists() {
        println!("Error: Directory '{directory}' does not exist");
        exit(1);
    }
    if !path.is_dir() {
        println!("Error: '{directory}' is not a directory");
        exit(1);
    }
    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Error: {e}");
            exit(1);
        }
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| matches_direction(&e.file_name().to_string_lossy(), direction))
        .map(|e| e.path())
        .collect();
    sort_for_direction(&mut files, direction);
    files
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn read_migration(path: &Path) -> io::Result<String> {
    fs::read_to_string(path)
}

/// Print migrations without executing (for preview or manual execution).
pub fn print_only(files: &[PathBuf]) {
    let rule = "=".repeat(60);
    println!("\n{rule}");
    println!("MIGRATION FILES (in execution order)");
    println!("{rule}\n");

    for (i, file) in files.iter().enumerate() {
        println!("{}. {}", i + 1, file_name(file));
        println!("{}", "-".repeat(40));
        match read_migration(file) {
            Ok(sql) => println!("{sql}"),
            Err(e) => {
                eprintln!("Error: {}: {e}", file.display());
                exit(1);
            }
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_glob() {
        assert!(matches_direction("001_init_up.sql", "up"));
        assert!(matches_direction("_up.sql", "up"));
        assert!(!matches_direction("001_init_down.sql", "up"));
        assert!(!matches_direction("001_init_up.sql.bak", "up"));
        assert!(!matches_direction("001up.sql", "up"));
    }

    #[test]
    fn sort_order() {
        let mut v: Vec<PathBuf> = ["d/002_b_up.sql", "d/001_a_up.sql", "d/010_c_up.sql"]
            .iter()
            .map(PathBuf::from)
            .collect();
        sort_for_direction(&mut v, "up");
        assert_eq!(file_name(&v[0]), "001_a_up.sql");
        assert_eq!(file_name(&v[2]), "010_c_up.sql");
        sort_for_direction(&mut v, "down");
        assert_eq!(file_name(&v[0]), "010_c_up.sql");
        assert_eq!(file_name(&v[2]), "001_a_up.sql");
    }

    #[test]
    fn lists_directory() {
        let dir = std::env::temp_dir().join(format!("sqlize-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for n in [
            "002_x_up.sql",
            "001_x_up.sql",
            "001_x_down.sql",
            "notes.txt",
        ] {
            fs::write(dir.join(n), "").unwrap();
        }
        let up = get_migration_files(dir.to_str().unwrap(), "up");
        let names: Vec<String> = up.iter().map(|p| file_name(p)).collect();
        assert_eq!(names, ["001_x_up.sql", "002_x_up.sql"]);
        let down = get_migration_files(dir.to_str().unwrap(), "down");
        assert_eq!(down.len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
