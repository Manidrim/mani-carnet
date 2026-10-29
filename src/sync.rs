//! Synchronisation via git : le dossier de données est un dépôt git.
//! - `setup(url)` : configure tout tout seul (init ou fusion avec un dépôt existant)
//! - `sync()`     : commit local + pull --rebase + push
//! Ça marche avec GitLab, GitHub, ou un dépôt nu sur un NAS.

use crate::config::merge_into;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{channel, Receiver};
use std::thread;

pub type Job = Receiver<Result<String, String>>;

fn run_git(dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    if let Some(d) = dir {
        cmd.arg("-C").arg(d);
    }
    // L'app peut être lancée depuis le Finder : jamais de question interactive qui bloquerait.
    cmd.env("GIT_TERMINAL_PROMPT", "0").env(
        "GIT_SSH_COMMAND",
        "ssh -o BatchMode=yes -o StrictHostKeyChecking=accept-new",
    );
    let out = cmd
        .args(args)
        .output()
        .map_err(|e| format!("git introuvable ({e}). Installe-le avec : xcode-select --install"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if out.status.success() {
        Ok(text)
    } else {
        Err(format!("git {} : {}", args.join(" "), text.trim()))
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    run_git(Some(root), args)
}

pub fn is_repo(root: &Path) -> bool {
    root.join(".git").exists()
}

pub fn remote_url(root: &Path) -> Option<String> {
    if !is_repo(root) {
        return None;
    }
    git(root, &["remote", "get-url", "origin"]).ok().map(|s| s.trim().to_string())
}

/// Si git n'a pas d'identité configurée, on en met une locale au dépôt (sinon le commit échoue).
fn ensure_identity(root: &Path) -> Result<(), String> {
    if git(root, &["config", "user.email"]).is_err() {
        git(root, &["config", "user.email", "carnet@localhost"])?;
        git(root, &["config", "user.name", "Carnet"])?;
    }
    Ok(())
}

/// Configure la synchro vers `url`, sans rien perdre :
/// - dépôt distant vide  → on initialise ici et on pousse
/// - dépôt distant rempli (2e Mac) → on le clone, on y fusionne les données locales,
///   l'ancien dossier est gardé en sauvegarde à côté.
pub fn setup(root: &Path, url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("Colle d'abord l'adresse du dépôt.".into());
    }
    if is_repo(root) {
        if git(root, &["remote", "get-url", "origin"]).is_ok() {
            git(root, &["remote", "set-url", "origin", url])?;
        } else {
            git(root, &["remote", "add", "origin", url])?;
        }
        ensure_identity(root)?;
        return sync(root);
    }

    let remote_heads = run_git(None, &["ls-remote", "--heads", url]).map_err(|e| {
        format!("Impossible de joindre le dépôt. Vérifie l'adresse et ta clé SSH. ({e})")
    })?;

    if remote_heads.trim().is_empty() {
        git(root, &["init", "-q", "-b", "main"])?;
        git(root, &["remote", "add", "origin", url])?;
        ensure_identity(root)?;
        return sync(root).map(|m| format!("Synchro activée. {m}"));
    }

    // Le dépôt contient déjà des données (autre Mac).
    let parent = root.parent().ok_or("dossier de données invalide")?;
    let name = root.file_name().ok_or("dossier de données invalide")?.to_string_lossy();
    let tmp = parent.join(format!("{name}.synchro-en-cours"));
    let _ = fs::remove_dir_all(&tmp);
    run_git(None, &["clone", "-q", url, &tmp.to_string_lossy()])?;
    merge_into(root, &tmp).map_err(|e| e.to_string())?;
    ensure_identity(&tmp)?;

    let backup = unique(parent.join(format!("{name} (avant synchro)")));
    fs::rename(root, &backup).map_err(|e| e.to_string())?;
    fs::rename(&tmp, root).map_err(|e| e.to_string())?;
    let msg = sync(root)?;
    Ok(format!(
        "Synchro activée, données fusionnées. Ancienne copie gardée dans « {} ». {msg}",
        backup.file_name().unwrap().to_string_lossy()
    ))
}

fn unique(p: PathBuf) -> PathBuf {
    if !p.exists() {
        return p;
    }
    (2..)
        .map(|n| PathBuf::from(format!("{} {n}", p.display())))
        .find(|c| !c.exists())
        .unwrap()
}

/// Synchronisation bloquante. Renvoie un message lisible.
pub fn sync(root: &Path) -> Result<String, String> {
    if !is_repo(root) {
        return Err("Synchro non configurée (onglet Réglages).".into());
    }
    let ignore = root.join(".gitignore");
    if !ignore.exists() {
        let _ = fs::write(&ignore, "*.tmp\n.DS_Store\n");
    }
    git(root, &["add", "-A"])?;
    if !git(root, &["status", "--porcelain"])?.trim().is_empty() {
        let msg = format!("carnet: sync {}", chrono::Local::now().format("%Y-%m-%d %H:%M"));
        git(root, &["commit", "-q", "-m", &msg])?;
    }
    let remotes = git(root, &["remote"])?;
    let Some(remote) = remotes.lines().next().map(str::trim) else {
        return Ok("Sauvegardé localement (aucun dépôt distant).".into());
    };
    let branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string();
    let has_upstream = git(root, &["rev-parse", "--abbrev-ref", "@{u}"]).is_ok();
    let remote_has_branch = !git(root, &["ls-remote", "--heads", remote, &branch])?.trim().is_empty();

    if remote_has_branch {
        let pull: Vec<&str> = if has_upstream {
            vec!["pull", "-q", "--rebase", "--autostash"]
        } else {
            vec!["pull", "-q", "--rebase", "--autostash", remote, &branch]
        };
        git(root, &pull).map_err(|e| {
            let _ = git(root, &["rebase", "--abort"]);
            format!("Conflit, rien n'a été écrasé. Résous-le à la main dans le dossier. ({e})")
        })?;
    }
    git(root, &["push", "-q", "-u", remote, &branch])?;
    Ok(format!("Synchronisé à {}", chrono::Local::now().format("%H:%M")))
}

/// Lance un travail long (git) dans un thread pour ne pas figer l'interface.
pub fn spawn(job: impl FnOnce() -> Result<String, String> + Send + 'static) -> Job {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let _ = tx.send(job());
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_first_mac_then_second_mac() {
        let base = std::env::temp_dir().join(format!("carnet-setup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let remote = base.join("remote.git");
        assert!(Command::new("git")
            .args(["init", "-q", "--bare", "-b", "main"])
            .arg(&remote)
            .status()
            .unwrap()
            .success());
        let url = remote.to_string_lossy().to_string();

        // Mac 1 : a déjà des données, dépôt vide
        let a = base.join("mac1").join("Carnet");
        fs::create_dir_all(a.join("notes")).unwrap();
        fs::write(a.join("taches.md"), "- [ ] depuis Mac 1\n").unwrap();
        fs::write(a.join("notes/n1.md"), "# note 1").unwrap();
        setup(&a, &url).unwrap();

        // Mac 2 : a aussi ses propres données, dépôt rempli → fusion
        let b = base.join("mac2").join("Carnet");
        fs::create_dir_all(b.join("notes")).unwrap();
        fs::write(b.join("taches.md"), "- [ ] depuis Mac 2\n").unwrap();
        let msg = setup(&b, &url).unwrap();
        assert!(msg.contains("fusionnées"), "{msg}");
        let tb = fs::read_to_string(b.join("taches.md")).unwrap();
        assert!(tb.contains("Mac 1") && tb.contains("Mac 2"));
        assert!(b.join("notes/n1.md").exists());
        assert!(base.join("mac2/Carnet (avant synchro)/taches.md").exists());

        // Retour sur Mac 1 : il récupère la tâche du Mac 2
        sync(&a).unwrap();
        assert!(fs::read_to_string(a.join("taches.md")).unwrap().contains("Mac 2"));
        assert_eq!(remote_url(&a).as_deref(), Some(url.as_str()));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn not_a_repo() {
        assert!(sync(Path::new("/")).is_err());
    }
}
