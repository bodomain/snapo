use crate::{
    command,
    history::{self, Entry, Filter, Session, View},
    stats,
    store::Store,
    sync::{self, Remote},
    timer::{self, DeadlineWatcher, Phase, Plan, Run},
};
use anyhow::Result;
use chrono::{Datelike, Local, NaiveDate};
use eframe::egui::{
    self, Align, Align2, Color32, FontFamily, FontId, Layout, RichText, Sense, Stroke, Vec2,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

#[derive(Clone, Copy)]
struct Palette {
    dark: bool,
    ink: Color32,
    muted: Color32,
    accent: Color32,
    tint: Color32,
    background: Color32,
    line: Color32,
    teal: Color32,
    danger: Color32,
    surface: Color32,
    notice_error_bg: Color32,
    activity: [Color32; 6],
}

const LIGHT: Palette = Palette {
    dark: false,
    ink: Color32::from_rgb(30, 43, 66),
    muted: Color32::from_rgb(103, 117, 139),
    accent: Color32::from_rgb(64, 88, 204),
    tint: Color32::from_rgb(233, 238, 252),
    background: Color32::from_rgb(245, 247, 251),
    line: Color32::from_rgb(222, 228, 238),
    teal: Color32::from_rgb(39, 133, 121),
    danger: Color32::from_rgb(175, 61, 77),
    surface: Color32::WHITE,
    notice_error_bg: Color32::from_rgb(252, 237, 240),
    activity: [
        Color32::from_rgb(64, 88, 204),
        Color32::from_rgb(39, 133, 121),
        Color32::from_rgb(186, 130, 57),
        Color32::from_rgb(143, 99, 174),
        Color32::from_rgb(191, 100, 120),
        Color32::from_rgb(64, 143, 173),
    ],
};

const DARK: Palette = Palette {
    dark: true,
    ink: Color32::from_rgb(232, 237, 247),
    muted: Color32::from_rgb(150, 160, 180),
    accent: Color32::from_rgb(122, 146, 255),
    tint: Color32::from_rgb(40, 49, 73),
    background: Color32::from_rgb(17, 21, 31),
    line: Color32::from_rgb(45, 54, 78),
    teal: Color32::from_rgb(77, 183, 168),
    danger: Color32::from_rgb(235, 120, 135),
    surface: Color32::from_rgb(25, 30, 45),
    notice_error_bg: Color32::from_rgb(63, 34, 44),
    activity: [
        Color32::from_rgb(122, 146, 255),
        Color32::from_rgb(77, 183, 168),
        Color32::from_rgb(224, 168, 92),
        Color32::from_rgb(168, 124, 200),
        Color32::from_rgb(214, 122, 144),
        Color32::from_rgb(96, 166, 200),
    ],
};

impl Palette {
    fn of(theme: Theme) -> Self {
        match theme {
            Theme::Light => LIGHT,
            Theme::Dark => DARK,
        }
    }
}

thread_local! {
    static PALETTE: std::cell::Cell<Palette> = const { std::cell::Cell::new(LIGHT) };
}

fn set_palette(palette: Palette) {
    PALETTE.with(|slot| slot.set(palette));
}

fn palette() -> Palette {
    PALETTE.with(|slot| slot.get())
}

fn ink() -> Color32 {
    palette().ink
}
fn subdued() -> Color32 {
    palette().muted
}
fn accent() -> Color32 {
    palette().accent
}
fn tint() -> Color32 {
    palette().tint
}
fn background() -> Color32 {
    palette().background
}
fn hairline() -> Color32 {
    palette().line
}
fn teal() -> Color32 {
    palette().teal
}
fn danger() -> Color32 {
    palette().danger
}
fn surface() -> Color32 {
    palette().surface
}
fn notice_error_bg() -> Color32 {
    palette().notice_error_bg
}
/// Ring and heading color of the running phase: focus in accent, breaks in teal.
fn phase_color(phase: Phase) -> Color32 {
    match phase {
        Phase::Work => accent(),
        Phase::ShortBreak | Phase::LongBreak => teal(),
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Page {
    #[default]
    #[value(alias = "history")]
    Focus,
    Stats,
    Trash,
    Conflicts,
    Settings,
}
impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Focus => "Fokus & Verlauf",
            Self::Stats => "Stats",
            Self::Trash => "Papierkorb",
            Self::Conflicts => "Konflikte",
            Self::Settings => "Einstellungen",
        }
    }
    fn subtitle(self) -> &'static str {
        match self {
            Self::Focus | Self::Settings => "",
            Self::Stats => "Die Verlaufsfilter gelten auch hier.",
            Self::Trash => "Gelöschte Sitzungen bleiben erhalten und lassen sich wiederherstellen.",
            Self::Conflicts => {
                "Vergleiche parallele Änderungen und entscheide, welche Version gilt."
            }
        }
    }
    fn view(self) -> View {
        match self {
            Self::Trash => View::Trash,
            Self::Conflicts => View::Conflicts,
            _ => View::History,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TimerDetails {
    Plan,
    Command,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    plan: Plan,
    activity: String,
    comment: String,
    weekly: bool,
    include_calendar_days: bool,
    stats_year: i32,
    stats_granularity: bool,
    theme: Theme,
}

struct Editor {
    entry: Option<Entry>,
    session: Session,
    minutes: String,
    deleted: bool,
    version: usize,
    error: Option<String>,
}
impl Editor {
    fn new(entry: Option<Entry>) -> Self {
        let revision = entry.as_ref().and_then(|e| e.heads.first());
        let session = revision
            .map(|r| r.session.clone())
            .unwrap_or_else(|| Session::now("", 25.0, ""));
        Self {
            minutes: session.duration_minutes.to_string(),
            session,
            deleted: revision.is_some_and(|r| r.deleted),
            entry,
            version: 0,
            error: None,
        }
    }
    fn save(&mut self, store: &mut Store) -> Result<()> {
        self.session.duration_minutes = self
            .minutes
            .replace(',', ".")
            .parse()
            .map_err(|_| anyhow::anyhow!("Die Dauer bitte als Zahl in Minuten eingeben."))?;
        if let Some(entry) = &self.entry {
            store.change(
                &entry.session_id,
                &entry.head_ids(),
                self.session.clone(),
                self.deleted,
                entry.conflicted(),
            )
        } else {
            store.create(self.session.clone(), None).map(|_| ())
        }
    }
}

enum Dialog {
    Editor(Box<Editor>),
    Export {
        path: String,
        entries: Vec<Entry>,
        error: Option<String>,
    },
    Delete(Entry),
    Discard,
    Quit,
    Import {
        path: String,
        error: Option<String>,
    },
}

pub struct Aftermath {
    store: Store,
    preferences: Preferences,
    entries: Vec<Entry>,
    page: Page,
    filter: Filter,
    show_filters: bool,
    selected: Option<String>,
    last_row_click: Option<egui::Id>,
    timer_details: Option<TimerDetails>,
    command: String,
    run: Option<Run>,
    watcher: DeadlineWatcher,
    applied_theme: Theme,
    save_failed: bool,
    dialog: Option<Dialog>,
    notice: Option<(String, bool)>,
    remote: Option<Remote>,
    configured: Result<Remote, String>,
    sync_error: Option<String>,
    syncing: Option<mpsc::Receiver<Result<(), String>>>,
    sync_again: bool,
    last_sync_attempt: Instant,
    last_reload: Instant,
    last_success: Option<String>,
    pending: usize,
    allow_close: bool,
    screenshot: Option<PathBuf>,
    screenshot_requested: bool,
    frames: u32,
}

impl Aftermath {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        store: Store,
        remote: Option<Remote>,
        configured: Result<Remote>,
        screenshot: Option<PathBuf>,
        page: Page,
    ) -> Self {
        let preferences: Preferences = cc
            .storage
            .and_then(|s| eframe::get_value(s, "snapo-preferences"))
            .unwrap_or_default();
        let palette = Palette::of(preferences.theme);
        set_palette(palette);
        configure_style(&cc.egui_ctx, palette);
        let mut app = Self::workspace(
            store,
            remote,
            configured.map_err(|e| e.to_string()),
            preferences,
            DeadlineWatcher::spawn(),
        );
        app.screenshot = screenshot;
        app.page = page;
        if app.remote.is_some() {
            app.start_sync(&cc.egui_ctx);
        }
        app
    }
    fn workspace(
        store: Store,
        remote: Option<Remote>,
        configured: Result<Remote, String>,
        preferences: Preferences,
        watcher: DeadlineWatcher,
    ) -> Self {
        let now = Instant::now();
        let theme = preferences.theme;
        let mut app = Self {
            store,
            preferences,
            entries: vec![],
            page: Page::Focus,
            filter: Filter::default(),
            show_filters: false,
            selected: None,
            last_row_click: None,
            timer_details: None,
            command: String::new(),
            run: None,
            watcher,
            applied_theme: theme,
            save_failed: false,
            dialog: None,
            notice: None,
            remote,
            configured,
            sync_error: None,
            syncing: None,
            sync_again: false,
            last_sync_attempt: now,
            last_reload: now,
            last_success: None,
            pending: 0,
            allow_close: false,
            screenshot: None,
            screenshot_requested: false,
            frames: 0,
        };
        app.reload();
        if app.preferences.stats_year == 0 {
            app.preferences.stats_year = Local::now().year();
        }
        app
    }
    fn notify(&mut self, text: impl Into<String>, error: bool) {
        self.notice = Some((text.into(), error));
    }
    fn reload(&mut self) {
        self.last_reload = Instant::now();
        match self.store.entries() {
            Ok(entries) => self.entries = entries,
            Err(e) => self.notify(format!("Verlauf konnte nicht geladen werden: {e}"), true),
        }
        match self.store.pending() {
            Ok(count) => self.pending = count,
            Err(e) => self.notify(e.to_string(), true),
        }
        match self.store.sync_status() {
            Ok(time) => self.last_success = time,
            Err(e) => self.notify(e.to_string(), true),
        }
    }
    fn saved(&mut self, text: &str, ctx: &egui::Context) {
        self.reload();
        self.notify(text, false);
        if self.remote.is_some() {
            self.start_sync(ctx);
        }
    }
    fn filtered(&self, view: View) -> Vec<Entry> {
        if self.filter.validate().is_err() {
            return vec![];
        }
        self.entries
            .iter()
            .filter(|e| self.filter.matches(e, view))
            .cloned()
            .collect()
    }
    fn selected_entry(&self) -> Option<Entry> {
        self.filter.validate().ok()?;
        self.entries
            .iter()
            .find(|e| {
                Some(&e.session_id) == self.selected.as_ref()
                    && self.filter.matches(e, self.page.view())
            })
            .cloned()
    }
    fn start_sync(&mut self, ctx: &egui::Context) {
        let Some(remote) = self.remote.clone() else {
            return;
        };
        if self.syncing.is_some() {
            self.sync_again = true;
            return;
        }
        let path = self.store.path.clone();
        let ctx = ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.syncing = Some(rx);
        self.last_sync_attempt = Instant::now();
        self.sync_again = false;
        std::thread::spawn(move || {
            let result =
                sync::synchronize(&path, &remote).map_err(|e| sync::safe_error(&e, &remote));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    fn poll_sync(&mut self, ctx: &egui::Context) {
        if let Some(receiver) = &self.syncing {
            let result = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                    "Synchronisierung wurde unterbrochen. Bitte erneut versuchen.".into(),
                )),
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                self.syncing = None;
                self.sync_error = result.err();
                self.reload();
                if self.sync_again && self.remote.is_some() {
                    self.start_sync(ctx);
                }
            }
        }
        if self.remote.is_some()
            && self.syncing.is_none()
            && self.last_sync_attempt.elapsed() >= Duration::from_secs(15)
        {
            self.start_sync(ctx);
        }
    }
    fn start_timer(&mut self, now: Instant) {
        match Run::new(
            self.preferences.plan.clone(),
            self.preferences.activity.clone(),
            self.preferences.comment.clone(),
            now,
        ) {
            Ok(run) => {
                self.run = Some(run);
                self.save_failed = false;
                self.notice = None;
            }
            Err(e) => self.notify(e.to_string(), true),
        }
    }
    /// Applies a `prodz`-style command line from the focus panel and starts it.
    fn execute_command(&mut self, now: Instant) {
        match command::parse(&self.command) {
            Ok(definition) => {
                let preferences = &mut self.preferences;
                if let Some(activity) = definition.activity {
                    preferences.activity = activity;
                }
                if let Some(comment) = definition.comment {
                    preferences.comment = comment;
                }
                if let Some(work) = definition.work {
                    preferences.plan.work = work;
                }
                if let Some(short_break) = definition.short_break {
                    preferences.plan.short_break = short_break;
                }
                if let Some(long_break) = definition.long_break {
                    preferences.plan.long_break = long_break;
                }
                if let Some(cycles) = definition.cycles {
                    preferences.plan.cycles = cycles;
                }
                if let Some(endless) = definition.endless {
                    preferences.plan.endless = endless;
                }
                self.start_timer(now);
                if self.run.is_some() {
                    self.command.clear();
                }
            }
            Err(e) => self.notify(e.to_string(), true),
        }
    }
    fn finish_timer(&mut self, advance: bool, completion: Instant, live: bool) -> Result<()> {
        let Some(run) = self.run.as_mut() else {
            return Ok(());
        };
        if run.phase == Phase::Work {
            let session = run.clock.snapshot(completion)?;
            self.store.create(session, Some(&run.clock.id))?;
        }
        let sound = advance && run.plan.sound;
        let continued = advance && run.advance(completion)?;
        if !continued {
            self.run = None;
        }
        self.save_failed = false;
        // While the tick loop was stalled on a hidden workspace, the watcher
        // already rang the bell (or several) on time; never ring twice for
        // the same phase, and never ring belatedly for catch-up phases.
        let rung = self.watcher.take_rung();
        if sound && live && !rung {
            timer::bell();
        }
        Ok(())
    }
    fn finish_action(
        &mut self,
        advance: bool,
        ctx: &egui::Context,
        completion: Instant,
        live: bool,
    ) -> bool {
        let was_work = self
            .run
            .as_ref()
            .is_some_and(|run| run.phase == Phase::Work);
        match self.finish_timer(advance, completion, live) {
            Ok(()) => {
                self.saved(
                    if was_work {
                        "Fokuszeit lokal gespeichert. Pausenzeiten werden nicht erfasst."
                    } else {
                        "Pause beendet. Pausenzeiten werden nicht erfasst."
                    },
                    ctx,
                );
                true
            }
            Err(e) => {
                self.save_failed = true;
                self.notify(format!("Speichern fehlgeschlagen. Deine Fokuszeit bleibt für einen erneuten Versuch erhalten: {e}"), true);
                false
            }
        }
    }
    fn tick(&mut self, ctx: &egui::Context) {
        // Catch up on phases that ended while the render loop was stalled on
        // a hidden workspace: advance at the exact completion instants, so
        // the next phase shows the time that has really passed instead of
        // starting fresh when the window regains focus.
        let now = Instant::now();
        while !self.save_failed {
            let completed = self
                .run
                .as_ref()
                .and_then(|run| run.clock.completes_at())
                .filter(|completion| *completion <= now);
            let Some(completion) = completed else {
                break;
            };
            // A completion the user is still seeing: ring the normal bell
            // unless the watcher already did. A stale one from a stalled
            // loop is silent – the watcher rang it on time.
            let live = now.saturating_duration_since(completion) < Duration::from_secs(2);
            self.finish_action(true, ctx, completion, live);
        }
        if self.last_reload.elapsed() > Duration::from_secs(2) {
            self.reload();
        }
        self.poll_sync(ctx);
    }
    /// Publishes the current completion deadline and the remaining phase
    /// sequence to the background bell watcher (see `DeadlineWatcher`). One
    /// call per frame is enough: while the loop is stalled on a hidden
    /// workspace nothing can change the run, so the last published chain
    /// stays valid.
    fn resync_deadline(&mut self) {
        let now = Instant::now();
        let scheduled = if self.save_failed {
            // The phase is stuck on a failed save; an overdue deadline
            // would ring on every frame until the retry succeeds.
            None
        } else {
            self.run.as_ref().and_then(|run| {
                if run.plan.sound && !run.clock.paused {
                    let remaining = run.clock.remaining(now)?;
                    Some((run.plan.clone(), run.phase, run.cycle, now + remaining))
                } else {
                    None
                }
            })
        };
        match scheduled {
            Some((plan, phase, cycle, at)) => self.watcher.schedule_run(plan, phase, cycle, at),
            None => self.watcher.cancel(),
        }
    }
    fn request_close(&mut self, ctx: &egui::Context) {
        if self.run.is_some() {
            self.dialog = Some(Dialog::Quit);
        } else {
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    fn edit_selected(&mut self) {
        if let Some(entry) = self.selected_entry() {
            self.dialog = Some(Dialog::Editor(Box::new(Editor::new(Some(entry)))));
        }
    }
    fn restore_selected(&mut self, ctx: &egui::Context) {
        if let Some(entry) = self.selected_entry()
            && !entry.conflicted()
        {
            match self
                .store
                .set_deleted(&entry.session_id, &entry.head_ids(), false)
            {
                Ok(()) => self.saved("Sitzung wiederhergestellt.", ctx),
                Err(e) => self.notify(e.to_string(), true),
            }
        }
    }
    fn export_dialog(&mut self) {
        if let Err(e) = self.filter.validate() {
            self.notify(e.to_string(), true);
            return;
        }
        let directory = directories::UserDirs::new()
            .map(|d| d.home_dir().to_owned())
            .unwrap_or_else(|| PathBuf::from("."));
        self.dialog = Some(Dialog::Export {
            path: directory
                .join(format!(
                    "aftermath-{}.csv",
                    Local::now().format("%Y%m%d-%H%M%S")
                ))
                .display()
                .to_string(),
            entries: self.filtered(View::History),
            error: None,
        });
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.dialog.is_some() {
            return;
        }
        let pressed = |key| ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, key));
        if pressed(egui::Key::N) {
            self.dialog = Some(Dialog::Editor(Box::new(Editor::new(None))));
        }
        if pressed(egui::Key::E) {
            self.edit_selected();
        }
        if pressed(egui::Key::D)
            && self.page == Page::Focus
            && let Some(entry) = self.selected_entry().filter(|e| !e.conflicted())
        {
            self.dialog = Some(Dialog::Delete(entry));
        }
        if pressed(egui::Key::R) && self.page == Page::Trash {
            self.restore_selected(ctx);
        }
        if pressed(egui::Key::S) {
            self.start_sync(ctx);
        }
        if pressed(egui::Key::Q) {
            self.request_close(ctx);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
            self.reload();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F2))
            && !self.save_failed
            && let Some(run) = &mut self.run
        {
            run.clock.toggle_pause(Instant::now());
        }
    }
    fn tabs(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("navigation")
            .frame(
                egui::Frame::new()
                    .fill(surface())
                    .inner_margin(egui::Margin::symmetric(14, 7)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let conflicts = self.entries.iter().filter(|e| e.conflicted()).count();
                    for page in [
                        Page::Focus,
                        Page::Stats,
                        Page::Trash,
                        Page::Conflicts,
                        Page::Settings,
                    ] {
                        let label = if page == Page::Conflicts && conflicts > 0 {
                            format!("Konflikte ({conflicts})")
                        } else {
                            page.title().to_owned()
                        };
                        if nav(ui, &label, self.page == page) && self.page != page {
                            self.page = page;
                            self.selected = None;
                        }
                    }
                    if self.page != Page::Focus
                        && let Some(run) = &self.run
                    {
                        let now = Instant::now();
                        let remaining = run.clock.remaining(now);
                        let time = clock_text(
                            remaining.unwrap_or_else(|| run.clock.elapsed(now)),
                            remaining.is_some(),
                        );
                        let status = if self.save_failed {
                            "Speicherfehler"
                        } else if run.clock.paused {
                            "Pausiert"
                        } else {
                            run.phase.label()
                        };
                        if ui
                            .button(RichText::new(format!("{status} · {time}")).color(accent()))
                            .on_hover_text("Zum Timer")
                            .clicked()
                        {
                            self.page = Page::Focus;
                            self.selected = None;
                        }
                    }
                });
            });
    }
    fn footer(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(surface())
                    .inner_margin(egui::Margin::symmetric(14, 5)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if self.syncing.is_some() {
                        ui.spinner();
                        ui.label("Synchronisierung läuft …");
                    } else if self.remote.is_none() {
                        ui.colored_label(subdued(), "●  Nur lokal");
                    } else if self.sync_error.is_some() {
                        ui.colored_label(danger(), "●  Synchronisierung fehlgeschlagen");
                    } else {
                        ui.colored_label(teal(), "●  Cloud-Synchronisierung aktiv");
                    }
                    if self.remote.is_some() {
                        ui.label(
                            RichText::new(format!("{} Revisionen ausstehend", self.pending))
                                .size(12.0)
                                .color(subdued()),
                        );
                        ui.label(
                            RichText::new(format!(
                                "Zuletzt: {}",
                                self.last_success
                                    .as_deref()
                                    .map(short_timestamp)
                                    .unwrap_or_else(|| "noch nie".into())
                            ))
                            .size(12.0)
                            .color(subdued()),
                        );
                        if ui
                            .add_enabled(
                                self.syncing.is_none(),
                                egui::Button::new("Jetzt synchronisieren"),
                            )
                            .clicked()
                        {
                            self.start_sync(ctx);
                        }
                    }
                    if self.sync_error.is_some() && ui.small_button("Details").clicked() {
                        self.page = Page::Settings;
                        self.selected = None;
                    }
                });
            });
    }
    fn timer_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let now = Instant::now();
            if ui.available_width() >= 520.0 {
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(264.0, 0.0),
                        Layout::top_down(Align::Center),
                        |ui| {
                            ui.set_width(264.0);
                            self.timer_dial(ui, now);
                        },
                    );
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width());
                        self.timer_info(ui);
                        self.timer_controls(ui, ctx, now);
                    });
                });
            } else {
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    self.timer_dial(ui, now);
                    self.timer_info(ui);
                });
                self.timer_controls(ui, ctx, now);
            }
            if self.run.is_none()
                && let Some(details) = self.timer_details
            {
                ui.separator();
                match details {
                    TimerDetails::Plan => self.timer_plan(ui),
                    TimerDetails::Command => self.timer_command(ui, now),
                }
            }
        });
    }
    /// The focus timer as a large clock dial: a watch face whose progress ring
    /// sweeps like a hand, with the time at its center. The dial claims the
    /// available room but stays proportionate on short windows, so the compact
    /// layout keeps room for the history below.
    fn timer_dial(&self, ui: &mut egui::Ui, now: Instant) {
        let (time, progress, phase, paused) = if let Some(run) = &self.run {
            let elapsed = run.clock.elapsed(now);
            let remaining = run.clock.remaining(now);
            (
                clock_text(remaining.unwrap_or(elapsed), remaining.is_some()),
                run.clock
                    .target
                    .map(|target| elapsed.as_secs_f32() / target.as_secs_f32()),
                run.phase,
                run.clock.paused,
            )
        } else {
            (
                if self.preferences.plan.endless {
                    "00:00".into()
                } else {
                    format!("{:02}:00", self.preferences.plan.work)
                },
                None,
                Phase::Work,
                false,
            )
        };
        // Height budget of navigation, status bar and panel margins. The dial
        // shrinks on short windows so the history below stays visible even
        // with open filters, and it stops at the column width on tall ones.
        const CHROME: f32 = 110.0;
        let diameter = ui
            .available_width()
            .min((ui.ctx().viewport_rect().height() - CHROME) * 0.36)
            .clamp(150.0, 252.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(diameter), Sense::hover());
        let painter = ui.painter().clone();
        let center = rect.center();
        let weight = 13.0;
        let radius = diameter / 2.0 - weight / 2.0 - 1.0;
        let face = Color32::from_rgba_unmultiplied(tint().r(), tint().g(), tint().b(), 38);
        painter.circle_filled(center, radius, face);
        for tick in 0..12 {
            let angle = tick as f32 * (std::f32::consts::TAU / 12.0);
            let direction = Vec2::new(angle.cos(), angle.sin());
            let outer = radius - weight / 2.0 - 4.0;
            painter.line_segment(
                [
                    center + direction * (outer - 6.0),
                    center + direction * outer,
                ],
                Stroke::new(2.0_f32, hairline()),
            );
        }
        painter.circle_stroke(center, radius, Stroke::new(weight, tint()));
        if let Some(progress) = progress.filter(|progress| *progress > 0.0) {
            let color = if self.save_failed {
                danger()
            } else {
                let color = phase_color(phase);
                if paused {
                    color.gamma_multiply(0.55)
                } else {
                    color
                }
            };
            if progress >= 1.0 {
                painter.circle_stroke(center, radius, Stroke::new(weight, color));
            } else {
                let sweep = progress * std::f32::consts::TAU;
                let steps = (sweep * 48.0).round().max(2.0) as usize;
                let points = (0..=steps)
                    .map(|step| {
                        let angle =
                            -std::f32::consts::FRAC_PI_2 + sweep * (step as f32 / steps as f32);
                        center + Vec2::new(angle.cos(), angle.sin()) * radius
                    })
                    .collect::<Vec<_>>();
                painter.add(egui::Shape::line(points, Stroke::new(weight, color)));
            }
        }
        let shrink = if time.len() > 5 {
            5.0 / time.len() as f32
        } else {
            1.0
        };
        painter.text(
            center + Vec2::new(0.0, -diameter * 0.05),
            Align2::CENTER_CENTER,
            time,
            FontId::monospace(diameter * 0.24 * shrink),
            ink(),
        );
        // The phase label sits inside the dial, so the card needs no heading
        // row beside the clock and stays flat on short windows.
        let word = if self.run.is_some() {
            phase.label()
        } else {
            "Fokustimer"
        };
        let word_color = if self.run.is_none() {
            ink()
        } else if self.save_failed {
            danger()
        } else {
            phase_color(phase)
        };
        painter.text(
            center + Vec2::new(0.0, diameter * 0.13),
            Align2::CENTER_CENTER,
            word,
            FontId::proportional((diameter * 0.09).clamp(13.0, 18.0)),
            word_color,
        );
        let status = if self.save_failed {
            Some(("Speicherfehler", danger()))
        } else if paused {
            Some(("pausiert", subdued()))
        } else {
            None
        };
        if let Some((status, color)) = status {
            painter.text(
                center + Vec2::new(0.0, diameter * 0.25),
                Align2::CENTER_CENTER,
                status,
                FontId::proportional((diameter * 0.07).clamp(10.0, 13.0)),
                color,
            );
        }
    }
    /// Context line beside the dial: cycle position while running, the plan
    /// summary while idle.
    fn timer_info(&self, ui: &mut egui::Ui) {
        if let Some(run) = &self.run {
            if run.clock.target.is_some() {
                muted(ui, &format!("Zyklus {} von {}", run.cycle, run.plan.cycles));
            } else {
                muted(ui, "Ohne Zeitlimit");
            }
        } else {
            let plan = &self.preferences.plan;
            if plan.endless {
                muted(ui, "Ohne Zeitlimit");
            } else {
                muted(
                    ui,
                    &format!(
                        "{} × {} min · {} min Pause",
                        plan.cycles, plan.work, plan.short_break
                    ),
                );
            }
        }
    }
    fn timer_controls(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, now: Instant) {
        if let Some(run) = &self.run {
            let paused = run.clock.paused;
            let pause = run.phase != Phase::Work;
            ui.add(
                egui::Label::new(
                    RichText::new(&run.clock.session.activity)
                        .size(18.0)
                        .strong(),
                )
                .truncate(),
            )
            .on_hover_text(&run.clock.session.activity);
            if !run.clock.session.comment.is_empty() {
                ui.add(
                    egui::Label::new(RichText::new(&run.clock.session.comment).color(subdued()))
                        .truncate(),
                )
                .on_hover_text(&run.clock.session.comment);
            }
            ui.horizontal_wrapped(|ui| {
                if self.save_failed {
                    if primary(ui, "Speichern wiederholen", 0.0).clicked() {
                        let complete = self.run.as_ref().is_some_and(|r| r.clock.complete(now));
                        self.finish_action(complete, ctx, now, true);
                    }
                } else if primary(ui, if paused { "Fortsetzen" } else { "Pausieren" }, 0.0)
                    .on_hover_text("F2")
                    .clicked()
                    && let Some(run) = &mut self.run
                {
                    run.clock.toggle_pause(now);
                }
                if !self.save_failed
                    && ui
                        .button(if pause {
                            "Timer beenden"
                        } else {
                            "Speichern & beenden"
                        })
                        .clicked()
                {
                    self.finish_action(false, ctx, now, true);
                }
                if ui.button("Verwerfen").clicked() {
                    self.dialog = Some(Dialog::Discard);
                }
            });
            return;
        }

        let mut start = false;
        ui.columns(2, |columns| {
            columns[0].label("Aktivität");
            let input = columns[0].add(
                egui::TextEdit::singleline(&mut self.preferences.activity)
                    .hint_text("Was steht an?")
                    .min_size(Vec2::new(0.0, 29.0))
                    .desired_width(f32::INFINITY),
            );
            start = input.lost_focus() && columns[0].input(|i| i.key_pressed(egui::Key::Enter));
            columns[1].label("Notiz");
            let input = columns[1].add(
                egui::TextEdit::singleline(&mut self.preferences.comment)
                    .hint_text("Optional")
                    .min_size(Vec2::new(0.0, 29.0))
                    .desired_width(f32::INFINITY),
            );
            start |= input.lost_focus() && columns[1].input(|i| i.key_pressed(egui::Key::Enter));
        });
        ui.horizontal_wrapped(|ui| {
            if primary(ui, "Fokus starten", 0.0).clicked() || start {
                self.start_timer(now);
            }
            for (details, label) in [
                (TimerDetails::Plan, "Timer-Plan"),
                (TimerDetails::Command, "Command-Line"),
            ] {
                let expanded = self.timer_details == Some(details);
                if ui
                    .add(egui::Button::new(label).selected(expanded))
                    .clicked()
                {
                    self.timer_details = (!expanded).then_some(details);
                }
            }
        });
    }
    fn timer_plan(&mut self, ui: &mut egui::Ui) {
        let plan = &mut self.preferences.plan;
        ui.add_enabled_ui(!plan.endless, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (label, value, max) in [
                    ("Fokus", &mut plan.work, 240),
                    ("Kurze Pause", &mut plan.short_break, 120),
                    ("Lange Pause", &mut plan.long_break, 180),
                    ("Zyklen", &mut plan.cycles, 24),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        let duration = egui::DragValue::new(value).range(1..=max);
                        ui.add(if label == "Zyklen" {
                            duration
                        } else {
                            duration.suffix(" min")
                        });
                    });
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut plan.endless, "Ohne Zeitlimit")
                .on_hover_text(
                    "Misst deine Fokuszeit aufwärts, bis du sie speicherst oder verwirfst.",
                );
            ui.checkbox(&mut plan.sound, "Ton am Ende");
            muted(
                ui,
                "Lange Pause nach jedem vierten Zyklus, sofern ein weiterer folgt.",
            );
        });
    }
    fn timer_command(&mut self, ui: &mut egui::Ui, now: Instant) {
        ui.horizontal(|ui| {
            let input = ui.add(
                egui::TextEdit::singleline(&mut self.command)
                    .hint_text("Lernen -w 30 -c 2 -t \"Vokabeln\"")
                    .min_size(Vec2::new(0.0, 29.0))
                    .desired_width(ui.available_width() - 88.0),
            );
            let submit = input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Starten").clicked() || submit {
                self.execute_command(now);
            }
        });
        muted(
            ui,
            "-a Aktivität · -t Notiz · -w Fokus · -b Kurze Pause · -lb Lange Pause · -c Zyklen · -e Ohne Zeitlimit",
        );
    }
    fn filters(&mut self, ui: &mut egui::Ui) {
        let extra_filters = [
            &self.filter.activity,
            &self.filter.since,
            &self.filter.until,
        ]
        .iter()
        .filter(|value| !value.is_empty())
        .count();
        let active = extra_filters > 0 || !self.filter.search.is_empty();
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filter.search)
                    .hint_text("Sitzungen suchen …")
                    .min_size(Vec2::new(0.0, 29.0))
                    .desired_width(210.0),
            );
            let label = if extra_filters > 0 {
                format!("Filter ({extra_filters})")
            } else {
                "Filter".into()
            };
            if ui
                .add(egui::Button::new(label).selected(self.show_filters))
                .on_hover_text("Nach Aktivität und Datum filtern")
                .clicked()
            {
                self.show_filters = !self.show_filters;
            }
            if active && ui.button("Filter zurücksetzen").clicked() {
                self.filter = Filter::default();
            }
        });
        if self.show_filters {
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter.activity)
                        .hint_text("Aktivität filtern")
                        .desired_width(150.0),
                );
                for (label, value) in [
                    ("Von", &mut self.filter.since),
                    ("Bis", &mut self.filter.until),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        ui.add(
                            egui::TextEdit::singleline(value)
                                .hint_text("JJJJ-MM-TT")
                                .desired_width(100.0),
                        );
                    });
                }
            });
        }
        if let Err(e) = self.filter.validate() {
            ui.colored_label(danger(), e.to_string());
        }
    }
    fn history(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        previous_click: Option<egui::Id>,
    ) {
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            if self.page == Page::Focus {
                let today = Local::now().date_naive().to_string();
                let (count, minutes) = self
                    .entries
                    .iter()
                    .filter_map(Entry::active)
                    .filter(|session| session.date == today)
                    .fold((0, 0.0), |(count, minutes), session| {
                        (count + 1, minutes + session.duration_minutes)
                    });
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Verlauf").size(20.0).strong());
                    let sessions = if count == 1 { "Sitzung" } else { "Sitzungen" };
                    muted(
                        ui,
                        &format!("Heute · {} · {count} {sessions}", format_minutes(minutes)),
                    );
                });
            }
            self.filters(ui);
            let entries = self.filtered(self.page.view());
            self.history_actions(ui, ctx, entries.len());

            if entries.is_empty() {
                let message = if [
                    &self.filter.search,
                    &self.filter.activity,
                    &self.filter.since,
                    &self.filter.until,
                ]
                .iter()
                .any(|value| !value.is_empty())
                {
                    "Keine passenden Sitzungen."
                } else {
                    match self.page {
                        Page::Trash => "Der Papierkorb ist leer.",
                        Page::Conflicts => "Es gibt keine offenen Konflikte.",
                        _ => "Starte den Timer oder ergänze eine Sitzung.",
                    }
                };
                ui.separator();
                muted(ui, message);
            } else {
                self.history_table(ui, &entries, previous_click);
            }
        });
    }
    fn history_actions(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, count: usize) {
        ui.horizontal_wrapped(|ui| {
            if self.page == Page::Focus {
                if ui
                    .button("+ Sitzung ergänzen")
                    .on_hover_text("Strg+N")
                    .clicked()
                {
                    self.dialog = Some(Dialog::Editor(Box::new(Editor::new(None))));
                }
                if ui.button("CSV exportieren").clicked() {
                    self.export_dialog();
                }
            }
            if let Some(entry) = self.selected_entry() {
                let edit = if entry.conflicted() {
                    "Konflikt auflösen"
                } else {
                    "Bearbeiten"
                };
                if ui.button(edit).on_hover_text("Strg+E").clicked() {
                    self.dialog = Some(Dialog::Editor(Box::new(Editor::new(Some(entry.clone())))));
                }
                if !entry.conflicted() {
                    match self.page {
                        Page::Focus
                            if ui.button("In Papierkorb").on_hover_text("Strg+D").clicked() =>
                        {
                            self.dialog = Some(Dialog::Delete(entry));
                        }
                        Page::Trash
                            if ui
                                .button("Wiederherstellen")
                                .on_hover_text("Strg+R")
                                .clicked() =>
                        {
                            self.restore_selected(ctx);
                        }
                        _ => {}
                    }
                }
            }
            if count > 0 {
                let sessions = if count == 1 { "Sitzung" } else { "Sitzungen" };
                muted(ui, &format!("{count} {sessions}"));
            }
        });
    }
    fn history_table(
        &mut self,
        ui: &mut egui::Ui,
        entries: &[Entry],
        previous_click: Option<egui::Id>,
    ) {
        const ROW_HEIGHT: f32 = 31.0;
        let width = ui.available_width();
        let (_, table) =
            ui.allocate_space(Vec2::new(width, ROW_HEIGHT * (entries.len() + 1) as f32));
        let row_rect = |index: usize| {
            egui::Rect::from_min_size(
                table.min + Vec2::new(0.0, index as f32 * ROW_HEIGHT),
                Vec2::new(width, ROW_HEIGHT),
            )
        };
        let header = row_rect(0);
        ui.painter().rect_filled(header, 5, background());
        history_cells(
            ui,
            header,
            ["Datum / Uhrzeit", "Aktivität", "Dauer", "Notiz", "Status"]
                .map(|label| RichText::new(label).strong().color(subdued())),
        );

        // Lay out the visible rows within the page's scroll area.
        let visible = ui.clip_rect().intersect(table);
        let first = ((visible.top() - table.top()) / ROW_HEIGHT).floor() as usize;
        let end = ((visible.bottom() - table.top()) / ROW_HEIGHT).ceil() as usize;
        for index in first.max(1)..end.min(entries.len() + 1) {
            let entry = &entries[index - 1];
            let session = &entry.heads[0].session;
            let row = row_rect(index);
            let selected = self.selected.as_ref() == Some(&entry.session_id);
            let response = ui.interact(row, ui.id().with(&entry.session_id), Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::SelectableLabel,
                    true,
                    selected,
                    &session.activity,
                )
            });
            if response.clicked() {
                self.selected = Some(entry.session_id.clone());
            }
            if response.clicked_by(egui::PointerButton::Primary) {
                self.last_row_click = Some(response.id);
            }
            if previous_click == Some(response.id)
                && (response.double_clicked() || response.triple_clicked())
            {
                self.dialog = Some(Dialog::Editor(Box::new(Editor::new(Some(entry.clone())))));
            }
            if selected || response.hovered() || response.has_focus() {
                ui.painter().rect_filled(row, 5, tint());
            } else if index % 2 == 0 {
                ui.painter().rect_filled(row, 5, background());
            }

            let mut validation_error = None;
            let status = if entry.conflicted() {
                RichText::new(format!("{} Versionen", entry.heads.len())).color(danger())
            } else if self.page == Page::Trash {
                RichText::new("Gelöscht").color(subdued())
            } else if let Err(error) = session.clone().validated() {
                validation_error = Some(error.to_string());
                RichText::new("Altdaten prüfen").color(danger())
            } else {
                RichText::new("")
            };
            let [timestamp, _, _, _, status] = history_cells(
                ui,
                row,
                [
                    RichText::new(short_timestamp(&session.timestamp)).color(subdued()),
                    RichText::new(&session.activity).strong(),
                    RichText::new(format_minutes(session.duration_minutes)),
                    RichText::new(&session.comment).color(subdued()),
                    status,
                ],
            );
            timestamp.on_hover_text(format!(
                "{}\nSitzung {}",
                session.timestamp, entry.session_id
            ));
            if let Some(error) = validation_error {
                status.on_hover_text(error);
            }
        }
    }
    fn stats(&mut self, ui: &mut egui::Ui) {
        self.filters(ui);
        let entries = self.filtered(View::History);
        let active: Vec<_> = entries.iter().filter_map(Entry::active).collect();
        let summary = stats::summarize(&active);

        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format_minutes(summary.total_minutes))
                    .size(30.0)
                    .strong(),
            );
            muted(ui, &format!("Fokuszeit · {} Sitzungen", summary.sessions));
            if ui.button("CSV exportieren").clicked() {
                self.export_dialog();
            }
        });
        ui.add_space(10.0);

        card().inner_margin(14).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                kpi(ui, &format_minutes(summary.total_minutes), "Gesamtzeit");
                kpi(ui, &summary.sessions.to_string(), "Sitzungen");
                kpi(ui, &summary.activities.to_string(), "Aktivitäten");
                kpi(
                    ui,
                    &format_minutes(summary.average_per_active_day),
                    "Ø pro Tag",
                );
                kpi(
                    ui,
                    &summary
                        .best_day
                        .as_ref()
                        .map(|(_, minutes)| format_minutes(*minutes))
                        .unwrap_or_else(|| "–".into()),
                    &summary
                        .best_day
                        .as_ref()
                        .map(|(date, _)| format!("Bester Tag · {}", short_date(date)))
                        .unwrap_or_else(|| "Bester Tag".into()),
                );
                kpi(ui, &summary.streak.to_string(), "Tage in Folge");
            });
        });
        ui.add_space(12.0);

        card().inner_margin(14).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Kalenderjahr").size(19.0).strong())
                    .on_hover_text("Aggregierte Fokuszeit über ein Kalenderjahr, aufgeteilt nach Monaten oder ISO-Wochen.");
                ui.add(
                    egui::DragValue::new(&mut self.preferences.stats_year)
                        .range(1970..=Local::now().year())
                        .prefix("Jahr "),
                );
                ui.selectable_value(&mut self.preferences.stats_granularity, false, "Monate");
                ui.selectable_value(&mut self.preferences.stats_granularity, true, "Wochen");
            });
            ui.add_space(6.0);
            let year = self.preferences.stats_year;
            let year_sessions = active
                .iter()
                .filter(|s| {
                    NaiveDate::parse_from_str(&s.date, "%Y-%m-%d")
                        .map(|date| date.year() == year)
                        .unwrap_or(false)
                })
                .count();
            let year_total = if self.preferences.stats_granularity {
                let weeks = stats::by_week(&active, year);
                let bars: Vec<(String, BTreeMap<String, f64>)> = (1..=53)
                    .map(|week| {
                        (
                            format!("W{week:02}"),
                            weeks.get(&week).cloned().unwrap_or_default(),
                        )
                    })
                    .collect();
                let total: f64 = weeks.values().flat_map(|m| m.values()).sum();
                stacked_bar_chart(ui, "Fokuszeit (Stunden)", "ISO-Woche", &bars, 220.0, 34.0);
                total
            } else {
                let months = stats::by_month(&active, year);
                let bars: Vec<(String, BTreeMap<String, f64>)> = (1..=12)
                    .map(|month| {
                        (
                            MONTHS[(month - 1) as usize].to_string(),
                            months.get(&month).cloned().unwrap_or_default(),
                        )
                    })
                    .collect();
                let total: f64 = months.values().flat_map(|m| m.values()).sum();
                stacked_bar_chart(ui, "Fokuszeit (Stunden)", "Monat", &bars, 220.0, 52.0);
                total
            };
            ui.add_space(6.0);
            muted(ui, &format!("{year} · {} · {} Sitzungen", format_minutes(year_total), year_sessions));
        });
        ui.add_space(12.0);

        card().inner_margin(14).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Fokuszeit pro Tag").size(19.0).strong())
                    .on_hover_text("Jedes Segment steht für eine Sitzung. Bewege den Zeiger darüber für Details.");
                if ui.add(egui::Button::new("Alle Kalendertage").selected(self.preferences.include_calendar_days))
                    .on_hover_text("Auch Tage ohne Fokuszeit anzeigen. Es gelten die Datumsfilter; ohne Datumsfilter gilt der Zeitraum zwischen der ersten und letzten Sitzung.")
                    .clicked() {
                    self.preferences.include_calendar_days = !self.preferences.include_calendar_days;
                }
            });
            ui.add_space(6.0);
            daily_focus_chart(ui, &active, self.preferences.include_calendar_days, &self.filter);
        });
        ui.add_space(12.0);

        let wide = ui.available_width() > 720.0;
        if wide {
            ui.columns(2, |columns| {
                card().show(&mut columns[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Wochentag").size(19.0).strong())
                        .on_hover_text("Gesamte Fokuszeit je Wochentag.");
                    ui.add_space(8.0);
                    weekday_chart(ui, &active);
                });
                card().show(&mut columns[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Tageszeit").size(19.0).strong())
                        .on_hover_text("Fokuszeit nach Uhrzeit des Sitzungsbeginns.");
                    ui.add_space(8.0);
                    hour_chart(ui, &active);
                });
            });
        } else {
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Wochentag").size(19.0).strong())
                    .on_hover_text("Gesamte Fokuszeit je Wochentag.");
                ui.add_space(8.0);
                weekday_chart(ui, &active);
            });
            ui.add_space(12.0);
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Tageszeit").size(19.0).strong())
                    .on_hover_text("Fokuszeit nach Uhrzeit des Sitzungsbeginns.");
                ui.add_space(8.0);
                hour_chart(ui, &active);
            });
        }
        ui.add_space(12.0);

        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Aktivitäten").size(19.0).strong())
                .on_hover_text("Deine häufigsten Aktivitäten nach Fokuszeit.");
            ui.add_space(10.0);
            activity_ranking(ui, &stats::by_activity(&active), summary.total_minutes);
        });
        ui.add_space(12.0);

        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Auswertung").size(19.0).strong());
                ui.selectable_value(&mut self.preferences.weekly, false, "Tage");
                ui.selectable_value(&mut self.preferences.weekly, true, "ISO-Wochen");
            });
            ui.add_space(14.0);
            egui::Grid::new("totals").num_columns(3).striped(true).spacing([36.0, 10.0]).show(ui, |ui| {
                for label in ["Zeitraum", "Aktivität", "Fokuszeit"] { ui.label(RichText::new(label).strong().color(subdued())); } ui.end_row();
                for ((period, activity), minutes) in history::totals(&entries, self.preferences.weekly).iter().rev() {
                    ui.label(period); ui.label(activity); ui.label(format_minutes(*minutes)); ui.end_row();
                }
            });
            ui.add_space(12.0); muted(ui, "Papierkorb und ungelöste Konflikte zählen nicht mit. Es gilt das aufgezeichnete Sitzungsdatum.");
        });
    }
    fn settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Deine Daten").size(19.0).strong()); ui.add_space(12.0);
            muted(ui, "Die App speichert jede Sitzung zuerst auf diesem Gerät.");
            ui.label(self.store.path.display().to_string()); ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(self.syncing.is_none(), egui::Button::new("Python-Datenbank importieren")).clicked() { self.dialog = Some(Dialog::Import { path: String::new(), error: None }); }
                if ui.button("CSV exportieren").clicked() { self.export_dialog(); }
                if ui.button("Datenbank prüfen").clicked() {
                    match self.store.check() { Ok(()) => self.notify("Datenbankprüfung erfolgreich.", false), Err(e) => self.notify(e.to_string(), true) }
                }
            });
            ui.add_space(8.0); muted(ui, "Ein CSV-Export enthält aktive Sitzungen nach den Verlaufsfiltern. Er ist keine vollständige Datensicherung.");
        });
        ui.add_space(18.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Darstellung").size(19.0).strong());
            ui.add_space(12.0);
            muted(ui, "Wähle das Farbschema der App.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.preferences.theme, Theme::Light, "Hell");
                ui.selectable_value(&mut self.preferences.theme, Theme::Dark, "Dunkel");
            });
            ui.add_space(8.0);
            muted(
                ui,
                "Die Auswahl wird gespeichert und gilt beim nächsten Start weiter.",
            );
        });
        ui.add_space(18.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Zwischen Geräten synchronisieren").size(19.0).strong()); ui.add_space(12.0);
            muted(ui, "Synchronisierung über deine Turso-Datenbank. Aktive Timer bleiben auf diesem Gerät.");
            ui.add_space(12.0);
            match &self.configured {
                Ok(remote) => {
                    ui.label(&remote.url);
                    let enabled = self.remote.is_some();
                    if ui.add_enabled(self.syncing.is_none(), egui::Button::new(if enabled { "Cloud-Synchronisierung ausschalten" } else { "Cloud-Synchronisierung einschalten" })).clicked() {
                        if enabled { self.remote = None; self.sync_error = None; self.sync_again = false; }
                        else { self.remote = Some(remote.clone()); self.start_sync(ctx); }
                    }
                }
                Err(message) => { ui.colored_label(subdued(), message); muted(ui, "TURSO_DATABASE_URL und TURSO_AUTH_TOKEN in .env setzen und die App neu starten. Mit --env-file lässt sich eine andere Datei wählen."); }
            }
            if let Some(error) = &self.sync_error { ui.add_space(12.0); ui.colored_label(danger(), error); }
            ui.add_space(12.0);
            muted(ui, "Bei aktiver Verbindung wird nach Änderungen und alle 15 Sekunden synchronisiert. Bei Verbindungsfehlern bleiben deine Änderungen lokal gespeichert.");
            muted(ui, "Eine vorhandene Python-Cloud-Datenbank muss auf Schema v2 migriert sein. Verwende auf jedem Gerät eine eigene Desktop-Datenbank.");
        });
        ui.add_space(18.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Tastenkürzel").size(19.0).strong()); ui.add_space(12.0);
            for (keys, action) in [("F2", "Timer pausieren / fortsetzen"), ("Strg+N", "Sitzung ergänzen"), ("Strg+E", "Ausgewählte Sitzung bearbeiten"), ("Strg+D", "In den Papierkorb verschieben"), ("Strg+R", "Aus dem Papierkorb wiederherstellen"), ("Strg+S", "Jetzt synchronisieren"), ("F5", "Verlauf neu laden"), ("Strg+Q", "App beenden"), ("Escape", "Dialog abbrechen")] {
                ui.horizontal(|ui| { ui.add_sized([90.0, 23.0], egui::Label::new(RichText::new(keys).monospace())); ui.label(action); });
            }
            ui.add_space(10.0); muted(ui, "Timer-Einstellungen werden gespeichert. Ein laufender Timer wird nach einem Prozessabbruch nicht wiederhergestellt.");
        });
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let mut close = false;
        let response = egui::Modal::new(egui::Id::new("dialog"))
            .frame(card().inner_margin(24)).show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 90.0).clamp(300.0, 520.0));
                match &mut dialog {
                    Dialog::Editor(editor) => {
                        let conflicted = editor.entry.as_ref().is_some_and(Entry::conflicted);
                        ui.heading(if conflicted { "Konflikt auflösen" } else if editor.entry.is_some() { "Sitzung bearbeiten" } else { "Sitzung ergänzen" });
                        ui.add_space(12.0);
                        egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 210.0).max(180.0)).show(ui, |ui| {
                            if let Some(entry) = &editor.entry
                                && conflicted {
                                    muted(ui, "Alle Versionen bleiben erhalten. Wähle eine Ausgangsversion und prüfe die Felder.");
                                    let previous = editor.version;
                                    egui::ComboBox::from_id_salt("version").selected_text(format!("Version {} von {}", editor.version + 1, entry.heads.len())).show_ui(ui, |ui| {
                                        for (index, revision) in entry.heads.iter().enumerate() {
                                            ui.selectable_value(&mut editor.version, index, format!("{} · {} · {}", index + 1, short_timestamp(&revision.created_at), if revision.deleted { "Papierkorb" } else { "Aktiv" }));
                                        }
                                    });
                                    if previous != editor.version {
                                        let revision = &entry.heads[editor.version];
                                        editor.session = revision.session.clone(); editor.minutes = revision.session.duration_minutes.to_string(); editor.deleted = revision.deleted;
                                    }
                                    let revision = &entry.heads[editor.version];
                                    muted(ui, &format!("Gerät: {}", revision.device_id));
                                    ui.checkbox(&mut editor.deleted, "Im Papierkorb behalten"); ui.separator();
                                }
                            field(ui, "Aktivität", &mut editor.session.activity);
                            field(ui, "Datum (JJJJ-MM-TT)", &mut editor.session.date);
                            field(ui, "Zeitstempel (ISO-Format, mit Zeitzone)", &mut editor.session.timestamp);
                            field(ui, "Dauer in Minuten", &mut editor.minutes);
                            ui.label("Notiz"); ui.add(egui::TextEdit::multiline(&mut editor.session.comment).desired_rows(3).desired_width(f32::INFINITY));
                            if let Some(error) = &editor.error { ui.colored_label(danger(), error); }
                        });
                        ui.add_space(16.0); ui.horizontal(|ui| {
                            if primary(ui, if conflicted { "Konflikt auflösen" } else { "Speichern" }, 155.0).clicked() {
                                match editor.save(&mut self.store) {
                                    Ok(()) => { self.saved("Sitzung lokal gespeichert.", ctx); close = true; }
                                    Err(e) => editor.error = Some(e.to_string()),
                                }
                            }
                            if ui.button("Abbrechen").clicked() { close = true; }
                        });
                    }
                    Dialog::Export { path, entries, error } => {
                        ui.heading("CSV exportieren"); ui.add_space(12.0);
                        muted(ui, "Exportiert aktive Sitzungen nach den aktuellen Verlaufsfiltern.");
                        field(ui, "Zieldatei", path);
                        if let Some(error) = error.as_ref() { ui.colored_label(danger(), error); }
                        ui.add_space(12.0); ui.horizontal(|ui| {
                            if primary(ui, "Exportieren", 140.0).clicked() {
                                match history::export_csv(entries, &expand_home(path)) {
                                    Ok(count) => { self.notify(format!("{count} Sitzungen exportiert: {path}"), false); close = true; }
                                    Err(e) => *error = Some(format!("Export fehlgeschlagen: {e}. Bestehende Dateien werden nicht überschrieben.")),
                                }
                            }
                            if ui.button("Abbrechen").clicked() { close = true; }
                        });
                    }
                    Dialog::Import { path, error } => {
                        ui.heading("Python-Verlauf übernehmen"); ui.add_space(12.0);
                        muted(ui, "Importiert eine normale SQLite-Datei (altes Sitzungsformat oder v2). Die Quelle wird nur gelesen. Wiederholte Importe derselben Quelle erzeugen keine Duplikate.");
                        muted(ui, "Turso-Replikate zuerst im Python-Client synchronisieren und anschließend hier die Cloud-Synchronisierung aktivieren.");
                        field(ui, "Quelldatenbank", path);
                        if let Some(error) = error.as_ref() { ui.colored_label(danger(), error); }
                        ui.add_space(12.0); ui.horizontal(|ui| {
                            if primary(ui, "Importieren", 140.0).clicked() {
                                match self.store.import(&expand_home(path)) {
                                    Ok(count) => { self.saved(&format!("{count} Revisionen importiert. Quelldatei unverändert."), ctx); close = true; }
                                    Err(e) => *error = Some(e.to_string()),
                                }
                            }
                            if ui.button("Abbrechen").clicked() { close = true; }
                        });
                    }
                    Dialog::Delete(entry) => {
                        ui.heading("In den Papierkorb verschieben?"); ui.add_space(12.0);
                        ui.label(&entry.heads[0].session.activity);
                        muted(ui, "Die Sitzung wird auf verbundenen Geräten ebenfalls gelöscht. Du kannst sie im Papierkorb wiederherstellen.");
                        ui.add_space(16.0); ui.horizontal(|ui| {
                            if primary(ui, "In Papierkorb", 145.0).clicked() {
                                match self.store.set_deleted(&entry.session_id, &entry.head_ids(), true) {
                                    Ok(()) => self.saved("Sitzung in den Papierkorb verschoben.", ctx), Err(e) => self.notify(e.to_string(), true),
                                }
                                close = true;
                            }
                            if ui.button("Abbrechen").clicked() { close = true; }
                        });
                    }
                    Dialog::Discard => {
                        ui.heading("Aktuelle Fokuszeit verwerfen?"); ui.add_space(12.0);
                        muted(ui, "Die laufende Zeit wird beendet und nicht gespeichert. Vorher abgeschlossene Zyklen bleiben erhalten.");
                        ui.add_space(16.0); ui.horizontal(|ui| {
                            if ui.button("Zeit verwerfen").clicked() { self.run = None; self.save_failed = false; close = true; }
                            if primary(ui, "Weiterarbeiten", 150.0).clicked() { close = true; }
                        });
                    }
                    Dialog::Quit => {
                        ui.heading("Dein Timer ist noch aktiv"); ui.add_space(12.0);
                        muted(ui, "Was soll mit der aktuellen Fokuszeit geschehen? Pausenzeiten werden nicht gespeichert.");
                        ui.add_space(16.0); ui.horizontal_wrapped(|ui| {
                            if primary(ui, "Speichern & schließen", 185.0).clicked() && self.finish_action(false, ctx, Instant::now(), true) {
                                self.allow_close = true; ctx.send_viewport_cmd(egui::ViewportCommand::Close); close = true;
                            }
                            if ui.button("Verwerfen & schließen").clicked() {
                                self.run = None; self.allow_close = true; ctx.send_viewport_cmd(egui::ViewportCommand::Close); close = true;
                            }
                            if ui.button("Abbrechen").clicked() { close = true; }
                        });
                        if self.save_failed { ui.colored_label(danger(), "Speichern fehlgeschlagen. Schließe diesen Dialog für Details oder versuche es erneut."); }
                    }
                }
            });
        if !close && !response.should_close() {
            self.dialog = Some(dialog);
        }
    }
    fn capture(&mut self, ctx: &egui::Context) {
        self.frames += 1;
        let Some(path) = &self.screenshot else { return };
        if self.frames > 6 && !self.screenshot_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.screenshot_requested = true;
        }
        for event in ctx.input(|i| i.events.clone()) {
            if let egui::Event::Screenshot { image, .. } = event {
                let rgba: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                if let Err(e) = image::save_buffer(
                    path,
                    &rgba,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                ) {
                    eprintln!("Bild konnte nicht gespeichert werden: {e}");
                }
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

impl Aftermath {
    fn render(&mut self, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_millis(200));
        let theme = self.preferences.theme;
        if theme != self.applied_theme {
            self.applied_theme = theme;
            configure_style(ctx, Palette::of(theme));
        }
        set_palette(Palette::of(theme));
        self.tick(ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.run.is_some()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_close(ctx);
        }
        self.shortcuts(ctx);
        let previous_click = if ctx.input(|i| i.pointer.primary_clicked()) {
            self.last_row_click.take()
        } else {
            None
        };
        self.tabs(ctx);
        self.footer(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(background()).inner_margin(14))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("page-scroll", self.page as u8))
                    .show(ui, |ui| {
                        if self.page != Page::Focus {
                            ui.label(RichText::new(self.page.title()).size(23.0).strong());
                            let subtitle = self.page.subtitle();
                            if !subtitle.is_empty() {
                                muted(ui, subtitle);
                            }
                        }
                        if let Some((notice, error)) = &self.notice {
                            let mut dismiss = false;
                            egui::Frame::new()
                                .fill(if *error { notice_error_bg() } else { tint() })
                                .corner_radius(8)
                                .inner_margin(8)
                                .show(ui, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.colored_label(
                                            if *error { danger() } else { accent() },
                                            notice,
                                        );
                                        if ui.small_button("×").clicked() {
                                            dismiss = true;
                                        }
                                    });
                                });
                            if dismiss {
                                self.notice = None;
                            }
                        }
                        match self.page {
                            Page::Focus => {
                                self.timer_card(ui, ctx);
                                ui.add_space(10.0);
                                self.history(ui, ctx, previous_click);
                            }
                            Page::Stats => self.stats(ui),
                            Page::Settings => self.settings(ui, ctx),
                            _ => self.history(ui, ctx, previous_click),
                        }
                    });
            });
        self.dialogs(ctx);
        self.resync_deadline();
        self.capture(ctx);
    }
}

