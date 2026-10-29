//! Complétion des `@dates` pendant la saisie d'une tâche.
//! Taper `@de` propose `demain`, `@ap` propose `après-demain`, `@me` propose `mercredi`…
//! ↑ ↓ pour choisir, Tab ou Entrée pour valider, Échap pour fermer, ou clic.

use crate::datepicker::short;
use crate::model::date_of_word;
use chrono::{Local, NaiveDate};
use eframe::egui::{
    self, Id, Key, Modifiers, Popup, PopupCloseBehavior, Response, RichText,
    text::{CCursor, CCursorRange},
};

/// Mots proposés, dans l'ordre d'affichage.
const WORDS: [&str; 10] = [
    "aujourd'hui",
    "demain",
    "après-demain",
    "lundi",
    "mardi",
    "mercredi",
    "jeudi",
    "vendredi",
    "samedi",
    "dimanche",
];

/// Minuscules sans accents ni apostrophes : « Après » et « apres » se valent.
fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| *c != '\'' && *c != '’')
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// Le mot `@…` en cours de frappe (dernier mot du texte), sans le `@`.
pub fn active_prefix(text: &str) -> Option<&str> {
    if text.ends_with(char::is_whitespace) {
        return None;
    }
    text.split_whitespace().next_back()?.strip_prefix('@')
}

/// Suggestions (mot, date) pour un préfixe. Vide si le mot est déjà complet.
pub fn suggestions(prefix: &str, today: NaiveDate) -> Vec<(&'static str, NaiveDate)> {
    let p = fold(prefix);
    let list: Vec<_> = WORDS
        .iter()
        .filter(|w| fold(w).starts_with(&p))
        .filter_map(|w| Some((*w, date_of_word(w, today)?)))
        .collect();
    if list.len() == 1 && fold(list[0].0) == p {
        return Vec::new(); // déjà tapé en entier : rien à proposer
    }
    list
}

/// Remplace le `@…` en cours par `@mot ` (avec une espace pour continuer à taper).
pub fn apply(text: &str, word: &str) -> String {
    let start = text.rfind(char::is_whitespace).map_or(0, |i| i + 1);
    format!("{}@{word} ", &text[..start])
}

#[derive(Clone, Default)]
struct State {
    selected: usize,
    /// La liste était affichée à l'image précédente (pour gérer Échap).
    was_open: bool,
    /// Texte sur lequel l'utilisateur a fait Échap : la liste reste fermée tant qu'il ne change pas.
    dismissed_for: Option<String>,
}

