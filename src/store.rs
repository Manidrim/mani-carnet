//! Stockage sur disque : des fichiers Markdown dans un dossier.
//!
//! ~/Documents/Carnet/
//!   taches.md          <- toutes les tâches
//!   notes/*.md         <- une note par fichier

use crate::model::{parse_tasks, serialize_tasks, tags_in, Task};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone)]
pub struct Store {
    root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct NoteMeta {
    pub path: PathBuf,
    pub title: String,
    pub modified: SystemTime,
    /// Contenu en minuscules, pour la recherche plein texte sans relire le disque.
    pub search: String,
    pub tags: Vec<String>,
}

impl Store {
    pub fn open_default() -> io::Result<Store> {
        Store::open(crate::config::data_dir())
    }

    pub fn open(root: PathBuf) -> io::Result<Store> {
        fs::create_dir_all(root.join("notes"))?;
        Ok(Store { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    // ---------- Tâches ----------

    fn tasks_path(&self) -> PathBuf {
        self.root.join("taches.md")
    }

    pub fn load_tasks(&self) -> Vec<Task> {
        fs::read_to_string(self.tasks_path())
            .map(|c| parse_tasks(&c))
            .unwrap_or_default()
    }

    pub fn save_tasks(&self, tasks: &[Task]) -> io::Result<()> {
        write_atomic(&self.tasks_path(), &serialize_tasks(tasks))
    }

    // ---------- Notes ----------

    fn notes_dir(&self) -> PathBuf {
        self.root.join("notes")
    }

    /// Liste des notes, la plus récemment modifiée en premier.
    pub fn list_notes(&self) -> Vec<NoteMeta> {
        let mut out = Vec::new();
        if let Ok(rd) = fs::read_dir(self.notes_dir()) {
            for e in rd.flatten() {
                let path = e.path();
                if path.extension().and_then(|s| s.to_str()) != Some("md") {
                    continue;
                }
                let modified = e
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                let content = fs::read_to_string(&path).unwrap_or_default();
                out.push(NoteMeta {
                    title: title_of(&content, &path),
                    search: content.to_lowercase(),
                    tags: tags_in(&content).iter().map(|t| t.to_lowercase()).collect(),
                    path,
                    modified,
                });
            }
        }
        out.sort_by(|a, b| b.modified.cmp(&a.modified));
        out
    }

    pub fn read_note(&self, path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_default()
    }

    pub fn write_note(&self, path: &Path, content: &str) -> io::Result<()> {
        write_atomic(path, content)
    }

    /// Crée une note vide avec un nom de fichier unique (horodaté).
    pub fn new_note(&self) -> io::Result<PathBuf> {
        let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M%S");
        let mut path = self.notes_dir().join(format!("{stamp}.md"));
        let mut n = 2;
        while path.exists() {
            path = self.notes_dir().join(format!("{stamp}-{n}.md"));
            n += 1;
        }
        write_atomic(&path, "# \n\n")?;
        Ok(path)
    }

    pub fn delete_note(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }
}

/// Titre = première ligne non vide, sans les `#` de Markdown.
pub fn title_of(content: &str, path: &Path) -> String {
    content
        .lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("note");
            format!("(sans titre) {name}")
        })
}

/// Écrit via un fichier temporaire puis renomme : pas de fichier à moitié écrit.
fn write_atomic(path: &Path, content: &str) -> io::Result<()> {
    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, content)?;
    fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_and_tasks_roundtrip() {
        let dir = std::env::temp_dir().join(format!("carnet-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = Store::open(dir.clone()).unwrap();

        let p = s.new_note().unwrap();
        s.write_note(&p, "# Idée\n\ncorps").unwrap();
        let notes = s.list_notes();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "Idée");

        let tasks = vec![Task::from_input("test #a @2026-12-01").unwrap()];
        s.save_tasks(&tasks).unwrap();
        assert_eq!(s.load_tasks(), tasks);

        s.delete_note(&p).unwrap();
        assert!(s.list_notes().is_empty());
        let _ = fs::remove_dir_all(dir);
    }
}