impl eframe::App for Aftermath {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render(ctx);
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "snapo-preferences", &self.preferences);
    }
}

fn configure_style(ctx: &egui::Context, palette: Palette) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "adwaita".into(),
        egui::FontData::from_static(include_bytes!("../assets/AdwaitaSans.ttf")).into(),
    );
    fonts.font_data.insert(
        "adwaita-mono".into(),
        egui::FontData::from_static(include_bytes!("../assets/AdwaitaMono.ttf")).into(),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "adwaita".into());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "adwaita-mono".into());
    ctx.set_fonts(fonts);
    ctx.set_visuals(if palette.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    });
    let mut style = (*ctx.style()).clone();
    style.visuals.override_text_color = Some(palette.ink);
    style.visuals.panel_fill = palette.background;
    style.visuals.window_fill = palette.surface;
    style.visuals.faint_bg_color = palette.background;
    style.visuals.extreme_bg_color = palette.surface;
    style.visuals.selection.bg_fill = palette.tint;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, palette.accent);
    style.visuals.widgets.inactive.bg_fill = palette.background;
    style.visuals.widgets.inactive.weak_bg_fill = palette.background;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, palette.line);
    style.visuals.widgets.hovered.bg_fill = palette.tint;
    style.visuals.widgets.hovered.weak_bg_fill = palette.tint;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, palette.accent);
    style.visuals.widgets.active.bg_fill = palette.tint;
    style.visuals.widgets.active.weak_bg_fill = palette.tint;
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 6.0);
    style.spacing.interact_size.y = 29.0;
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(14.0));
    ctx.set_style(style);
}
fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(surface())
        .stroke(Stroke::new(1.0_f32, hairline()))
        .corner_radius(12)
        .inner_margin(14)
}
fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(subdued()).size(13.0));
}
fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.add_space(8.0);
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
}
fn primary(ui: &mut egui::Ui, text: &str, width: f32) -> egui::Response {
    let foreground = if palette().dark {
        background()
    } else {
        Color32::WHITE
    };
    ui.add(
        egui::Button::new(RichText::new(text).color(foreground).strong())
            .min_size(Vec2::new(width, 29.0))
            .fill(accent())
            .corner_radius(7),
    )
}
fn nav(ui: &mut egui::Ui, text: &str, active: bool) -> bool {
    ui.add(
        egui::Button::new(
            RichText::new(text)
                .color(if active { accent() } else { subdued() })
                .strong(),
        )
        .fill(if active { tint() } else { Color32::TRANSPARENT })
        .stroke(Stroke::NONE)
        .corner_radius(7),
    )
    .clicked()
}
fn history_cells(ui: &mut egui::Ui, row: egui::Rect, cells: [RichText; 5]) -> [egui::Response; 5] {
    let flexible = (row.width() - 310.0).max(0.0);
    let mut widths = [134.0, flexible * 0.45, 76.0, flexible * 0.55, 100.0].into_iter();
    let mut left = row.left();
    cells.map(|text| {
        let right = left + widths.next().expect("column width");
        let cell = egui::Rect::from_min_max(
            egui::pos2(left + 8.0, row.top()),
            egui::pos2(right, row.bottom()),
        );
        left = right;
        ui.new_child(
            egui::UiBuilder::new()
                .max_rect(cell)
                .layout(Layout::left_to_right(Align::Center)),
        )
        .add(egui::Label::new(text).truncate().selectable(false))
    })
}
fn daily_focus_chart(
    ui: &mut egui::Ui,
    sessions: &[&Session],
    include_calendar_days: bool,
    filter: &Filter,
) {
    let mut days: BTreeMap<String, BTreeMap<&str, Vec<&Session>>> = BTreeMap::new();
    for session in sessions {
        days.entry(session.date.clone())
            .or_default()
            .entry(&session.activity)
            .or_default()
            .push(session);
    }
    if include_calendar_days && filter.validate().is_ok() {
        let recorded_dates: Vec<_> = days
            .keys()
            .filter_map(|day| NaiveDate::parse_from_str(day, "%Y-%m-%d").ok())
            .collect();
        let start = NaiveDate::parse_from_str(&filter.since, "%Y-%m-%d")
            .ok()
            .or_else(|| recorded_dates.iter().min().copied());
        let end = NaiveDate::parse_from_str(&filter.until, "%Y-%m-%d")
            .ok()
            .or_else(|| recorded_dates.iter().max().copied());
        if let (Some(mut day), Some(end)) = (start, end) {
            while day <= end {
                days.entry(day.to_string()).or_default();
                let Some(next) = day.succ_opt() else { break };
                day = next;
            }
        }
    }
    if days.is_empty() {
        muted(
            ui,
            "Noch keine Fokuszeit für diesen Zeitraum. Gespeicherte Sitzungen erscheinen hier.",
        );
        return;
    }
    let maximum = days
        .values()
        .map(|activities| {
            activities
                .values()
                .flatten()
                .map(|s| s.duration_minutes)
                .sum::<f64>()
        })
        .fold(1.0_f64, f64::max);
    egui::ScrollArea::horizontal()
        .id_salt("daily-chart-scroll")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let width = ui.available_width().max(60.0 + days.len() as f32 * 62.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 220.0), Sense::hover());
            let plot = egui::Rect::from_min_max(
                rect.min + Vec2::new(48.0, 28.0),
                rect.max - Vec2::new(12.0, 40.0),
            );
            let painter = ui.painter().clone();
            painter.text(
                rect.min,
                Align2::LEFT_TOP,
                "Fokuszeit (Stunden)",
                FontId::proportional(12.0),
                subdued(),
            );
            for tick in 0..=4 {
                let y = plot.bottom() - plot.height() * tick as f32 / 4.0;
                painter.line_segment(
                    [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
                    Stroke::new(1.0_f32, hairline()),
                );
                painter.text(
                    egui::pos2(plot.left() - 8.0, y),
                    Align2::RIGHT_CENTER,
                    axis_hours(maximum * tick as f64 / 4.0),
                    FontId::proportional(12.0),
                    subdued(),
                );
            }
            painter.line_segment(
                [plot.left_top(), plot.left_bottom()],
                Stroke::new(1.0_f32, subdued()),
            );
            painter.line_segment(
                [plot.left_bottom(), plot.right_bottom()],
                Stroke::new(1.0_f32, subdued()),
            );
            let slot_width = plot.width() / days.len() as f32;
            let bar_width = (slot_width * 0.6).min(40.0);
            for (day_index, (day, activities)) in days.iter().enumerate() {
                let x = plot.left() + slot_width * (day_index as f32 + 0.5);
                let mut bottom = plot.bottom();
                let mut total = 0.0;
                for (activity, sessions) in activities {
                    for (index, session) in sessions.iter().enumerate() {
                        let height = (session.duration_minutes / maximum) as f32 * plot.height();
                        let segment = egui::Rect::from_min_size(
                            egui::pos2(x - bar_width / 2.0, bottom - height),
                            Vec2::new(bar_width, height),
                        );
                        painter.rect_filled(segment, 2.0, activity_color(activity));
                        painter.line_segment(
                            [segment.left_top(), segment.right_top()],
                            Stroke::new(1.0_f32, surface()),
                        );
                        ui.interact(
                            segment,
                            ui.id().with((day, activity, index)),
                            Sense::hover(),
                        )
                        .on_hover_text(format!(
                            "{} · {}\n{}\n{}",
                            session.activity,
                            format_minutes(session.duration_minutes),
                            session.timestamp,
                            session.comment
                        ));
                        bottom -= height;
                        total += session.duration_minutes;
                    }
                }
                if activities.is_empty() {
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(x - bar_width / 2.0, plot.bottom() - 3.0),
                            Vec2::new(bar_width, 3.0),
                        ),
                        1.0,
                        hairline(),
                    );
                } else if slot_width >= 80.0 {
                    painter.text(
                        egui::pos2(x, bottom - 4.0),
                        Align2::CENTER_BOTTOM,
                        format_minutes(total),
                        FontId::proportional(11.0),
                        ink(),
                    );
                }
                let label = NaiveDate::parse_from_str(day, "%Y-%m-%d")
                    .map(|date| date.format("%d.%m.").to_string())
                    .unwrap_or_else(|_| day.clone());
                painter.text(
                    egui::pos2(x, plot.bottom() + 6.0),
                    Align2::CENTER_TOP,
                    label,
                    FontId::proportional(11.0),
                    subdued(),
                );
                ui.interact(
                    egui::Rect::from_min_size(
                        egui::pos2(x - slot_width / 2.0, plot.bottom() - 3.0),
                        Vec2::new(slot_width, 24.0),
                    ),
                    ui.id().with((day, "date")),
                    Sense::hover(),
                )
                .on_hover_text(format!("{day} · {}", format_minutes(total)));
            }
            painter.text(
                egui::pos2(plot.center().x, rect.bottom() - 4.0),
                Align2::CENTER_BOTTOM,
                "Datum (Tag)",
                FontId::proportional(11.0),
                subdued(),
            );
        });
    ui.add_space(4.0);
    let activities: std::collections::BTreeSet<_> = sessions.iter().map(|s| &s.activity).collect();
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Aktivitäten:").color(subdued()));
        for activity in activities {
            ui.horizontal(|ui| {
                ui.colored_label(activity_color(activity), "●");
                ui.label(activity);
            });
            ui.add_space(6.0);
        }
    });
}

