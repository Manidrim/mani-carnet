//! Modèle de données. Tout est du texte Markdown lisible à la main.
//!
//! Une tâche = une ligne :  `- [ ] Appeler Greg #octopus @2026-11-03T14:30`
//!   - `#tag`                  : étiquette
//!   - `@AAAA-MM-JJ`           : échéance (rappel le jour même)
//!   - `@AAAA-MM-JJTHH:MM`     : échéance avec heure (rappel à l'heure pile)
//!   - à la saisie seulement   : `@auj`, `@demain`, `@lundi`… `@dimanche`, `@14h30`

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Weekday};

#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub done: bool,
    /// Texte brut, sans la case à cocher ni l'échéance (les #tags restent dedans).
    pub text: String,
    pub due: Option<NaiveDate>,
    pub time: Option<NaiveTime>,
}

impl Task {
    /// Construit une tâche depuis ce que l'utilisateur tape (date du jour = aujourd'hui).
    pub fn from_input(input: &str) -> Option<Task> {
        Task::from_input_at(input, Local::now().date_naive())
    }

    /// Variante testable : `today` est injecté.
    pub fn from_input_at(input: &str, today: NaiveDate) -> Option<Task> {
        let mut due = None;
        let mut time = None;
        let mut words = Vec::new();
        for w in input.split_whitespace() {
            match w.strip_prefix('@').and_then(|s| parse_due(s, today)) {
                Some((d, t)) => {
                    if d.is_some() {
                        due = d;
                    }
                    if t.is_some() {
                        time = t;
                    }
                }
                None => words.push(w),
            }
        }
        // `@14h30` seul => aujourd'hui à 14h30
        if time.is_some() && due.is_none() {
            due = Some(today);
        }
        let text = words.join(" ");
        if text.is_empty() {
            return None;
        }
        Some(Task { done: false, text, due, time })
    }

    pub fn to_line(&self) -> String {
        let mark = if self.done { 'x' } else { ' ' };
        let mut line = format!("- [{mark}] {}", self.text);
        if let Some(d) = self.due {
            line.push_str(&format!(" @{}", d.format("%Y-%m-%d")));
            if let Some(t) = self.time {
                line.push_str(&format!("T{}", t.format("%H:%M")));
            }
        }
        line
    }

    pub fn from_line(line: &str) -> Option<Task> {
        let line = line.trim_start();
        let rest = line.strip_prefix("- [")?;
        let mut chars = rest.chars();
        let mark = chars.next()?;
        let rest = chars.as_str().strip_prefix(']')?;
        let mut t = Task::from_input(rest)?;
        t.done = matches!(mark, 'x' | 'X');
        Some(t)
    }

    pub fn tags(&self) -> Vec<&str> {
        tags_in(&self.text)
    }

