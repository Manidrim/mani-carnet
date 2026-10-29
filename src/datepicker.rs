//! Petit calendrier en français, affiché dans une bulle sous un bouton.
//! Raccourcis (aujourd'hui, demain, lundi, +1 semaine), navigation par mois, « retirer la date ».

use chrono::{Datelike, Duration, Local, NaiveDate, Weekday};
use eframe::egui::{self, Popup, PopupCloseBehavior, Response, RichText};

/// 7 colonnes de 34 px + espacements.
const WIDTH: f32 = 7.0 * 34.0 + 6.0 * 2.0;

pub enum Pick {
    Nothing,
    Set(NaiveDate),
    Clear,
}

const MONTHS: [&str; 12] = [
    "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre",
    "novembre", "décembre",
];
const MONTHS_SHORT: [&str; 12] = [
    "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc.",
];
const DAYS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
const DAYS_SHORT: [&str; 7] = ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."];

/// Date lisible : « aujourd'hui », « demain », « jeudi », « lun. 3 nov. », « 3 nov. 2027 ».
pub fn human(d: NaiveDate, today: NaiveDate) -> String {
    let wd = d.weekday().num_days_from_monday() as usize;
    let m = d.month0() as usize;
    match (d - today).num_days() {
        0 => "aujourd'hui".into(),
        1 => "demain".into(),
        -1 => "hier".into(),
        2..=6 => DAYS[wd].into(),
        _ if d.year() == today.year() => format!("{} {} {}", DAYS_SHORT[wd], d.day(), MONTHS_SHORT[m]),
        _ => format!("{} {} {}", d.day(), MONTHS_SHORT[m], d.year()),
    }
}

fn first_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap()
}

fn add_months(d: NaiveDate, n: i32) -> NaiveDate {
    let total = d.year() * 12 + d.month0() as i32 + n;
    NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1).unwrap()
}

fn next_monday(today: NaiveDate) -> NaiveDate {
    let diff = 7 - today.weekday().num_days_from_monday() as i64;
    today + Duration::days(diff)
}

/// Ouvre un calendrier quand `button` est cliqué. Renvoie le choix de l'utilisateur.
pub fn picker(ui: &egui::Ui, button: &Response, current: Option<NaiveDate>) -> Pick {
    let today = Local::now().date_naive();
    let popup_id = button.id.with("calendrier");
    let month_id = popup_id.with("mois");
    if button.clicked() {
        // À chaque ouverture : on affiche le mois de la date actuelle (ou d'aujourd'hui).
        let start = first_of_month(current.unwrap_or(today));
        ui.ctx().data_mut(|d| d.insert_temp(month_id, start));
    }

    let mut pick = Pick::Nothing;
    Popup::from_toggle_button_response(button)
        .id(popup_id)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .width(WIDTH)
        .show(|ui| {
            ui.set_max_width(WIDTH);
            let mut month = ui
                .ctx()
                .data(|d| d.get_temp::<NaiveDate>(month_id))
                .unwrap_or_else(|| first_of_month(today));

            ui.horizontal(|ui| {
                for (label, d) in [
                    ("Aujourd'hui", today),
                    ("Demain", today + Duration::days(1)),
                    ("Lundi", next_monday(today)),
                    ("+1 semaine", today + Duration::days(7)),
                ] {
                    if ui.button(label).on_hover_text(human(d, today)).clicked() {
                        pick = Pick::Set(d);
                    }
                }
            });
            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("◀").on_hover_text("Mois précédent").clicked() {
                    month = add_months(month, -1);
                }
                let title = format!("{} {}", MONTHS[month.month0() as usize], month.year());
                ui.add_sized([WIDTH - 70.0, 20.0], egui::Label::new(RichText::new(title).strong()));
                if ui.button("▶").on_hover_text("Mois suivant").clicked() {
                    month = add_months(month, 1);
                }
            });

            const CELL: [f32; 2] = [34.0, 26.0];
            egui::Grid::new(popup_id.with("grille"))
                .spacing([2.0, 2.0])
                .show(ui, |ui| {
                    for name in ["L", "M", "M", "J", "V", "S", "D"] {
                        ui.add_sized(CELL, egui::Label::new(RichText::new(name).weak()));
                    }
                    ui.end_row();

                    let offset = month.weekday().num_days_from_monday();
                    for _ in 0..offset {
                        ui.add_sized(CELL, egui::Label::new(""));
                    }
                    let mut col = offset;
                    let mut d = month;
                    while d.month() == month.month() {
                        let mut txt = RichText::new(d.day().to_string());
                        if d == today {
                            txt = txt.strong().underline();
                        } else if d < today {
                            txt = txt.weak();
                        } else if matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
                            txt = txt.italics();
                        }
                        let selected = current == Some(d);
                        let r = ui
                            .add_sized(CELL, egui::Button::selectable(selected, txt))
                            .on_hover_text(human(d, today));
                        if r.clicked() {
                            pick = Pick::Set(d);
                        }
                        col += 1;
                        if col == 7 {
                            ui.end_row();
                            col = 0;
                        }
                        d = d.succ_opt().unwrap();
                    }
                });

            if current.is_some() {
                ui.separator();
                if ui.button("✕ Retirer la date").clicked() {
                    pick = Pick::Clear;
                }
            }

            ui.ctx().data_mut(|d| d.insert_temp(month_id, month));
        });

    if !matches!(pick, Pick::Nothing) {
        Popup::close_id(ui.ctx(), popup_id);
    }
    pick
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn human_dates() {
        let today = d("2026-10-29"); // jeudi
        assert_eq!(human(today, today), "aujourd'hui");
        assert_eq!(human(d("2026-10-30"), today), "demain");
        assert_eq!(human(d("2026-11-02"), today), "lundi");
        assert_eq!(human(d("2026-11-12"), today), "jeu. 12 nov.");
        assert_eq!(human(d("2027-01-05"), today), "5 janv. 2027");
    }

    #[test]
    fn months_wrap() {
        assert_eq!(add_months(d("2026-12-01"), 1), d("2027-01-01"));
        assert_eq!(add_months(d("2026-01-01"), -1), d("2025-12-01"));
        assert_eq!(next_monday(d("2026-11-02")), d("2026-11-09")); // un lundi → lundi suivant
    }
}