fn activity_color(activity: &str) -> Color32 {
    let palette = palette().activity;
    let hash = activity.bytes().fold(0_usize, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(byte as usize)
    });
    palette[hash % palette.len()]
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez",
];

fn short_date(date: &str) -> String {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|d| d.format("%d.%m.%Y").to_string())
        .unwrap_or_else(|_| date.to_string())
}

fn kpi(ui: &mut egui::Ui, value: &str, label: &str) {
    egui::Frame::new()
        .fill(tint())
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_min_width(96.0);
            ui.label(RichText::new(value).size(19.0).strong());
            ui.label(RichText::new(label).size(12.0).color(subdued()));
        });
}

/// Stacked bars, one column per period, split into activity segments.
fn stacked_bar_chart(
    ui: &mut egui::Ui,
    y_title: &str,
    x_title: &str,
    bars: &[(String, BTreeMap<String, f64>)],
    height: f32,
    min_slot: f32,
) {
    if bars.iter().all(|(_, activities)| activities.is_empty()) {
        muted(ui, "In diesem Zeitraum gibt es noch keine Fokuszeit.");
        return;
    }
    let maximum = bars
        .iter()
        .map(|(_, activities)| activities.values().sum::<f64>())
        .fold(1.0_f64, f64::max);
    egui::ScrollArea::horizontal()
        .id_salt(("stacked-bars", x_title))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let width = ui
                .available_width()
                .max(60.0 + bars.len() as f32 * min_slot);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
            let plot = egui::Rect::from_min_max(
                rect.min + Vec2::new(48.0, 28.0),
                rect.max - Vec2::new(12.0, 40.0),
            );
            let painter = ui.painter().clone();
            painter.text(
                rect.min,
                Align2::LEFT_TOP,
                y_title,
                FontId::proportional(12.0),
                subdued(),
            );
            for tick in 0..=4 {
                let y = plot.bottom() - plot.height() * tick as f32 / 4.0;
                painter.line_segment(
                    [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
                    Stroke::new(1.0_f32, hairline()),
                );
                painter.text(
                    egui::pos2(plot.left() - 8.0, y),
                    Align2::RIGHT_CENTER,
                    axis_hours(maximum * tick as f64 / 4.0),
                    FontId::proportional(12.0),
                    subdued(),
                );
            }
            painter.line_segment(
                [plot.left_top(), plot.left_bottom()],
                Stroke::new(1.0_f32, subdued()),
            );
            painter.line_segment(
                [plot.left_bottom(), plot.right_bottom()],
                Stroke::new(1.0_f32, subdued()),
            );
            let slot_width = plot.width() / bars.len() as f32;
            let bar_width = (slot_width * 0.6).min(48.0);
            for (index, (label, activities)) in bars.iter().enumerate() {
                let x = plot.left() + slot_width * (index as f32 + 0.5);
                let mut bottom = plot.bottom();
                let total: f64 = activities.values().sum();
                for (activity, minutes) in activities {
                    let bar_height = (*minutes / maximum) as f32 * plot.height();
                    let segment = egui::Rect::from_min_size(
                        egui::pos2(x - bar_width / 2.0, bottom - bar_height),
                        Vec2::new(bar_width, bar_height),
                    );
                    painter.rect_filled(segment, 2.0, activity_color(activity));
                    ui.interact(segment, ui.id().with((label, activity)), Sense::hover())
                        .on_hover_text(format!("{activity} · {}", format_minutes(*minutes)));
                    bottom -= bar_height;
                }
                if total == 0.0 {
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(x - bar_width / 2.0, plot.bottom() - 3.0),
                            Vec2::new(bar_width, 3.0),
                        ),
                        1.0,
                        hairline(),
                    );
                } else if slot_width >= 56.0 {
                    painter.text(
                        egui::pos2(x, bottom - 4.0),
                        Align2::CENTER_BOTTOM,
                        format_minutes(total),
                        FontId::proportional(11.0),
                        ink(),
                    );
                }
                painter.text(
                    egui::pos2(x, plot.bottom() + 6.0),
                    Align2::CENTER_TOP,
                    label,
                    FontId::proportional(11.0),
                    subdued(),
                );
            }
            painter.text(
                egui::pos2(plot.center().x, rect.bottom() - 4.0),
                Align2::CENTER_BOTTOM,
                x_title,
                FontId::proportional(11.0),
                subdued(),
            );
        });
}

