//! Réglages persistants et actions "système" faites par l'app elle-même :
//! emplacement des données, déplacement vers iCloud Drive, lancement à l'ouverture de session.

use crate::model::{parse_tasks, serialize_tasks};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// ~/Library/Application Support/Carnet/dossier.txt contient le chemin des données.
fn config_file() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("Carnet").join("dossier.txt"))
}

pub fn default_dir() -> PathBuf {
    dirs::document_dir()
        .map(|d| d.join("Carnet"))
        .unwrap_or_else(|| PathBuf::from("Carnet"))
}

/// Priorité : variable CARNET_DIR > réglage enregistré > ~/Documents/Carnet.
pub fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("CARNET_DIR") {
        return PathBuf::from(p);
    }
    config_file()
        .and_then(|f| fs::read_to_string(f).ok())
        .map(|s| PathBuf::from(s.trim()))
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(default_dir)
}

pub fn save_data_dir(p: &Path) -> io::Result<()> {
    let f = config_file().ok_or_else(|| io::Error::other("pas de dossier de config"))?;
    fs::create_dir_all(f.parent().unwrap())?;
    fs::write(f, p.to_string_lossy().as_bytes())
}

// ---------- iCloud Drive ----------

pub fn icloud_root() -> Option<PathBuf> {
    let p = dirs::home_dir()?.join("Library/Mobile Documents/com~apple~CloudDocs");
    p.is_dir().then_some(p)
}

pub fn is_in_icloud(dir: &Path) -> bool {
    icloud_root().is_some_and(|r| dir.starts_with(r))
}

/// Copie les données de `from` vers `to` sans jamais rien écraser ni supprimer :
/// - tâches : union des deux listes (doublons exacts ignorés)
/// - notes : copiées ; si le nom existe déjà avec un autre contenu → suffixe `-importee`
/// L'ancien dossier reste intact (filet de sécurité).
pub fn merge_into(from: &Path, to: &Path) -> io::Result<usize> {
    fs::create_dir_all(to.join("notes"))?;
    let mut copied = 0;

    let src_tasks = fs::read_to_string(from.join("taches.md")).unwrap_or_default();
    let dst_tasks = fs::read_to_string(to.join("taches.md")).unwrap_or_default();
    let mut all = parse_tasks(&dst_tasks);
    for t in parse_tasks(&src_tasks) {
        if !all.contains(&t) {
            all.push(t);
            copied += 1;
        }
    }
    if !all.is_empty() {
        fs::write(to.join("taches.md"), serialize_tasks(&all))?;
    }

    if let Ok(rd) = fs::read_dir(from.join("notes")) {
        for e in rd.flatten() {
            let src = e.path();
            if src.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let content = fs::read(&src)?;
            let mut dst = to.join("notes").join(e.file_name());
            if dst.exists() {
                if fs::read(&dst)? == content {
                    continue;
                }
                let stem = src.file_stem().unwrap().to_string_lossy();
                dst = to.join("notes").join(format!("{stem}-importee.md"));
            }
            fs::write(dst, content)?;
            copied += 1;
        }
    }
    Ok(copied)
}

// ---------- Lancement à l'ouverture de session (LaunchAgent macOS) ----------

const AGENT_LABEL: &str = "fr.campingcarpark.carnet";

fn agent_file() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join("Library/LaunchAgents").join(format!("{AGENT_LABEL}.plist")))
}

pub fn autostart_enabled() -> bool {
    agent_file().is_some_and(|f| f.exists())
}

pub fn set_autostart(on: bool) -> io::Result<()> {
    let f = agent_file().ok_or_else(|| io::Error::other("dossier personnel introuvable"))?;
    if !on {
        if f.exists() {
            fs::remove_file(f)?;
        }
        return Ok(());
    }
    // Si on tourne depuis Carnet.app, on lance le bundle (icône dans le Dock) ; sinon le binaire.
    let exe = std::env::current_exe()?;
    let args = match exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")) {
        Some(app) => format!(
            "<string>/usr/bin/open</string><string>-a</string><string>{}</string>",
            xml(&app.to_string_lossy())
        ),
        None => format!("<string>{}</string>", xml(&exe.to_string_lossy())),
    };
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{AGENT_LABEL}</string>
  <key>ProgramArguments</key><array>{args}</array>
  <key>RunAtLoad</key><true/>
</dict>
</plist>
"#
    );
    fs::create_dir_all(f.parent().unwrap())?;
    fs::write(f, plist)
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Ouvre un dossier dans le Finder.
pub fn reveal(p: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(p).spawn();
    #[cfg(not(target_os = "macos"))]
    let _ = p;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_never_loses_anything() {
        let base = std::env::temp_dir().join(format!("carnet-merge-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (a, b) = (base.join("a"), base.join("b"));
        fs::create_dir_all(a.join("notes")).unwrap();
        fs::create_dir_all(b.join("notes")).unwrap();
        fs::write(a.join("taches.md"), "- [ ] commune\n- [ ] seulement A\n").unwrap();
        fs::write(b.join("taches.md"), "- [ ] commune\n- [ ] seulement B\n").unwrap();
        fs::write(a.join("notes/x.md"), "# version A").unwrap();
        fs::write(b.join("notes/x.md"), "# version B").unwrap();
        fs::write(a.join("notes/y.md"), "# y").unwrap();

        merge_into(&a, &b).unwrap();
        let tasks = fs::read_to_string(b.join("taches.md")).unwrap();
        assert_eq!(tasks.matches("commune").count(), 1);
        assert!(tasks.contains("seulement A") && tasks.contains("seulement B"));
        assert_eq!(fs::read_to_string(b.join("notes/x.md")).unwrap(), "# version B");
        assert_eq!(fs::read_to_string(b.join("notes/x-importee.md")).unwrap(), "# version A");
        assert!(b.join("notes/y.md").exists());
        assert!(a.join("notes/x.md").exists(), "la source reste intacte");
        let _ = fs::remove_dir_all(base);
    }
}