/// Champ de saisie d'une ligne avec complétion des `@dates`.
/// Entrée/Tab ne valident la tâche que si aucune suggestion n'est affichée.
/// Renvoie aussi `true` si Échap a servi à fermer la liste (et ne doit rien faire d'autre).
pub fn text_edit(
    ui: &mut egui::Ui,
    text: &mut String,
    id_salt: &str,
    hint: &str,
    width: f32,
) -> (Response, bool) {
    let today = Local::now().date_naive();
    let edit_id = ui.make_persistent_id(id_salt);
    let state_id = edit_id.with("completion");
    let mut st: State = ui.data(|d| d.get_temp(state_id)).unwrap_or_default();

    let sugs = active_prefix(text)
        .map(|p| suggestions(p, today))
        .unwrap_or_default();
    // egui retire le focus sur Échap avant qu'on puisse intercepter la touche :
    // si la liste était ouverte, Échap la ferme seulement, et on garde le focus.
    let dismissed = st.was_open && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
    if dismissed {
        st.dismissed_for = Some(text.clone());
        ui.memory_mut(|m| m.request_focus(edit_id));
    }
    let focused = ui.memory(|m| m.has_focus(edit_id));
    let open = focused && !sugs.is_empty() && st.dismissed_for.as_deref() != Some(text.as_str());
    if st.dismissed_for.as_deref() != Some(text.as_str()) {
        st.dismissed_for = None;
    }
    st.selected = st.selected.min(sugs.len().saturating_sub(1));

    // Les touches de navigation sont interceptées avant que le champ ne les traite.
    let mut accept: Option<&str> = None;
    if open {
        ui.input_mut(|i| {
            if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                st.selected = (st.selected + 1) % sugs.len();
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                st.selected = (st.selected + sugs.len() - 1) % sugs.len();
            }
            if i.consume_key(Modifiers::NONE, Key::Enter) || i.consume_key(Modifiers::NONE, Key::Tab) {
                accept = Some(sugs[st.selected].0);
            }
        });
    }

    let resp = ui.add(
        egui::TextEdit::singleline(text)
            .id(edit_id)
            .hint_text(hint)
            .lock_focus(open) // Tab ne fait pas sortir du champ quand la liste est ouverte
            .desired_width(width),
    );

    if open && accept.is_none() {
        Popup::from_response(&resp)
            .id(edit_id.with("popup"))
            .open(true)
            .close_behavior(PopupCloseBehavior::IgnoreClicks)
            .show(|ui| {
                ui.set_min_width(240.0);
                for (i, (word, date)) in sugs.iter().enumerate() {
                    let line = RichText::new(format!("@{word}"));
                    let r = ui
                        .horizontal(|ui| {
                            let r = ui.selectable_label(i == st.selected, line);
                            ui.label(RichText::new(short(*date)).weak());
                            r
                        })
                        .inner;
                    if r.clicked() {
                        accept = Some(word);
                    }
                    if r.hovered() {
                        st.selected = i;
                    }
                }
                ui.label(RichText::new("↑↓ choisir · Tab/Entrée valider · Échap fermer").small().weak());
            });
    }

    if let Some(word) = accept {
        *text = apply(text, word);
        move_cursor_to_end(ui.ctx(), edit_id, text);
        ui.memory_mut(|m| m.request_focus(edit_id));
        st.selected = 0;
    }
    st.was_open = open && accept.is_none();
    ui.data_mut(|d| d.insert_temp(state_id, st));
    (resp, dismissed)
}

fn move_cursor_to_end(ctx: &egui::Context, id: Id, text: &str) {
    if let Some(mut s) = egui::text_edit::TextEditState::load(ctx, id) {
        let end = CCursor::new(text.chars().count());
        s.cursor.set_char_range(Some(CCursorRange::one(end)));
        s.store(ctx, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn prefix_detection() {
        assert_eq!(active_prefix("Appeler @de"), Some("de"));
        assert_eq!(active_prefix("Appeler @"), Some(""));
        assert_eq!(active_prefix("Appeler @demain "), None);
        assert_eq!(active_prefix("Appeler #tag"), None);
    }

    #[test]
    fn suggestions_by_prefix() {
        let today = d("2026-10-29"); // jeudi
        let words = |p| suggestions(p, today).iter().map(|s| s.0).collect::<Vec<_>>();
        assert_eq!(words("de"), vec!["demain"]);
        assert_eq!(words("ap"), vec!["après-demain"]);
        assert_eq!(words("apres"), vec!["après-demain"]); // sans accent
        assert_eq!(words("m"), vec!["mardi", "mercredi"]);
        assert_eq!(words("aujourdhui"), Vec::<&str>::new()); // déjà complet
        assert_eq!(words("14h"), Vec::<&str>::new());
        assert_eq!(words("").len(), 10);
        assert_eq!(suggestions("ap", today)[0].1, d("2026-10-31"));
        assert_eq!(suggestions("lu", today)[0].1, d("2026-11-02"));
    }

    #[test]
    fn apply_replaces_last_word() {
        assert_eq!(apply("Appeler Greg @de", "demain"), "Appeler Greg @demain ");
        assert_eq!(apply("@ma", "mardi"), "@mardi ");
    }

    #[test]
    fn completed_words_are_understood() {
        let today = d("2026-10-29");
        for w in WORDS {
            let t = crate::model::Task::from_input_at(&format!("x @{w}"), today).unwrap();
            assert!(t.due.is_some(), "{w} non compris");
            assert_eq!(t.text, "x");
        }
    }
}