/// Simple single-series bar chart (weekdays, hours of day).
fn simple_bar_chart(ui: &mut egui::Ui, y_title: &str, bars: &[(String, f64)], height: f32) {
    if bars.iter().all(|(_, value)| *value <= 0.0) {
        muted(ui, "Keine Daten für diesen Zeitraum.");
        return;
    }
    let maximum = bars.iter().map(|(_, value)| *value).fold(1.0_f64, f64::max);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let plot = egui::Rect::from_min_max(
        rect.min + Vec2::new(40.0, 24.0),
        rect.max - Vec2::new(10.0, 36.0),
    );
    let painter = ui.painter().clone();
    painter.text(
        rect.min,
        Align2::LEFT_TOP,
        y_title,
        FontId::proportional(12.0),
        subdued(),
    );
    for tick in 0..=4 {
        let y = plot.bottom() - plot.height() * tick as f32 / 4.0;
        painter.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            Stroke::new(1.0_f32, hairline()),
        );
        painter.text(
            egui::pos2(plot.left() - 6.0, y),
            Align2::RIGHT_CENTER,
            axis_hours(maximum * tick as f64 / 4.0),
            FontId::proportional(11.0),
            subdued(),
        );
    }
    painter.line_segment(
        [plot.left_bottom(), plot.right_bottom()],
        Stroke::new(1.0_f32, subdued()),
    );
    let slot = plot.width() / bars.len() as f32;
    let bar_width = (slot * 0.6).min(36.0);
    for (index, (label, value)) in bars.iter().enumerate() {
        let x = plot.left() + slot * (index as f32 + 0.5);
        let bar_height = (*value / maximum) as f32 * plot.height();
        let segment = egui::Rect::from_min_size(
            egui::pos2(x - bar_width / 2.0, plot.bottom() - bar_height),
            Vec2::new(bar_width, bar_height),
        );
        painter.rect_filled(segment, 2.0, accent());
        ui.interact(segment, ui.id().with(("simple", label)), Sense::hover())
            .on_hover_text(format!("{label} · {}", format_minutes(*value)));
        painter.text(
            egui::pos2(x, plot.bottom() + 6.0),
            Align2::CENTER_TOP,
            label,
            FontId::proportional(11.0),
            subdued(),
        );
    }
}

