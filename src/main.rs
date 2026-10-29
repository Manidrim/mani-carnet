mod completion;
mod config;
mod datepicker;
mod model;
mod notify;
mod store;
mod sync;

use chrono::Local;
use eframe::egui::{self, Color32, Key, KeyboardShortcut, Modifiers, RichText};
use model::{has_tag, tags_in, Task};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use store::{NoteMeta, Store};

const AUTO_SYNC: Duration = Duration::from_secs(5 * 60);
const SAVE_DELAY: Duration = Duration::from_millis(600);
const RED: Color32 = Color32::from_rgb(220, 70, 70);
const TAG: Color32 = Color32::from_rgb(90, 150, 230);

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Tasks,
    Notes,
    Settings,
}

struct App {
    store: Store,
    tab: Tab,
    status: String,
    /// Tag sélectionné (partagé entre tâches et notes).
    tag: Option<String>,
    sync_rx: Option<sync::Job>,
    has_remote: bool,
    last_sync: Instant,
    sync_url: String,
    autostart: bool,

    // Tâches
    tasks: Vec<Task>,
    new_task: String,
    /// Date limite choisie au calendrier pour la prochaine tâche.
    new_due: Option<chrono::NaiveDate>,
    task_filter: String,
    hide_done: bool,
    only_today: bool,
    focus_new_task: bool,
    editing: Option<(usize, String)>,
    focus_edit: bool,

    // Notes
    notes: Vec<NoteMeta>,
    note_filter: String,
    current: Option<PathBuf>,
    editor: String,
    dirty_since: Option<Instant>,
    focus_editor: bool,
}

impl App {
    fn new(store: Store) -> Self {
        let mut app = App {
            tasks: store.load_tasks(),
            notes: store.list_notes(),
            store,
            tab: Tab::Tasks,
            status: String::new(),
            tag: None,
            sync_rx: None,
            has_remote: false,
            last_sync: Instant::now(),
            sync_url: String::new(),
            autostart: config::autostart_enabled(),
            new_task: String::new(),
            new_due: None,
            task_filter: String::new(),
            hide_done: false,
            only_today: false,
            focus_new_task: true,
            editing: None,
            focus_edit: false,
            note_filter: String::new(),
            current: None,
            editor: String::new(),
            dirty_since: None,
            focus_editor: false,
        };
        app.has_remote = sync::remote_url(app.store.root()).is_some();
        if app.has_remote {
            app.start_sync();
        }
        app
    }

    fn report<T>(&mut self, r: std::io::Result<T>) {
        if let Err(e) = r {
            self.status = format!("Erreur d'écriture : {e}");
        }
    }

    fn save_tasks(&mut self) {
        let r = self.store.save_tasks(&self.tasks);
        self.report(r);
    }

    fn flush_note(&mut self) {
        if let (Some(p), Some(_)) = (self.current.clone(), self.dirty_since) {
            let r = self.store.write_note(&p, &self.editor);
            self.report(r);
            self.dirty_since = None;
            self.notes = self.store.list_notes();
        }
    }

    fn open_note(&mut self, path: PathBuf) {
        self.flush_note();
        self.editor = self.store.read_note(&path);
        self.current = Some(path);
    }

    fn create_note(&mut self) {
        self.flush_note();
        match self.store.new_note() {
            Ok(p) => {
                // Si un tag est sélectionné, la nouvelle note le reçoit d'office.
                if let Some(t) = &self.tag {
                    let _ = self.store.write_note(&p, &format!("# \n\n#{t}\n"));
                }
                self.notes = self.store.list_notes();
                self.editor = self.store.read_note(&p);
                self.current = Some(p);
                self.tab = Tab::Notes;
                self.focus_editor = true;
            }
            Err(e) => self.status = format!("Impossible de créer la note : {e}"),
        }
    }

