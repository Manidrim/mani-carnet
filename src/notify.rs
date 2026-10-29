//! Notifications système. Sur macOS : `osascript`, aucune dépendance à compiler.

pub fn send(title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    {
        let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            "display notification \"{}\" with title \"{}\" sound name \"Glass\"",
            esc(body),
            esc(title)
        );
        let _ = std::process::Command::new("osascript").arg("-e").arg(script).spawn();
    }
    #[cfg(not(target_os = "macos"))]
    eprintln!("[rappel] {title} — {body}");
}

use crate::store::Store;
use chrono::Local;
use std::collections::HashSet;
use std::thread;
use std::time::Duration;

/// Thread de rappels : relit `taches.md` toutes les 30 s, indépendamment de la fenêtre.
/// Au lancement : un seul résumé pour tout ce qui est déjà en retard (pas de rafale).
/// Ensuite : une notification par tâche quand son heure arrive.
pub fn spawn_reminders() {
    thread::spawn(move || {
        let mut notified: HashSet<String> = HashSet::new();
        let mut first = true;
        loop {
            let now = Local::now().naive_local();
            // Relu à chaque tour : suit le dossier si on l'a déplacé (iCloud).
            let Ok(store) = Store::open_default() else {
                thread::sleep(Duration::from_secs(30));
                continue;
            };
            let due: Vec<_> = store
                .load_tasks()
                .into_iter()
                .filter(|t| !t.done && t.reminder_at().is_some_and(|r| r <= now))
                .filter(|t| !notified.contains(&t.key()))
                .collect();
            if first && due.len() > 2 {
                let list: Vec<&str> = due.iter().take(3).map(|t| t.text.as_str()).collect();
                send(
                    &format!("Carnet — {} tâches à faire", due.len()),
                    &format!("{}…", list.join(" · ")),
                );
            } else {
                for t in &due {
                    let today = Local::now().date_naive();
                    let when = match (t.due, t.time) {
                        (Some(d), _) if d < today => format!("en retard, prévu le {}", d.format("%d/%m")),
                        (_, Some(h)) => format!("à {}", h.format("%H:%M")),
                        _ => "aujourd'hui".into(),
                    };
                    send("Carnet — rappel", &format!("{} ({when})", t.text));
                }
            }
            notified.extend(due.iter().map(|t| t.key()));
            first = false;
            thread::sleep(Duration::from_secs(30));
        }
    });
}