fn weekday_chart(ui: &mut egui::Ui, active: &[&Session]) {
    let weekdays = stats::by_weekday(active);
    let bars: Vec<(String, f64)> = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"]
        .iter()
        .enumerate()
        .map(|(index, label)| (label.to_string(), weekdays[index]))
        .collect();
    simple_bar_chart(ui, "Fokuszeit (Stunden)", &bars, 150.0);
}

fn hour_chart(ui: &mut egui::Ui, active: &[&Session]) {
    let hours = stats::by_hour(active);
    let bars: Vec<(String, f64)> = (0..24)
        .map(|hour| (hour.to_string(), hours[hour as usize]))
        .collect();
    simple_bar_chart(ui, "Fokuszeit (Stunden)", &bars, 150.0);
}

fn activity_ranking(ui: &mut egui::Ui, ranked: &[(String, f64)], total: f64) {
    if ranked.is_empty() {
        muted(ui, "Noch keine Aktivitäten.");
        return;
    }
    for (activity, minutes) in ranked.iter().take(8) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.colored_label(activity_color(activity), "●");
            ui.label(activity);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let share = if total > 0.0 {
                    minutes / total * 100.0
                } else {
                    0.0
                };
                ui.label(
                    RichText::new(format!("{} · {share:.0} %", format_minutes(*minutes)))
                        .color(subdued()),
                );
            });
        });
        let share = if total > 0.0 { *minutes / total } else { 0.0 };
        let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 8.0), Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(bar, 4.0, tint());
        painter.rect_filled(
            egui::Rect::from_min_size(bar.min, Vec2::new(bar.width() * share as f32, 8.0)),
            4.0,
            activity_color(activity),
        );
        ui.add_space(6.0);
    }
}