    fn delete_current_note(&mut self) {
        if let Some(p) = self.current.take() {
            let r = self.store.delete_note(&p);
            self.report(r);
            self.dirty_since = None;
            self.editor.clear();
            self.notes = self.store.list_notes();
        }
    }

    fn add_task(&mut self) {
        let mut input = self.new_task.clone();
        if let Some(t) = &self.tag {
            if !has_tag(&input, t) {
                input.push_str(&format!(" #{t}"));
            }
        }
        if let Some(mut t) = Task::from_input(&input) {
            // Une date tapée (@demain…) a priorité sur celle du calendrier.
            if t.due.is_none() {
                t.due = self.new_due;
            }
            self.new_due = None;
            self.tasks.insert(0, t);
            self.new_task.clear();
            self.save_tasks();
        }
    }

    fn start_sync(&mut self) {
        if self.sync_rx.is_some() {
            return;
        }
        self.flush_note();
        self.save_tasks();
        self.status = "Synchronisation…".into();
        self.last_sync = Instant::now();
        let root = self.store.root().to_path_buf();
        self.sync_rx = Some(sync::spawn(move || sync::sync(&root)));
    }

    fn start_setup(&mut self) {
        if self.sync_rx.is_some() {
            return;
        }
        self.flush_note();
        self.save_tasks();
        self.status = "Configuration de la synchro…".into();
        let root = self.store.root().to_path_buf();
        let url = self.sync_url.clone();
        self.sync_rx = Some(sync::spawn(move || sync::setup(&root, &url)));
    }

    /// Déplace les données vers iCloud Drive (copie + fusion, l'ancien dossier reste intact).
    fn move_to_icloud(&mut self) {
        let Some(icloud) = config::icloud_root() else { return };
        self.flush_note();
        self.save_tasks();
        let to = icloud.join("Carnet");
        let from = self.store.root().to_path_buf();
        let res = config::merge_into(&from, &to)
            .and_then(|_| config::save_data_dir(&to))
            .and_then(|_| Store::open(to.clone()));
        match res {
            Ok(store) => {
                self.store = store;
                self.reload();
                self.current = None;
                self.editor.clear();
                self.has_remote = sync::remote_url(self.store.root()).is_some();
                self.status = format!(
                    "Données dans iCloud Drive. L'ancien dossier {} est conservé, tu peux le supprimer.",
                    from.display()
                );
            }
            Err(e) => self.status = format!("Déplacement impossible : {e}"),
        }
    }

    fn reload(&mut self) {
        self.tasks = self.store.load_tasks();
        self.notes = self.store.list_notes();
        if let Some(p) = &self.current {
            if p.exists() {
                self.editor = self.store.read_note(p);
            } else {
                self.current = None;
                self.editor.clear();
            }
        }
    }

    /// Récupère le résultat de la synchro et recharge depuis le disque.
    fn poll_sync(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.sync_rx else { return };
        match rx.try_recv() {
            Ok(res) => {
                self.sync_rx = None;
                self.status = match res {
                    Ok(m) => m,
                    Err(e) => e,
                };
                self.has_remote = sync::remote_url(self.store.root()).is_some();
                self.reload();
            }
            Err(_) => ctx.request_repaint_after(Duration::from_millis(200)),
        }
    }

    // ---------- Puces de tags ----------