    /// Moment où le rappel doit sonner : l'heure si précisée, sinon 9h00 le jour J.
    pub fn reminder_at(&self) -> Option<NaiveDateTime> {
        let d = self.due?;
        let t = self.time.unwrap_or_else(|| NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        Some(d.and_time(t))
    }

    /// Clé stable pour savoir si on a déjà notifié cette tâche.
    pub fn key(&self) -> String {
        self.to_line()
    }
}

/// Comprend : `2026-11-03`, `2026-11-03T14:30`, `14h30`, `14:30`, `auj`, `demain`, `lundi`…
fn parse_due(s: &str, today: NaiveDate) -> Option<(Option<NaiveDate>, Option<NaiveTime>)> {
    let lower = s.to_lowercase();
    if let Some((d, t)) = lower.split_once('t') {
        if let (Some(d), Some(t)) = (parse_date(d), parse_time(t)) {
            return Some((Some(d), Some(t)));
        }
    }
    if let Some(d) = parse_date(&lower) {
        return Some((Some(d), None));
    }
    if let Some(t) = parse_time(&lower) {
        return Some((None, Some(t)));
    }
    let d = match lower.as_str() {
        "auj" | "aujourdhui" | "aujourd'hui" | "today" => today,
        "demain" | "tomorrow" => today + Duration::days(1),
        "apresdemain" | "après-demain" | "apres-demain" | "aprèsdemain" => today + Duration::days(2),
        other => next_weekday(today, weekday_fr(other)?),
    };
    Some((Some(d), None))
}

pub fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

fn parse_time(s: &str) -> Option<NaiveTime> {
    let s = s.replace('h', ":");
    let s = if s.ends_with(':') { format!("{s}00") } else { s };
    NaiveTime::parse_from_str(&s, "%H:%M").ok()
}

fn weekday_fr(s: &str) -> Option<Weekday> {
    Some(match s {
        "lundi" => Weekday::Mon,
        "mardi" => Weekday::Tue,
        "mercredi" => Weekday::Wed,
        "jeudi" => Weekday::Thu,
        "vendredi" => Weekday::Fri,
        "samedi" => Weekday::Sat,
        "dimanche" => Weekday::Sun,
        _ => return None,
    })
}

/// Date correspondant à un mot de complétion (`demain`, `lundi`…), pour l'afficher.
pub fn date_of_word(word: &str, today: NaiveDate) -> Option<NaiveDate> {
    parse_due(word, today).and_then(|(d, _)| d)
}

/// Prochain jour donné, strictement après aujourd'hui (`@lundi` un lundi = lundi prochain).
fn next_weekday(today: NaiveDate, wd: Weekday) -> NaiveDate {
    let diff = (7 + wd.num_days_from_monday() as i64 - today.weekday().num_days_from_monday() as i64) % 7;
    today + Duration::days(if diff == 0 { 7 } else { diff })
}

/// Extrait les `#tags` d'un texte (sans ponctuation finale, en minuscules).
pub fn tags_in(text: &str) -> Vec<&str> {
    text.split_whitespace()
        .filter_map(|w| w.strip_prefix('#'))
        .map(|t| t.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_'))
        .filter(|t| !t.is_empty() && !t.starts_with('#'))
        .collect()
}

pub fn has_tag(text: &str, tag: &str) -> bool {
    tags_in(text).iter().any(|t| t.eq_ignore_ascii_case(tag))
}

pub fn parse_tasks(content: &str) -> Vec<Task> {
    content.lines().filter_map(Task::from_line).collect()
}

pub fn serialize_tasks(tasks: &[Task]) -> String {
    let mut out = String::from("# Tâches\n\n");
    for t in tasks {
        out.push_str(&t.to_line());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        parse_date(s).unwrap()
    }

    #[test]
    fn roundtrip() {
        let t = Task::from_input("Appeler Greg #octopus @2026-11-03").unwrap();
        assert_eq!(t.text, "Appeler Greg #octopus");
        assert_eq!(t.due, Some(d("2026-11-03")));
        assert_eq!(Task::from_line(&t.to_line()).unwrap(), t);
    }

    #[test]
    fn roundtrip_with_time() {
        let t = Task::from_input("Réunion @2026-11-03T14:30").unwrap();
        assert_eq!(t.to_line(), "- [ ] Réunion @2026-11-03T14:30");
        assert_eq!(Task::from_line(&t.to_line()).unwrap(), t);
    }

    #[test]
    fn friendly_dates() {
        let thu = d("2026-10-29"); // un jeudi
        let t = Task::from_input_at("x @demain @9h", thu).unwrap();
        assert_eq!(t.due, Some(d("2026-10-30")));
        assert_eq!(t.time, NaiveTime::from_hms_opt(9, 0, 0));
        assert_eq!(Task::from_input_at("x @lundi", thu).unwrap().due, Some(d("2026-11-02")));
        assert_eq!(Task::from_input_at("x @jeudi", thu).unwrap().due, Some(d("2026-11-05")));
        assert_eq!(Task::from_input_at("x @14h30", thu).unwrap().due, Some(thu));
        // un @ inconnu reste dans le texte
        assert_eq!(Task::from_input_at("mail @greg", thu).unwrap().text, "mail @greg");
    }

    #[test]
    fn done_flag() {
        let t = Task::from_line("- [x] Fini").unwrap();
        assert!(t.done);
        assert_eq!(t.to_line(), "- [x] Fini");
    }

    #[test]
    fn tags() {
        let t = Task::from_input("a #x b #y, ## titre").unwrap();
        assert_eq!(t.tags(), vec!["x", "y"]);
        assert!(has_tag(&t.text, "Y"));
    }

    #[test]
    fn empty_rejected() {
        assert!(Task::from_input("   ").is_none());
        assert!(Task::from_input("@2026-01-01").is_none());
    }
}