fn clock_text(duration: Duration, countdown: bool) -> String {
    let seconds = if countdown {
        duration.as_secs_f64().ceil() as u64
    } else {
        duration.as_secs()
    };
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}
fn format_minutes(minutes: f64) -> String {
    if minutes <= 0.0 {
        return "0 min".into();
    }
    if minutes >= 60.0 {
        format!("{} h {:02} min", minutes as u64 / 60, minutes as u64 % 60)
    } else if minutes >= 1.0 {
        format!("{minutes:.1} min")
    } else {
        format!("{:.0} s", minutes * 60.0)
    }
}
/// Axis tick label in hours: whole hours without decimals, otherwise one decimal.
fn axis_hours(minutes: f64) -> String {
    let hours = minutes / 60.0;
    if (hours - hours.round()).abs() < 0.05 {
        format!("{:.0}", hours)
    } else {
        format!("{hours:.1}")
    }
}
fn short_timestamp(timestamp: &str) -> String {
    timestamp
        .chars()
        .take(16)
        .collect::<String>()
        .replace('T', "  ")
}
fn expand_home(path: &str) -> PathBuf {
    if let Some(suffix) = path.strip_prefix("~/")
        && let Some(dirs) = directories::UserDirs::new()
    {
        return dirs.home_dir().join(suffix);
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn app() -> (TempDir, Aftermath, egui::Context) {
        app_with(DeadlineWatcher::with_bell(|| {}))
    }
    fn app_with(watcher: DeadlineWatcher) -> (TempDir, Aftermath, egui::Context) {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("ui.db")).unwrap();
        let ctx = egui::Context::default();
        configure_style(&ctx, Palette::of(Theme::Light));
        let app = Aftermath::workspace(
            store,
            None,
            Err("Keine Cloud konfiguriert".into()),
            Preferences::default(),
            watcher,
        );
        (dir, app, ctx)
    }
    fn frame(
        app: &mut Aftermath,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        size: Vec2,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    }
    fn text_position(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.size() / 2.0)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| text_position(s, label)),
            _ => None,
        }
    }
    fn click(app: &mut Aftermath, ctx: &egui::Context, label: &str) {
        click_at_size(app, ctx, label, Vec2::new(1180.0, 820.0));
    }
    fn click_at_size(app: &mut Aftermath, ctx: &egui::Context, label: &str, size: Vec2) {
        for _ in 0..3 {
            frame(app, ctx, vec![], size);
        }
        let output = frame(app, ctx, vec![], size);
        let pos = output
            .shapes
            .iter()
            .rev()
            .find_map(|s| text_position(&s.shape, label).filter(|pos| s.clip_rect.contains(*pos)))
            .unwrap_or_else(|| panic!("Visible control not found: {label}"));
        frame(app, ctx, vec![egui::Event::PointerMoved(pos)], size);
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
                size,
            );
        }
    }
    fn key(app: &mut Aftermath, ctx: &egui::Context, key: egui::Key, modifiers: egui::Modifiers) {
        frame(
            app,
            ctx,
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            Vec2::new(1180.0, 820.0),
        );
        frame(
            app,
            ctx,
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers,
            }],
            Vec2::new(1180.0, 820.0),
        );
    }

    #[test]
    fn ui_create_select_edit_trash_restore_and_export() {
        let (dir, mut app, ctx) = app();
        key(&mut app, &ctx, egui::Key::N, egui::Modifiers::CTRL);
        let Some(Dialog::Editor(editor)) = &mut app.dialog else {
            panic!("Editor missing")
        };
        editor.session.activity = "UI Lernen".into();
        editor.minutes = "42,5".into();
        editor.session.comment = "Kapitel 2".into();
        click(&mut app, &ctx, "Speichern");
        assert!(app.dialog.is_none());
        assert_eq!(app.store.entries().unwrap().len(), 1);
        click(&mut app, &ctx, "UI Lernen");
        click(&mut app, &ctx, "Bearbeiten");
        let Some(Dialog::Editor(editor)) = &mut app.dialog else {
            panic!("Editor missing")
        };
        editor.session.activity = "UI Rust".into();
        click(&mut app, &ctx, "Speichern");
        assert_eq!(
            app.store.entries().unwrap()[0].active().unwrap().activity,
            "UI Rust"
        );
        click(&mut app, &ctx, "In Papierkorb");
        click(&mut app, &ctx, "In Papierkorb");
        assert!(app.store.entries().unwrap()[0].current().unwrap().deleted);
        click(&mut app, &ctx, "Papierkorb");
        click(&mut app, &ctx, "UI Rust");
        click(&mut app, &ctx, "Wiederherstellen");
        assert!(app.store.entries().unwrap()[0].active().is_some());
        click(&mut app, &ctx, "Fokus & Verlauf");
        click(&mut app, &ctx, "CSV exportieren");
        let target = dir.path().join("ui-export.csv");
        let Some(Dialog::Export { path, .. }) = &mut app.dialog else {
            panic!("Export missing")
        };
        *path = target.display().to_string();
        click(&mut app, &ctx, "Exportieren");
        assert!(std::fs::read_to_string(target).unwrap().contains("UI Rust"));
    }

    #[test]
    fn compact_focus_tab_shows_timer_and_editable_history_together() {
        let (_dir, mut app, ctx) = app();
        app.store
            .create(Session::now("Lesen", 15.0, "Kapitel 2"), None)
            .unwrap();
        app.reload();
        let size = Vec2::new(626.0, 520.0);
        let output = frame(&mut app, &ctx, vec![], size);
        for label in ["Fokus starten", "Lesen"] {
            assert!(
                output
                    .shapes
                    .iter()
                    .any(|shape| text_position(&shape.shape, label)
                        .is_some_and(|pos| shape.clip_rect.contains(pos))),
                "{label} fehlt"
            );
        }
        click_at_size(&mut app, &ctx, "Filter", size);
        assert!(app.show_filters);
        click_at_size(&mut app, &ctx, "Kapitel 2", size);
        assert!(app.selected.is_some());
        assert!(app.dialog.is_none());
        click_at_size(&mut app, &ctx, "Kapitel 2", size);
        assert!(matches!(app.dialog, Some(Dialog::Editor(_))));
    }

    #[test]
    fn ui_command_line_defines_activity_and_starts_timer() {
        let (_dir, mut app, ctx) = app();
        app.command = "Lernen -w 30 -b 5 -lb 20 -c 6 -t \"Vokabeln\"".into();
        click(&mut app, &ctx, "Command-Line");
        click(&mut app, &ctx, "Starten");
        let run = app.run.as_ref().expect("Timer über Command-Line gestartet");
        assert_eq!(run.clock.session.activity, "Lernen");
        assert_eq!(run.clock.session.comment, "Vokabeln");
        assert_eq!(run.plan.work, 30);
        assert_eq!(run.plan.short_break, 5);
        assert_eq!(run.plan.long_break, 20);
        assert_eq!(run.plan.cycles, 6);
        assert_eq!(app.preferences.activity, "Lernen");
        assert!(app.command.is_empty());
        app.run = None;
        app.command = "Lesen".into();
        click(&mut app, &ctx, "Starten");
        let run = app.run.as_ref().expect("Aktivität ohne Optionen startet");
        assert_eq!(run.clock.session.activity, "Lesen");
        assert_eq!(run.plan.work, 30, "Der Rhythmus bleibt bestehen");
        // Eine leere Zeile startet mit dem zuletzt definierten Rhythmus erneut.
        app.run = None;
        app.command.clear();
        click(&mut app, &ctx, "Starten");
        let run = app
            .run
            .as_ref()
            .expect("Leere Zeile nutzt die letzten Werte");
        assert_eq!(run.clock.session.activity, "Lesen");
        assert!(app.command.is_empty());
    }

    #[test]
    fn ui_command_line_reports_parse_errors_without_starting() {
        let (_dir, mut app, ctx) = app();
        app.command = "Lernen -x 5".into();
        click(&mut app, &ctx, "Command-Line");
        click(&mut app, &ctx, "Starten");
        assert!(app.run.is_none());
        let (text, error) = app.notice.as_ref().expect("Fehlermeldung erwartet");
        assert!(error);
        assert!(text.contains("Unbekannte Option '-x'"), "{text}");
        assert_eq!(app.command, "Lernen -x 5", "Eingabe bleibt korrigierbar");
    }

    #[test]
    fn ui_stale_form_displays_error_and_keeps_user_input() {
        let (_dir, mut app, ctx) = app();
        app.store
            .create(Session::now("Original", 20.0, ""), None)
            .unwrap();
        app.reload();
        let entry = app.entries[0].clone();
        app.dialog = Some(Dialog::Editor(Box::new(Editor::new(Some(entry.clone())))));
        app.store
            .change(
                &entry.session_id,
                &entry.head_ids(),
                Session::now("Extern", 30.0, ""),
                false,
                false,
            )
            .unwrap();
        click(&mut app, &ctx, "Speichern");
        assert!(
            matches!(&app.dialog, Some(Dialog::Editor(editor)) if editor.error.as_deref().unwrap().contains("inzwischen geändert") && editor.session.activity == "Original")
        );
        assert_eq!(
            app.store.entries().unwrap()[0].active().unwrap().activity,
            "Extern"
        );
    }

    #[test]
    fn hidden_workspace_completion_rings_on_time_and_catches_up_without_second_bell() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::thread;

        let rings = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&rings);
        let (_dir, mut app, ctx) = app_with(DeadlineWatcher::with_bell(move || {
            counted.fetch_add(1, Ordering::Relaxed);
        }));
        app.preferences.activity = "Versteckt".into();
        app.preferences.plan.work = 1;
        click(&mut app, &ctx, "Fokus starten");
        // The last frame before the window disappears to another workspace.
        app.run.as_mut().unwrap().clock = timer::Clock::new(
            "Versteckt".into(),
            "".into(),
            Some(1),
            Instant::now() - Duration::from_millis(59_700),
        )
        .unwrap();
        frame(&mut app, &ctx, vec![], Vec2::new(1180.0, 820.0));
        // Hidden: no frame callbacks arrive, so the tick loop is stalled and
        // only the background watcher can ring.
        for _ in 0..250 {
            if rings.load(Ordering::Relaxed) == 1 {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            rings.load(Ordering::Relaxed),
            1,
            "Die Glocke klingelt pünktlich ohne Frame-Callbacks"
        );
        // The user returns: tick catches up without a second bell and saves.
        app.tick(&ctx);
        assert_eq!(rings.load(Ordering::Relaxed), 1, "Kein zweites Läuten");
        assert_eq!(app.store.revisions().unwrap().len(), 1);
        assert_eq!(app.run.as_ref().unwrap().phase, Phase::ShortBreak);
    }

    #[test]
    fn ui_pause_close_cancel_and_save_once() {
        let (_dir, mut app, ctx) = app();
        app.preferences.activity = "Timer UI".into();
        click(&mut app, &ctx, "Fokus starten");
        assert!(app.run.is_some());
        key(&mut app, &ctx, egui::Key::F2, egui::Modifiers::NONE);
        assert!(app.run.as_ref().unwrap().clock.paused);
        key(&mut app, &ctx, egui::Key::Q, egui::Modifiers::CTRL);
        assert!(matches!(app.dialog, Some(Dialog::Quit)));
        click(&mut app, &ctx, "Abbrechen");
        assert!(app.run.is_some());
        assert!(!app.allow_close);
        key(&mut app, &ctx, egui::Key::Q, egui::Modifiers::CTRL);
        click(&mut app, &ctx, "Speichern & schließen");
        assert!(app.allow_close);
        assert!(app.run.is_none());
        assert_eq!(app.store.revisions().unwrap().len(), 1);
    }

    #[test]
    fn ui_discard_requires_confirmation_and_saves_nothing() {
        let (_dir, mut app, ctx) = app();
        app.preferences.activity = "Verwerfen".into();
        click(&mut app, &ctx, "Fokus starten");
        click(&mut app, &ctx, "Verwerfen");
        assert!(app.run.is_some());
        click(&mut app, &ctx, "Zeit verwerfen");
        assert!(app.run.is_none());
        assert!(app.store.revisions().unwrap().is_empty());
    }

    #[test]
    fn ui_failed_save_is_retained_and_retry_succeeds_once() {
        let (_dir, mut app, ctx) = app();
        app.preferences.activity = "Speicherfehler".into();
        let connection = rusqlite::Connection::open(&app.store.path).unwrap();
        connection.execute_batch("CREATE TRIGGER fail_save BEFORE INSERT ON session_revisions BEGIN SELECT RAISE(ABORT, 'simulierter Speicherfehler'); END;").unwrap();
        click(&mut app, &ctx, "Fokus starten");
        click(&mut app, &ctx, "Speichern & beenden");
        assert!(app.save_failed);
        assert!(app.run.as_ref().unwrap().clock.paused);
        assert!(app.store.revisions().unwrap().is_empty());
        connection.execute_batch("DROP TRIGGER fail_save").unwrap();
        click(&mut app, &ctx, "Speichern wiederholen");
        assert!(app.run.is_none());
        assert_eq!(app.store.revisions().unwrap().len(), 1);
    }

    #[test]
    fn completed_cycles_save_only_work_and_stop_after_final_cycle() {
        let (_dir, mut app, ctx) = app();
        let past = Instant::now() - Duration::from_secs(61);
        let plan = Plan {
            work: 1,
            short_break: 1,
            cycles: 2,
            sound: false,
            ..Default::default()
        };
        app.run = Some(Run::new(plan, "Zyklen".into(), "".into(), past).unwrap());
        app.tick(&ctx);
        assert_eq!(app.store.revisions().unwrap().len(), 1);
        assert_eq!(app.run.as_ref().unwrap().phase, Phase::ShortBreak);
        app.run.as_mut().unwrap().clock =
            crate::timer::Clock::new("Zyklen".into(), "".into(), Some(1), past).unwrap();
        app.tick(&ctx);
        assert_eq!(app.store.revisions().unwrap().len(), 1);
        assert_eq!(app.run.as_ref().unwrap().phase, Phase::Work);
        app.run.as_mut().unwrap().clock =
            crate::timer::Clock::new("Zyklen".into(), "".into(), Some(1), past).unwrap();
        app.tick(&ctx);
        assert_eq!(app.store.revisions().unwrap().len(), 2);
        assert!(app.run.is_none());
    }

    #[test]
    fn stalled_phases_catch_up_at_their_exact_deadlines() {
        // The reported bug: while the window hid on another workspace the
        // work phase ended, but the break only started counting when the
        // GUI regained focus. Tick must advance by the real completion
        // instants, so the break already shows its elapsed background time.
        let (_dir, mut app, ctx) = app();
        let past = Instant::now() - Duration::from_secs(150);
        let plan = Plan {
            work: 1,
            short_break: 1,
            cycles: 4,
            sound: false,
            ..Default::default()
        };
        app.run = Some(Run::new(plan, "Versteckt".into(), "".into(), past).unwrap());
        app.tick(&ctx);
        let run = app.run.as_ref().unwrap();
        assert_eq!(app.store.revisions().unwrap().len(), 1);
        assert_eq!(
            run.phase,
            Phase::Work,
            "Fokus und Pause liefen im Hintergrund durch"
        );
        assert_eq!(run.cycle, 2);
        let remaining = run.clock.remaining(Instant::now()).unwrap();
        assert!(
            remaining > Duration::from_secs(25) && remaining < Duration::from_secs(35),
            "Das neue Fokusintervall zählt ab seinem echten Start, nicht ab Fokus"
        );
        // The break that ended while hidden saved no session, and the clock
        // of the live phase was started at its real deadline.
        assert_eq!(
            run.clock.completes_at(),
            Some(past + Duration::from_secs(180)),
            "Die Pause wurde mitgezählt statt neu gestartet"
        );
    }

    #[test]
    fn all_pages_and_editor_render_at_small_and_large_sizes() {
        let (_dir, mut app, ctx) = app();
        for size in [
            Vec2::new(626.0, 520.0),
            Vec2::new(720.0, 560.0),
            Vec2::new(1180.0, 820.0),
        ] {
            for page in [
                Page::Focus,
                Page::Stats,
                Page::Trash,
                Page::Conflicts,
                Page::Settings,
            ] {
                app.page = page;
                let output = frame(&mut app, &ctx, vec![], size);
                assert!(!output.shapes.is_empty());
                ctx.tessellate(output.shapes, output.pixels_per_point);
            }
            app.dialog = Some(Dialog::Editor(Box::new(Editor::new(None))));
            for _ in 0..3 {
                frame(&mut app, &ctx, vec![], size);
            }
            let output = frame(&mut app, &ctx, vec![], size);
            assert!(output.shapes.iter().any(|s| {
                text_position(&s.shape, "Speichern")
                    .is_some_and(|p| p.y < size.y && s.clip_rect.contains(p))
            }));
            app.dialog = None;
        }
    }

    #[test]
    fn timer_controls_remain_responsive_while_sync_is_pending() {
        let (_dir, mut app, ctx) = app();
        app.preferences.activity = "Parallel".into();
        click(&mut app, &ctx, "Fokus starten");
        let (_sender, receiver) = mpsc::channel();
        app.syncing = Some(receiver);
        click(&mut app, &ctx, "Pausieren");
        assert!(app.run.as_ref().unwrap().clock.paused);
        assert!(app.syncing.is_some());
        click(&mut app, &ctx, "Fortsetzen");
        assert!(!app.run.as_ref().unwrap().clock.paused);
    }

    #[test]
    fn ui_conflict_version_selection_and_corrected_resolution() {
        let (_dir, mut app, ctx) = app();
        let first = history::Revision {
            revision_id: "a".into(),
            session_id: "conflict".into(),
            parents: vec![],
            device_id: "Laptop".into(),
            created_at: "2026-01-01T12:00:00Z".into(),
            session: Session::now("Version A", 25.0, ""),
            deleted: false,
            legacy_id: None,
        };
        let second = history::Revision {
            revision_id: "b".into(),
            device_id: "Desktop".into(),
            session: Session::now("Version B", 45.0, ""),
            deleted: true,
            ..first.clone()
        };
        app.store.merge(&[first, second], false).unwrap();
        app.reload();
        click(&mut app, &ctx, "Konflikte (1)");
        click(&mut app, &ctx, "Version A");
        assert_eq!(app.selected.as_deref(), Some("conflict"));
        click(&mut app, &ctx, "Konflikt auflösen");
        assert!(matches!(app.dialog, Some(Dialog::Editor(_))));
        click(&mut app, &ctx, "Version 1 von 2");
        click(&mut app, &ctx, "2 · 2026-01-01  12:00 · Papierkorb");
        let Some(Dialog::Editor(editor)) = &mut app.dialog else {
            panic!("Conflict editor missing")
        };
        assert_eq!(editor.session.activity, "Version B");
        assert!(editor.deleted);
        editor.session.activity = "Zusammengeführt".into();
        click(&mut app, &ctx, "Im Papierkorb behalten");
        click(&mut app, &ctx, "Konflikt auflösen");
        assert!(app.dialog.is_none());
        assert_eq!(app.store.revisions().unwrap().len(), 3);
        assert_eq!(
            app.store.entries().unwrap()[0].active().unwrap().activity,
            "Zusammengeführt"
        );
    }

    #[test]
    fn ui_calendar_days_toggle_includes_gaps_filter_edges_and_empty_periods() {
        let (_dir, mut app, ctx) = app();
        for day in ["2024-02-28", "2024-03-01"] {
            let mut session = Session::now("Lesen", 15.0, "");
            session.date = day.into();
            session.timestamp = format!("{day}T12:00:00+01:00");
            app.store.create(session, None).unwrap();
        }
        app.reload();
        app.page = Page::Stats;
        let size = Vec2::new(1180.0, 820.0);
        let output = frame(&mut app, &ctx, vec![], size);
        assert!(
            !output
                .shapes
                .iter()
                .any(|s| text_position(&s.shape, "29.02.").is_some())
        );

        click(&mut app, &ctx, "Alle Kalendertage");
        assert!(app.preferences.include_calendar_days);
        for size in [Vec2::new(720.0, 560.0), size] {
            let output = frame(&mut app, &ctx, vec![], size);
            for label in ["28.02.", "29.02.", "01.03."] {
                assert!(
                    output
                        .shapes
                        .iter()
                        .any(|s| text_position(&s.shape, label).is_some()),
                    "Missing calendar day: {label}"
                );
            }
        }

        app.filter.since = "2024-02-27".into();
        app.filter.until = "2024-03-02".into();
        // A fully specified period remains visible even without matching sessions.
        app.filter.activity = "Keine Treffer".into();
        let output = frame(&mut app, &ctx, vec![], size);
        for label in ["27.02.", "28.02.", "29.02.", "01.03.", "02.03."] {
            assert!(
                output
                    .shapes
                    .iter()
                    .any(|s| text_position(&s.shape, label).is_some()),
                "Missing calendar day in empty period: {label}"
            );
        }
        click(&mut app, &ctx, "Alle Kalendertage");
        assert!(!app.preferences.include_calendar_days);
        let output = frame(&mut app, &ctx, vec![], size);
        assert!(output.shapes.iter().any(|s| {
            text_position(
                &s.shape,
                "Noch keine Fokuszeit für diesen Zeitraum. Gespeicherte Sitzungen erscheinen hier.",
            )
            .is_some()
        }));
    }

    #[test]
    fn ui_stats_year_chart_groups_by_month_and_shows_kpis() {
        let (_dir, mut app, ctx) = app();
        for (date, activity, minutes) in [
            ("2026-01-10", "Lesen", 30.0),
            ("2026-01-20", "Schreiben", 15.0),
            ("2026-02-05", "Lesen", 45.0),
        ] {
            let mut session = Session::now(activity, minutes, "");
            session.date = date.into();
            session.timestamp = format!("{date}T09:00:00+01:00");
            app.store.create(session, None).unwrap();
        }
        app.reload();
        app.page = Page::Stats;
        app.preferences.stats_year = 2026;
        // A tall viewport keeps every card inside the scroll area's clip rect.
        let size = Vec2::new(1400.0, 3600.0);
        let output = frame(&mut app, &ctx, vec![], size);
        for label in [
            "Gesamtzeit",
            "Kalenderjahr",
            "Jan",
            "Feb",
            "Wochentag",
            "Tageszeit",
            "Aktivitäten",
        ] {
            assert!(
                output
                    .shapes
                    .iter()
                    .any(|s| text_position(&s.shape, label).is_some()),
                "Missing on Stats page: {label}"
            );
        }
        // Switching to weeks renders ISO-week labels instead of month labels.
        app.preferences.stats_granularity = true;
        let output = frame(&mut app, &ctx, vec![], size);
        assert!(
            output
                .shapes
                .iter()
                .any(|s| text_position(&s.shape, "W03").is_some())
        );
    }

    #[test]
    fn ui_report_chart_and_totals_follow_history_filters() {
        let (_dir, mut app, ctx) = app();
        app.store
            .create(Session::now("Lesen", 15.0, ""), None)
            .unwrap();
        app.store
            .create(Session::now("Schreiben", 25.0, ""), None)
            .unwrap();
        app.reload();
        app.page = Page::Stats;
        app.filter.activity = "lesen".into();
        for size in [Vec2::new(720.0, 560.0), Vec2::new(1180.0, 820.0)] {
            let output = frame(&mut app, &ctx, vec![], size);
            assert!(
                output
                    .shapes
                    .iter()
                    .any(|s| text_position(&s.shape, "15.0 min").is_some())
            );
            assert!(
                !output
                    .shapes
                    .iter()
                    .any(|s| text_position(&s.shape, "Schreiben").is_some())
            );
        }
        app.filter.since = "unvollständig".into();
        click(&mut app, &ctx, "CSV exportieren");
        assert!(app.dialog.is_none());
        assert!(app.notice.as_ref().is_some_and(|(_, error)| *error));
    }

    #[test]
    fn ui_theme_toggle_switches_palette_and_renders() {
        let (_dir, mut app, ctx) = app();
        assert_eq!(app.preferences.theme, Theme::Light);
        app.page = Page::Settings;
        click(&mut app, &ctx, "Dunkel");
        assert_eq!(app.preferences.theme, Theme::Dark);
        // A frame applies the new palette and draws without panicking.
        let output = frame(&mut app, &ctx, vec![], Vec2::new(1180.0, 820.0));
        assert_eq!(app.applied_theme, Theme::Dark);
        assert!(palette().dark);
        assert!(!output.shapes.is_empty());
        // Switching back returns to the light palette.
        click(&mut app, &ctx, "Hell");
        assert_eq!(app.preferences.theme, Theme::Light);
        let output = frame(&mut app, &ctx, vec![], Vec2::new(1180.0, 820.0));
        assert_eq!(app.applied_theme, Theme::Light);
        assert!(!palette().dark);
        assert!(!output.shapes.is_empty());
    }
}