    fn tag_bar(&mut self, ui: &mut egui::Ui, all: BTreeSet<String>) {
        if all.is_empty() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Tags :").weak());
            if ui.selectable_label(self.tag.is_none(), "tous").clicked() {
                self.tag = None;
            }
            for t in all {
                let sel = self.tag.as_deref() == Some(t.as_str());
                if ui
                    .selectable_label(sel, RichText::new(format!("#{t}")).color(TAG))
                    .clicked()
                {
                    self.tag = if sel { None } else { Some(t) };
                }
            }
        });
    }

    // ---------- Vue Tâches ----------

    fn tasks_ui(&mut self, ui: &mut egui::Ui) {
        let hint = match &self.tag {
            Some(t) => format!("Nouvelle tâche dans #{t}…  (@demain  @lundi  @14h30)"),
            None => "Nouvelle tâche…  (#tag, @ pour une date : @demain, @lundi, @14h30)".into(),
        };
        let today = Local::now().date_naive();
        let resp = ui
            .horizontal(|ui| {
                let width = ui.available_width() - 190.0;
                let (resp, _) = completion::text_edit(ui, &mut self.new_task, "nouvelle_tache", &hint, width);
                let label = match self.new_due {
                    Some(d) => RichText::new(format!("📅 {}", datepicker::human(d, today))).color(TAG),
                    None => RichText::new("📅 Date limite"),
                };
                let b = ui.add_sized([180.0, 20.0], egui::Button::new(label))
                    .on_hover_text("Choisir une date limite pour la prochaine tâche");
                match datepicker::picker(ui, &b, self.new_due) {
                    datepicker::Pick::Set(d) => {
                        self.new_due = Some(d);
                        self.focus_new_task = true;
                    }
                    datepicker::Pick::Clear => self.new_due = None,
                    datepicker::Pick::Nothing => {}
                }
                resp
            })
            .inner;
        if self.focus_new_task {
            resp.request_focus();
            self.focus_new_task = false;
        }
        if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            self.add_task();
            self.focus_new_task = true;
        }

        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.task_filter)
                    .hint_text("Filtrer…")
                    .desired_width(200.0),
            );
            ui.checkbox(&mut self.only_today, "Aujourd'hui + en retard");
            ui.checkbox(&mut self.hide_done, "Masquer les terminées");
            let open = self.tasks.iter().filter(|t| !t.done).count();
            ui.label(RichText::new(format!("{open} à faire / {}", self.tasks.len())).weak());
        });
        let all: BTreeSet<String> = self
            .tasks
            .iter()
            .flat_map(|t| t.tags().into_iter().map(str::to_lowercase))
            .collect();
        self.tag_bar(ui, all);
        ui.separator();

        let filter = self.task_filter.to_lowercase();
        let mut changed = false;
        let mut to_delete = None;
        let mut clicked_tag = None;
        let mut start_edit = None;
        let mut commit_edit = false;
        let mut cancel_edit = false;

        // Ordre : à faire avant terminées, puis par échéance (sans date à la fin), ordre stable.
        let mut order: Vec<usize> = (0..self.tasks.len()).collect();
        order.sort_by_key(|&i| {
            let t = &self.tasks[i];
            (t.done, t.reminder_at().is_none(), t.reminder_at())
        });

        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            for i in order {
                let t = &mut self.tasks[i];
                if self.hide_done && t.done {
                    continue;
                }
                if self.only_today && (t.done || !t.due.is_some_and(|d| d <= today)) {
                    continue;
                }
                if !filter.is_empty() && !t.text.to_lowercase().contains(&filter) {
                    continue;
                }
                if let Some(tag) = &self.tag {
                    if !has_tag(&t.text, tag) {
                        continue;
                    }
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if ui.checkbox(&mut t.done, "").changed() {
                        changed = true;
                    }

                    // Mode édition (double-clic sur le texte)
                    if let Some((ei, buf)) = &mut self.editing {
                        if *ei == i {
                            let width = ui.available_width();
                            let (r, dismissed) = completion::text_edit(ui, buf, "edition_tache", "", width);
                            // Focus donné une seule fois à l'ouverture (le redemander à chaque image
                            // empêchait de détecter la fin de l'édition).
                            if self.focus_edit {
                                r.request_focus();
                                self.focus_edit = false;
                            }
                            // Entrée et Échap sont déjà absorbés par la complétion quand sa liste est ouverte.
                            if dismissed {
                                // Échap a seulement fermé la liste : on reste en édition.
                            } else if ui.input(|i| i.key_pressed(Key::Escape)) {
                                cancel_edit = true;
                            } else if r.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter)) {
                                commit_edit = true;
                            }
                            return;
                        }
                    }

                    // Texte : mots normaux + #tags cliquables
                    for w in t.text.split_whitespace() {
                        let is_tag = w.starts_with('#') && w.len() > 1;
                        let mut rt = RichText::new(w);
                        if is_tag {
                            rt = rt.color(TAG);
                        }
                        if t.done {
                            rt = rt.strikethrough().weak();
                        }
                        let r = ui.add(egui::Label::new(rt).sense(egui::Sense::click()));
                        if is_tag && r.clicked() {
                            clicked_tag = tags_in(w).first().map(|s| s.to_lowercase());
                        }
                        if r.double_clicked() {
                            start_edit = Some(i);
                        }
                    }

                    // Date limite : clic dessus = calendrier pour la changer.
                    let mut pick = datepicker::Pick::Nothing;
                    if let Some(d) = t.due {
                        let mut s = datepicker::human(d, today);
                        if let Some(h) = t.time {
                            s.push_str(&h.format(" %H:%M").to_string());
                        }
                        let overdue = !t.done && d < today;
                        let txt = RichText::new(format!("⏰ {s}"));
                        let txt = if overdue { txt.color(RED) } else { txt.weak() };
                        let b = ui
                            .add(egui::Button::new(txt).frame(false))
                            .on_hover_text("Changer la date limite");
                        pick = datepicker::picker(ui, &b, t.due);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").on_hover_text("Supprimer").clicked() {
                            to_delete = Some(i);
                        }
                        if ui.small_button("✏").on_hover_text("Modifier (ou double-clic)").clicked() {
                            start_edit = Some(i);
                        }
                        if t.due.is_none() {
                            let b = ui.small_button("📅").on_hover_text("Ajouter une date limite");
                            pick = datepicker::picker(ui, &b, None);
                        }
                    });
                    match pick {
                        datepicker::Pick::Set(d) => {
                            t.due = Some(d);
                            changed = true;
                        }
                        datepicker::Pick::Clear => {
                            t.due = None;
                            t.time = None;
                            changed = true;
                        }
                        datepicker::Pick::Nothing => {}
                    }
                });
            }
        });

        if let Some(i) = start_edit {
            self.editing = Some((i, self.tasks[i].to_line()[6..].to_string()));
            self.focus_edit = true;
        }
        if cancel_edit {
            self.editing = None;
        } else if commit_edit {
            if let Some((i, buf)) = self.editing.take() {
                if let Some(mut nt) = Task::from_input(&buf) {
                    nt.done = self.tasks[i].done;
                    self.tasks[i] = nt;
                    changed = true;
                }
            }
        }
        if let Some(t) = clicked_tag {
            self.tag = Some(t);
        }
        if let Some(i) = to_delete {
            self.tasks.remove(i);
            self.editing = None;
            changed = true;
        }
        if changed {
            self.save_tasks();
        }
    }

    // ---------- Vue Notes ----------

    fn notes_ui(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("notes_list").default_size(260.0).show(root, |ui| {
            if ui.button("➕ Nouvelle note  (⌘N)").clicked() {
                self.create_note();
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.note_filter)
                    .hint_text("Rechercher…")
                    .desired_width(f32::INFINITY),
            );
            let all: BTreeSet<String> = self.notes.iter().flat_map(|n| n.tags.clone()).collect();
            self.tag_bar(ui, all);
            ui.separator();

            let filter = self.note_filter.to_lowercase();
            let mut open = None;
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                for n in &self.notes {
                    if !filter.is_empty() && !n.search.contains(&filter) {
                        continue;
                    }
                    if let Some(t) = &self.tag {
                        if !n.tags.contains(t) {
                            continue;
                        }
                    }
                    let selected = self.current.as_ref() == Some(&n.path);
                    if ui.selectable_label(selected, &n.title).clicked() {
                        open = Some(n.path.clone());
                    }
                }
            });
            if let Some(p) = open {
                self.open_note(p);
                self.focus_editor = true;
            }
        });

        egui::CentralPanel::default_margins().show(root, |ui| {
            if self.current.is_none() {
                ui.centered_and_justified(|ui| {
                    ui.label("Choisis une note ou crée-en une (⌘N).");
                });
                return;
            }
            ui.horizontal(|ui| {
                let tags: Vec<String> = tags_in(&self.editor).iter().map(|t| format!("#{t}")).collect();
                ui.label(RichText::new(tags.join("  ")).color(TAG));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("🗑 Supprimer").clicked() {
                        self.delete_current_note();
                    }
                });
            });
            if self.current.is_none() {
                return;
            }
            let resp = egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.add_sized(
                        ui.available_size(),
                        egui::TextEdit::multiline(&mut self.editor)
                            .code_editor()
                            .desired_width(f32::INFINITY)
                            .hint_text("# Titre\n\nTa note en Markdown… #tag"),
                    )
                })
                .inner;
            if self.focus_editor {
                resp.request_focus();
                self.focus_editor = false;
            }
            if resp.changed() {
                self.dirty_since = Some(Instant::now());
            }
        });
    }

    // ---------- Vue Réglages ----------

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        let root = self.store.root().to_path_buf();
        ui.heading("Emplacement des données");
        ui.horizontal(|ui| {
            ui.label(RichText::new(root.display().to_string()).monospace());
            if ui.button("Ouvrir dans le Finder").clicked() {
                config::reveal(&root);
            }
        });
        ui.add_space(14.0);

        ui.heading("Synchro iCloud Drive (zéro configuration)");
        match config::icloud_root() {
            None => {
                ui.label("iCloud Drive n'est pas activé sur ce Mac (Réglages Système → identifiant Apple → iCloud).");
            }
            Some(_) if config::is_in_icloud(&root) => {
                ui.label(RichText::new("✔ Tes données sont dans iCloud Drive : elles se synchronisent toutes seules entre tes Mac.").color(Color32::from_rgb(90, 180, 110)));
            }
            Some(_) => {
                ui.label("Un clic : tes notes et tâches passent dans iCloud Drive et se synchronisent entre tes Mac. Rien n'est supprimé.");
                if ui.button("☁ Déplacer mes données dans iCloud Drive").clicked() {
                    self.move_to_icloud();
                }
            }
        }
        ui.add_space(14.0);

        ui.heading("Synchro git (GitLab, GitHub…) avec historique");
        if self.has_remote {
            let url = sync::remote_url(&root).unwrap_or_default();
            ui.label(RichText::new(format!("✔ Synchronisé avec {url}")).color(Color32::from_rgb(90, 180, 110)));
            ui.label(RichText::new("Automatique au lancement, toutes les 5 minutes et à la fermeture. ⌘S pour forcer.").weak());
        } else {
            ui.label("1. Crée un projet vide et privé sur GitLab.  2. Colle son adresse SSH ici.  3. Clique sur Activer.");
            ui.label(RichText::new("Si le dépôt contient déjà des données (autre Mac), elles sont fusionnées avec les tiennes et rien n'est perdu.").weak());
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.sync_url)
                        .hint_text("git@gitlab.com:emeyer/carnet.git")
                        .desired_width(380.0),
                );
                let busy = self.sync_rx.is_some();
                if ui.add_enabled(!busy, egui::Button::new("Activer la synchro")).clicked() {
                    self.start_setup();
                }
                if busy {
                    ui.spinner();
                }
            });
        }
        ui.add_space(14.0);

        ui.heading("Rappels");
        ui.label("Les notifications ne partent que si Carnet est ouvert (même en arrière-plan).");
        if ui.checkbox(&mut self.autostart, "Ouvrir Carnet automatiquement à l'ouverture de session").changed() {
            let on = self.autostart;
            match config::set_autostart(on) {
                Ok(()) => self.status = if on { "Carnet se lancera à l'ouverture de session.".into() } else { "Lancement automatique désactivé.".into() },
                Err(e) => {
                    self.autostart = !on;
                    self.status = format!("Impossible de modifier le lancement auto : {e}");
                }
            }
        }
        if ui.button("Envoyer une notification de test").clicked() {
            notify::send("Carnet", "Les rappels fonctionnent ✔");
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();

        // Raccourcis (⌘ sur macOS)
        let new_note = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
        let tab1 = KeyboardShortcut::new(Modifiers::COMMAND, Key::Num1);
        let tab2 = KeyboardShortcut::new(Modifiers::COMMAND, Key::Num2);
        let sync_key = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
        let settings_key = KeyboardShortcut::new(Modifiers::COMMAND, Key::Comma);
        if ctx.input_mut(|i| i.consume_shortcut(&settings_key)) {
            self.tab = Tab::Settings;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&new_note)) {
            self.create_note();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&tab1)) {
            self.tab = Tab::Tasks;
            self.focus_new_task = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&tab2)) {
            self.tab = Tab::Notes;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sync_key)) {
            self.start_sync();
        }

        self.poll_sync(&ctx);

        // Synchro automatique périodique.
        if self.has_remote {
            if self.last_sync.elapsed() >= AUTO_SYNC {
                self.start_sync();
            }
            ctx.request_repaint_after(Duration::from_secs(60));
        }

        // Sauvegarde différée de la note en cours d'édition.
        if let Some(t) = self.dirty_since {
            if t.elapsed() >= SAVE_DELAY {
                self.flush_note();
            } else {
                ctx.request_repaint_after(SAVE_DELAY);
            }
        }

        egui::Panel::top("tabs").show(root, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(self.tab == Tab::Tasks, "✅ Tâches  (⌘1)").clicked() {
                    self.tab = Tab::Tasks;
                    self.focus_new_task = true;
                }
                if ui.selectable_label(self.tab == Tab::Notes, "📝 Notes  (⌘2)").clicked() {
                    self.tab = Tab::Notes;
                }
                if ui.selectable_label(self.tab == Tab::Settings, "⚙ Réglages  (⌘,)").clicked() {
                    self.tab = Tab::Settings;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.sync_rx.is_some() {
                        ui.spinner();
                    } else if self.has_remote && ui
                        .button("⟳ Synchro  (⌘S)")
                        .on_hover_text("Envoie et récupère les changements")
                        .clicked()
                    {
                        self.start_sync();
                    }
                });
            });
        });

        egui::Panel::bottom("status").show(root, |ui| {
            let msg = if self.status.is_empty() {
                format!("Dossier : {}", self.store.root().display())
            } else {
                self.status.clone()
            };
            ui.label(RichText::new(msg).small().weak());
        });

        match self.tab {
            Tab::Tasks => {
                egui::CentralPanel::default_margins().show(root, |ui| self.tasks_ui(ui));
            }
            Tab::Notes => self.notes_ui(root),
            Tab::Settings => {
                egui::CentralPanel::default_margins().show(root, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| self.settings_ui(ui));
                });
            }
        }
    }

    fn on_exit(&mut self) {
        self.flush_note();
        self.save_tasks();
        if self.has_remote {
            let _ = sync::sync(self.store.root());
        }
    }
}

fn main() -> eframe::Result<()> {
    let store = match Store::open_default() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Impossible d'ouvrir le dossier de données : {e}");
            std::process::exit(1);
        }
    };
    notify::spawn_reminders();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 640.0])
            .with_title("Carnet"),
        ..Default::default()
    };
    eframe::run_native(
        "Carnet",
        options,
        Box::new(|_cc| Ok(Box::new(App::new(store)))),
    )
}
