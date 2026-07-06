use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use ratatui::widgets::{ListState, TableState};

use crate::config::today_iso;
use crate::models::*;
use crate::storage;
use crate::basecamp;

// ── background messages ──────────────────────────────────────────────────────

pub enum Msg {
    DataLoaded {
        projects: Vec<Project>,
        last_logged: HashMap<i64, String>,
        committed: Vec<LoggedEntry>,
        my_name: Option<String>,
    },
    TodosLoaded(Vec<Todo>),
    PendingTodosLoaded { project_id: i64, todos: Vec<PendingTodo> },
    TodoCreated { project_id: i64 },
    CampfiresLoaded(Vec<Campfire>),
    ChatLinesLoaded { room_id: i64, lines: Vec<ChatLine> },
    SendComplete { sent: usize, failures: Vec<(StagedEntry, String)> },
    Error(String),
    AuthError(String),
}

// ── todos filter ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum TodoFilter {
    All,
    Mine,
}

impl TodoFilter {
    pub fn toggle(&self) -> Self {
        match self { TodoFilter::All => TodoFilter::Mine, TodoFilter::Mine => TodoFilter::All }
    }
    pub fn label(&self) -> &'static str {
        match self { TodoFilter::All => "todos", TodoFilter::Mine => "míos" }
    }
}

// ── form fields ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum FormField {
    Hours,
    Comment,
    Date,
}

// ── timer ─────────────────────────────────────────────────────────────────────

pub struct TimerState {
    pub project: Project,
    pub accumulated: f64,
    pub running: bool,
    pub started_at: Option<Instant>,
}

impl TimerState {
    pub fn new(project: Project) -> Self {
        Self { project, accumulated: 0.0, running: true, started_at: Some(Instant::now()) }
    }

    pub fn elapsed_seconds(&self) -> f64 {
        let mut s = self.accumulated;
        if self.running {
            if let Some(t) = self.started_at {
                s += t.elapsed().as_secs_f64();
            }
        }
        s
    }

    pub fn toggle(&mut self) {
        if self.running {
            self.accumulated = self.elapsed_seconds();
            self.running = false;
            self.started_at = None;
        } else {
            self.running = true;
            self.started_at = Some(Instant::now());
        }
    }
}

// ── screens ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum MainFocus {
    Projects,
    Logged,  // today committed (synced) panel
    Staged,  // today staged panel
}

pub struct MainState {
    pub project_table: TableState,
    pub logged_table: TableState,
    pub staged_table: TableState,
    pub todos_list: ListState,
    pub focus: MainFocus,
}

impl Default for MainState {
    fn default() -> Self {
        let mut s = Self {
            project_table: TableState::default(),
            logged_table: TableState::default(),
            staged_table: TableState::default(),
            todos_list: ListState::default(),
            focus: MainFocus::Projects,
        };
        s.project_table.select(Some(0));
        s
    }
}

pub struct AddTodoState {
    pub project: Project,
    pub title: String,
    pub error: String,
    pub submitting: bool,
}

pub struct FormState {
    pub project: Project,
    pub initial_hours: String,
    pub hours: String,
    pub comment: String,
    pub date: String,
    pub selected_todo: Option<Todo>,
    pub field: FormField,
    pub error: String,
}

impl FormState {
    pub fn new(project: Project, initial_hours: String) -> Self {
        let date = today_iso();
        let field = if initial_hours.is_empty() { FormField::Hours } else { FormField::Comment };
        Self {
            hours: initial_hours.clone(),
            initial_hours,
            project,
            comment: String::new(),
            date,
            selected_todo: None,
            field,
            error: String::new(),
        }
    }
}

pub struct TodoPickerState {
    pub project: Project,
    pub return_hours: String,
    pub return_comment: String,
    pub return_date: String,
    pub return_todo: Option<Todo>,
    pub todos: Vec<Todo>,
    pub list: ListState,
    pub loading: bool,
}

pub struct ConfirmState {
    pub entries: Vec<StagedEntry>,
    pub table: TableState,
    pub sending: bool,
}

impl ConfirmState {
    pub fn new() -> Self {
        let entries = storage::load_staged();
        Self { entries, table: TableState::default(), sending: false }
    }
}

pub struct ChatState {
    pub rooms: Vec<Campfire>,
    pub rooms_list: ListState,
    pub current: Option<Campfire>,
    pub lines: Vec<ChatLine>,
    pub input: String,
    pub loading_rooms: bool,
    pub loading_lines: bool,
    pub focus_input: bool,
    pub last_poll: Instant,
}

impl ChatState {
    pub fn new() -> Self {
        Self {
            rooms: vec![],
            rooms_list: ListState::default(),
            current: None,
            lines: vec![],
            input: String::new(),
            loading_rooms: true,
            loading_lines: false,
            focus_input: false,
            last_poll: Instant::now(),
        }
    }
}

pub enum Screen {
    Main(MainState),
    EntryForm(FormState),
    TodoPicker(TodoPickerState),
    Timer(TimerState),
    ConfirmSend(ConfirmState),
    Chat(ChatState),
    AddTodo(AddTodoState),
}

// ── notification ───────────────────────────────────────────────────────────────

pub struct Notification {
    pub message: String,
    pub error: bool,
    pub shown_at: Instant,
}

// ── App ────────────────────────────────────────────────────────────────────────

pub struct App {
    pub screen: Screen,
    pub projects: Vec<Project>,
    pub favorites: HashSet<i64>,
    pub last_logged: HashMap<i64, String>,
    pub today_committed: Vec<LoggedEntry>,
    pub pending_todos: Vec<PendingTodo>,
    pub pending_todos_project_id: Option<i64>,
    pub pending_todos_loading: bool,
    pub todos_filter: TodoFilter,
    pub my_name: Option<String>,
    pub loading: bool,
    pub notification: Option<Notification>,
    pub should_quit: bool,
    pub tx: Sender<Msg>,
    pub rx: Receiver<Msg>,
}

impl App {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            screen: Screen::Main(MainState::default()),
            projects: vec![],
            favorites: storage::load_favorites(),
            last_logged: HashMap::new(),
            today_committed: vec![],
            pending_todos: vec![],
            pending_todos_project_id: None,
            pending_todos_loading: false,
            todos_filter: TodoFilter::All,
            my_name: None,
            loading: true,
            notification: None,
            should_quit: false,
            tx,
            rx,
        };
        app.spawn_load_data();
        app
    }

    pub fn notify(&mut self, msg: impl Into<String>, error: bool) {
        self.notification = Some(Notification {
            message: msg.into(),
            error,
            shown_at: Instant::now(),
        });
    }

    pub fn tick_notification(&mut self) {
        if let Some(ref n) = self.notification {
            if n.shown_at.elapsed().as_secs() >= 4 {
                self.notification = None;
            }
        }
    }

    // ── background loaders ──────────────────────────────────────────────────

    pub fn spawn_load_data(&mut self) {
        self.loading = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let projects = match basecamp::list_projects() {
                Ok(p) => p,
                Err(e) => {
                    let msg = if e.auth {
                        format!("{} (run: basecamp auth login --scope full)", e.message)
                    } else {
                        e.message
                    };
                    tx.send(Msg::Error(msg)).ok();
                    return;
                }
            };
            let logged = match basecamp::my_window_entries(90) {
                Ok(l) => l,
                Err(e) => {
                    tx.send(Msg::Error(e.message)).ok();
                    return;
                }
            };
            let mut last_logged: HashMap<i64, String> = HashMap::new();
            for e in &logged {
                if let Some(pid) = e.project_id {
                    let entry = last_logged.entry(pid).or_default();
                    if e.created_at > *entry {
                        *entry = e.created_at.clone();
                    }
                }
            }
            let today = today_iso();
            let committed: Vec<LoggedEntry> = logged.into_iter().filter(|e| e.date == today).collect();
            let my_name = basecamp::my_name();
            tx.send(Msg::DataLoaded { projects, last_logged, committed, my_name }).ok();
        });
    }

    pub fn spawn_load_todos(&mut self, project_id: i64) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            match basecamp::list_todos(project_id) {
                Ok(todos) => tx.send(Msg::TodosLoaded(todos)).ok(),
                Err(e) => tx.send(Msg::Error(e.message)).ok(),
            };
        });
    }

    pub fn spawn_load_pending_todos(&mut self, project_id: i64) {
        if self.pending_todos_project_id == Some(project_id) && !self.pending_todos_loading {
            return; // already loaded for this project
        }
        self.pending_todos_loading = true;
        self.pending_todos_project_id = Some(project_id);
        self.pending_todos = vec![];
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            match basecamp::list_pending_todos(project_id) {
                Ok(todos) => tx.send(Msg::PendingTodosLoaded { project_id, todos }).ok(),
                Err(e) => tx.send(Msg::Error(e.message)).ok(),
            };
        });
    }

    pub fn spawn_create_todo(&mut self, project: Project, title: String) {
        let tx = self.tx.clone();
        let project_id = project.id;
        std::thread::spawn(move || {
            match basecamp::get_first_todolist(project_id) {
                Ok(Some(list_id)) => {
                    match basecamp::create_todo(project_id, list_id, &title) {
                        Ok(()) => tx.send(Msg::TodoCreated { project_id }).ok(),
                        Err(e) => tx.send(Msg::Error(e.message)).ok(),
                    };
                }
                Ok(None) => {
                    tx.send(Msg::Error(format!("No se encontró ninguna lista de tareas en este proyecto."))).ok();
                }
                Err(e) => {
                    tx.send(Msg::Error(e.message)).ok();
                }
            }
        });
    }

    pub fn spawn_load_campfires(&mut self) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            match basecamp::list_campfires() {
                Ok(rooms) => tx.send(Msg::CampfiresLoaded(rooms)).ok(),
                Err(e) => tx.send(Msg::Error(e.message)).ok(),
            };
        });
    }

    pub fn spawn_load_chat_lines(&mut self, bucket_id: i64, room_id: i64) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            match basecamp::chat_lines(bucket_id, room_id) {
                Ok(lines) => tx.send(Msg::ChatLinesLoaded { room_id, lines }).ok(),
                Err(e) => tx.send(Msg::Error(e.message)).ok(),
            };
        });
    }

    pub fn spawn_send_entries(&mut self, entries: Vec<StagedEntry>) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let mut sent = 0;
            let mut failures = vec![];
            for e in entries {
                let rid = if let Some(todo_id) = e.todo_id {
                    Some(todo_id)
                } else {
                    basecamp::project_timesheet_recording_id(e.project_id)
                };
                match rid {
                    None => {
                        failures.push((e, "no project timesheet — tag a todo instead".to_string()));
                    }
                    Some(recording_id) => {
                        let hours = e.hours.trim().replace(',', ".");
                        match basecamp::create_entry(recording_id, &e.date, &hours, &e.description) {
                            Ok(()) => {
                                storage::remove_staged(&e.id);
                                sent += 1;
                            }
                            Err(err) => {
                                let msg = if err.auth {
                                    format!("{} (run: basecamp auth login --scope full)", err.message)
                                } else {
                                    err.message
                                };
                                failures.push((e, msg));
                            }
                        }
                    }
                }
            }
            tx.send(Msg::SendComplete { sent, failures }).ok();
        });
    }

    // ── sorted projects ────────────────────────────────────────────────────

    pub fn sorted_projects(&self) -> Vec<&Project> {
        let mut projects: Vec<&Project> = self.projects.iter().collect();
        projects.sort_by(|a, b| {
            let a_fav = self.favorites.contains(&a.id);
            let b_fav = self.favorites.contains(&b.id);
            let a_logged = self.last_logged.contains_key(&a.id);
            let b_logged = self.last_logged.contains_key(&b.id);
            let a_recency = self.last_logged.get(&a.id).cloned().unwrap_or(a.updated_at.clone());
            let b_recency = self.last_logged.get(&b.id).cloned().unwrap_or(b.updated_at.clone());
            b_fav.cmp(&a_fav)
                .then(b_logged.cmp(&a_logged))
                .then(b_recency.cmp(&a_recency))
        });
        projects
    }

    // ── message handler ────────────────────────────────────────────────────

    pub fn handle_msg(&mut self, msg: Msg) {
        match msg {
            Msg::DataLoaded { projects, last_logged, committed, my_name } => {
                self.projects = projects;
                self.last_logged = last_logged;
                self.today_committed = committed;
                if my_name.is_some() { self.my_name = my_name; }
                self.loading = false;
                if let Screen::Main(ref mut s) = self.screen {
                    if s.project_table.selected().is_none() && !self.projects.is_empty() {
                        s.project_table.select(Some(0));
                    }
                }
                // Load todos for the first project in the list
                if let Some(p) = self.sorted_projects().first().map(|p| p.id) {
                    self.spawn_load_pending_todos(p);
                }
            }
            Msg::TodosLoaded(todos) => {
                if let Screen::TodoPicker(ref mut s) = self.screen {
                    s.todos = todos;
                    s.loading = false;
                    if !s.todos.is_empty() {
                        s.list.select(Some(0));
                    }
                }
            }
            Msg::PendingTodosLoaded { project_id, todos } => {
                if self.pending_todos_project_id == Some(project_id) {
                    self.pending_todos = todos;
                    self.pending_todos_loading = false;
                }
            }
            Msg::TodoCreated { project_id } => {
                self.notify("Todo añadido ✓", false);
                // Reload todos panel for this project
                self.pending_todos_project_id = None; // force reload
                self.spawn_load_pending_todos(project_id);
                // Return to main if we were in AddTodo
                if let Screen::AddTodo(_) = self.screen {
                    self.screen = Screen::Main(MainState::default());
                }
            }
            Msg::CampfiresLoaded(rooms) => {
                if let Screen::Chat(ref mut s) = self.screen {
                    s.rooms = rooms;
                    s.loading_rooms = false;
                    if !s.rooms.is_empty() && s.rooms_list.selected().is_none() {
                        s.rooms_list.select(Some(0));
                    }
                }
            }
            Msg::ChatLinesLoaded { room_id, lines } => {
                if let Screen::Chat(ref mut s) = self.screen {
                    if s.current.as_ref().map(|r| r.id) == Some(room_id) {
                        s.lines = lines;
                        s.loading_lines = false;
                    }
                }
            }
            Msg::SendComplete { sent, failures } => {
                if let Screen::ConfirmSend(_) = self.screen {
                    self.screen = Screen::Main(MainState::default());
                    self.spawn_load_data();
                }
                if failures.is_empty() {
                    let word = if sent == 1 { "entrada" } else { "entradas" };
                    self.notify(format!("Enviadas {sent} {word} a Basecamp ✓"), false);
                } else {
                    let detail: Vec<String> = failures.iter().take(3)
                        .map(|(e, m)| format!("{}: {}", e.project_name, m))
                        .collect();
                    self.notify(
                        format!("Enviadas {sent}, {} fallaron. {}", failures.len(), detail.join("; ")),
                        true,
                    );
                }
            }
            Msg::Error(e) => {
                self.loading = false;
                self.notify(e, true);
            }
            Msg::AuthError(e) => {
                self.loading = false;
                self.notify(format!("{e} (run: basecamp auth login --scope full)"), true);
            }
        }
    }

    // ── key handler ────────────────────────────────────────────────────────

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::{KeyCode, KeyModifiers};

        match &mut self.screen {
            Screen::Main(_state) => {
                match key.code {
                    KeyCode::Char('q') => self.should_quit = true,
                    KeyCode::Char('r') => self.spawn_load_data(),
                    KeyCode::Char('g') => {
                        let chat = ChatState::new();
                        self.screen = Screen::Chat(chat);
                        self.spawn_load_campfires();
                    }
                    KeyCode::Char('c') => {
                        let staged = storage::load_staged();
                        if staged.is_empty() {
                            self.notify("No hay entradas pendientes.", false);
                        } else {
                            self.screen = Screen::ConfirmSend(ConfirmState::new());
                        }
                    }
                    KeyCode::Tab => {
                        let s = match &mut self.screen { Screen::Main(s) => s, _ => return };
                        s.focus = match s.focus {
                            MainFocus::Projects => MainFocus::Logged,
                            MainFocus::Logged   => MainFocus::Staged,
                            MainFocus::Staged   => MainFocus::Projects,
                        };
                    }
                    KeyCode::BackTab => {
                        let s = match &mut self.screen { Screen::Main(s) => s, _ => return };
                        s.focus = match s.focus {
                            MainFocus::Projects => MainFocus::Staged,
                            MainFocus::Logged   => MainFocus::Projects,
                            MainFocus::Staged   => MainFocus::Logged,
                        };
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        let s = match &mut self.screen { Screen::Main(s) => s, _ => return };
                        match s.focus {
                            MainFocus::Projects => list_prev(&mut s.project_table),
                            MainFocus::Logged   => list_prev(&mut s.logged_table),
                            MainFocus::Staged   => list_prev(&mut s.staged_table),
                        }
                        // Reload todos panel when project selection changes
                        if let Screen::Main(s) = &self.screen {
                            if s.focus == MainFocus::Projects {
                                if let Some(p) = self.selected_project() {
                                    self.spawn_load_pending_todos(p.id);
                                }
                            }
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let s = match &mut self.screen { Screen::Main(s) => s, _ => return };
                        match s.focus {
                            MainFocus::Projects => {
                                let len = self.projects.len();
                                list_next(&mut s.project_table, len);
                            }
                            MainFocus::Logged => {
                                let len = self.today_committed.len();
                                list_next(&mut s.logged_table, len);
                            }
                            MainFocus::Staged => {
                                let len = storage::load_staged().len();
                                list_next(&mut s.staged_table, len);
                            }
                        }
                        if let Screen::Main(s) = &self.screen {
                            if s.focus == MainFocus::Projects {
                                if let Some(p) = self.selected_project() {
                                    self.spawn_load_pending_todos(p.id);
                                }
                            }
                        }
                    }
                    KeyCode::Enter => {
                        let project = self.selected_project();
                        if let Some(p) = project {
                            self.screen = Screen::EntryForm(FormState::new(p, String::new()));
                        }
                    }
                    KeyCode::Char('s') => {
                        let project = self.selected_project();
                        if let Some(p) = project {
                            self.screen = Screen::Timer(TimerState::new(p));
                        }
                    }
                    KeyCode::Char('f') => {
                        let project = self.selected_project();
                        if let Some(p) = project {
                            self.favorites = storage::toggle_favorite(p.id);
                            let faved = self.favorites.contains(&p.id);
                            let msg = if faved {
                                format!("★ Favorito: {}", p.name)
                            } else {
                                format!("Eliminado de favoritos: {}", p.name)
                            };
                            self.notify(msg, false);
                        }
                    }
                    KeyCode::Char('t') => {
                        let project = self.selected_project();
                        if let Some(p) = project {
                            self.screen = Screen::AddTodo(AddTodoState {
                                project: p,
                                title: String::new(),
                                error: String::new(),
                                submitting: false,
                            });
                        }
                    }
                    KeyCode::Char('T') => {
                        self.todos_filter = self.todos_filter.toggle();
                    }
                    KeyCode::Char('d') => {
                        // Extract all info first to avoid double-borrow
                        let (focus, selected) = match &self.screen {
                            Screen::Main(s) => (s.focus.clone(), s.staged_table.selected()),
                            _ => return,
                        };
                        if focus == MainFocus::Staged {
                            let staged = storage::load_staged();
                            if let Some(idx) = selected {
                                if idx < staged.len() {
                                    let id = staged[idx].id.clone();
                                    storage::remove_staged(&id);
                                    self.notify("Entrada eliminada.", false);
                                    let new_len = storage::load_staged().len();
                                    if let Screen::Main(s) = &mut self.screen {
                                        if new_len == 0 {
                                            s.staged_table.select(None);
                                        } else {
                                            s.staged_table.select(Some(idx.min(new_len - 1)));
                                        }
                                    }
                                }
                            }
                        } else if focus == MainFocus::Logged {
                            self.notify("Esa entrada ya está en Basecamp — no se puede eliminar aquí.", true);
                        }
                    }
                    _ => {}
                }
            }

            Screen::EntryForm(_state) => {
                match (key.code, key.modifiers) {
                    (KeyCode::Esc, _) => {
                        self.screen = Screen::Main(MainState::default());
                    }
                    (KeyCode::Tab, _) => {
                        let s = match &mut self.screen { Screen::EntryForm(s) => s, _ => return };
                        s.field = match s.field {
                            FormField::Hours => FormField::Comment,
                            FormField::Comment => FormField::Date,
                            FormField::Date => FormField::Hours,
                        };
                    }
                    (KeyCode::BackTab, _) => {
                        let s = match &mut self.screen { Screen::EntryForm(s) => s, _ => return };
                        s.field = match s.field {
                            FormField::Hours => FormField::Date,
                            FormField::Comment => FormField::Hours,
                            FormField::Date => FormField::Comment,
                        };
                    }
                    (KeyCode::Char('t'), KeyModifiers::CONTROL) => {
                        // Open todo picker
                        if let Screen::EntryForm(form) = &self.screen {
                            let state = TodoPickerState {
                                project: form.project.clone(),
                                return_hours: form.hours.clone(),
                                return_comment: form.comment.clone(),
                                return_date: form.date.clone(),
                                return_todo: form.selected_todo.clone(),
                                todos: vec![],
                                list: ListState::default(),
                                loading: true,
                            };
                            let pid = form.project.id;
                            self.screen = Screen::TodoPicker(state);
                            self.spawn_load_todos(pid);
                        }
                    }
                    (KeyCode::Enter, _) => {
                        // Try to stage the entry
                        if let Screen::EntryForm(form) = &mut self.screen {
                            match crate::models::parse_hours(&form.hours) {
                                Err(e) => {
                                    form.error = e;
                                    form.field = FormField::Hours;
                                }
                                Ok(_) => {
                                    let entry = StagedEntry::new(
                                        form.project.id,
                                        form.project.name.clone(),
                                        form.hours.clone(),
                                        form.comment.clone(),
                                        if form.date.is_empty() { today_iso() } else { form.date.clone() },
                                        form.selected_todo.as_ref().map(|t| t.id),
                                        form.selected_todo.as_ref().map(|t| t.title.clone()),
                                    );
                                    storage::add_staged(entry);
                                    self.screen = Screen::Main(MainState::default());
                                    self.notify("Entrada añadida a la cola.", false);
                                }
                            }
                        }
                    }
                    (KeyCode::Char(c), _) => {
                        let s = match &mut self.screen { Screen::EntryForm(s) => s, _ => return };
                        s.error.clear();
                        match s.field {
                            FormField::Hours => s.hours.push(c),
                            FormField::Comment => s.comment.push(c),
                            FormField::Date => s.date.push(c),
                        }
                    }
                    (KeyCode::Backspace, _) => {
                        let s = match &mut self.screen { Screen::EntryForm(s) => s, _ => return };
                        match s.field {
                            FormField::Hours => { s.hours.pop(); }
                            FormField::Comment => { s.comment.pop(); }
                            FormField::Date => { s.date.pop(); }
                        }
                    }
                    _ => {}
                }
            }

            Screen::TodoPicker(_state) => {
                match key.code {
                    KeyCode::Esc => {
                        // Return to form with original state
                        if let Screen::TodoPicker(s) = &self.screen {
                            let form = FormState {
                                project: s.project.clone(),
                                initial_hours: s.return_hours.clone(),
                                hours: s.return_hours.clone(),
                                comment: s.return_comment.clone(),
                                date: s.return_date.clone(),
                                selected_todo: s.return_todo.clone(),
                                field: FormField::Comment,
                                error: String::new(),
                            };
                            self.screen = Screen::EntryForm(form);
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        let s = match &mut self.screen { Screen::TodoPicker(s) => s, _ => return };
                        let len = s.todos.len();
                        if len > 0 {
                            let i = s.list.selected().unwrap_or(0);
                            s.list.select(Some(if i == 0 { len - 1 } else { i - 1 }));
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let s = match &mut self.screen { Screen::TodoPicker(s) => s, _ => return };
                        let len = s.todos.len();
                        if len > 0 {
                            let i = s.list.selected().unwrap_or(0);
                            s.list.select(Some((i + 1) % len));
                        }
                    }
                    KeyCode::Enter => {
                        if let Screen::TodoPicker(s) = &self.screen {
                            let selected_todo = s.list.selected().and_then(|i| s.todos.get(i)).cloned();
                            let form = FormState {
                                project: s.project.clone(),
                                initial_hours: s.return_hours.clone(),
                                hours: s.return_hours.clone(),
                                comment: s.return_comment.clone(),
                                date: s.return_date.clone(),
                                selected_todo,
                                field: FormField::Comment,
                                error: String::new(),
                            };
                            self.screen = Screen::EntryForm(form);
                        }
                    }
                    _ => {}
                }
            }

            Screen::Timer(_state) => {
                match key.code {
                    KeyCode::Esc => {
                        let _project = if let Screen::Timer(s) = &self.screen {
                            s.project.clone()
                        } else { return };
                        self.screen = Screen::Main(MainState::default());
                    }
                    KeyCode::Char(' ') | KeyCode::Char('p') => {
                        if let Screen::Timer(s) = &mut self.screen {
                            s.toggle();
                        }
                    }
                    KeyCode::Char('s') => {
                        if let Screen::Timer(timer) = &self.screen {
                            let minutes = round_up_5min(timer.elapsed_seconds());
                            let project = timer.project.clone();
                            let hours = minutes_to_hms(minutes);
                            self.screen = Screen::EntryForm(FormState::new(project, hours));
                        }
                    }
                    _ => {}
                }
            }

            Screen::ConfirmSend(_state) => {
                match key.code {
                    KeyCode::Esc => {
                        self.screen = Screen::Main(MainState::default());
                    }
                    KeyCode::Char('y') | KeyCode::Enter => {
                        if let Screen::ConfirmSend(s) = &mut self.screen {
                            if !s.sending && !s.entries.is_empty() {
                                s.sending = true;
                                let entries = s.entries.clone();
                                self.spawn_send_entries(entries);
                            }
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        let s = match &mut self.screen { Screen::ConfirmSend(s) => s, _ => return };
                        list_prev(&mut s.table);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let s = match &mut self.screen { Screen::ConfirmSend(s) => s, _ => return };
                        let len = s.entries.len();
                        list_next(&mut s.table, len);
                    }
                    _ => {}
                }
            }

            Screen::Chat(state) => {
                match key.code {
                    KeyCode::Esc => {
                        if state.focus_input {
                            state.focus_input = false;
                        } else {
                            self.screen = Screen::Main(MainState::default());
                        }
                    }
                    KeyCode::Char('q') if !state.focus_input => {
                        self.should_quit = true;
                    }
                    KeyCode::Char('r') if !state.focus_input => {
                        let s = match &mut self.screen { Screen::Chat(s) => s, _ => return };
                        s.loading_rooms = true;
                        self.spawn_load_campfires();
                        if let Screen::Chat(s) = &self.screen {
                            if let Some(ref room) = s.current {
                                self.spawn_load_chat_lines(room.bucket_id, room.id);
                            }
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') if !state.focus_input => {
                        let s = match &mut self.screen { Screen::Chat(s) => s, _ => return };
                        if !s.rooms.is_empty() {
                            let i = s.rooms_list.selected().unwrap_or(0);
                            s.rooms_list.select(Some(if i == 0 { s.rooms.len() - 1 } else { i - 1 }));
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') if !state.focus_input => {
                        let s = match &mut self.screen { Screen::Chat(s) => s, _ => return };
                        if !s.rooms.is_empty() {
                            let len = s.rooms.len();
                            let i = s.rooms_list.selected().unwrap_or(0);
                            s.rooms_list.select(Some((i + 1) % len));
                        }
                    }
                    KeyCode::Enter if !state.focus_input => {
                        if let Screen::Chat(s) = &mut self.screen {
                            if let Some(idx) = s.rooms_list.selected() {
                                if let Some(room) = s.rooms.get(idx).cloned() {
                                    s.loading_lines = true;
                                    let bid = room.bucket_id;
                                    let rid = room.id;
                                    s.current = Some(room);
                                    s.lines = vec![];
                                    s.focus_input = true;
                                    self.spawn_load_chat_lines(bid, rid);
                                }
                            }
                        }
                    }
                    KeyCode::Enter if state.focus_input => {
                        if let Screen::Chat(s) = &mut self.screen {
                            let text = s.input.trim().to_string();
                            if !text.is_empty() {
                                if let Some(ref room) = s.current {
                                    let bid = room.bucket_id;
                                    let rid = room.id;
                                    s.input.clear();
                                    let tx = self.tx.clone();
                                    let t = text.clone();
                                    std::thread::spawn(move || {
                                        if let Err(e) = basecamp::post_chat_line(bid, rid, &t) {
                                            tx.send(Msg::Error(e.message)).ok();
                                        }
                                        // reload lines after sending
                                        match basecamp::chat_lines(bid, rid) {
                                            Ok(lines) => tx.send(Msg::ChatLinesLoaded { room_id: rid, lines }).ok(),
                                            Err(e) => tx.send(Msg::Error(e.message)).ok(),
                                        };
                                    });
                                }
                            }
                        }
                    }
                    KeyCode::Char(c) if state.focus_input => {
                        let s = match &mut self.screen { Screen::Chat(s) => s, _ => return };
                        s.input.push(c);
                    }
                    KeyCode::Backspace if state.focus_input => {
                        let s = match &mut self.screen { Screen::Chat(s) => s, _ => return };
                        s.input.pop();
                    }
                    _ => {}
                }
            }

            Screen::AddTodo(state) => {
                match key.code {
                    KeyCode::Esc => {
                        self.screen = Screen::Main(MainState::default());
                    }
                    KeyCode::Enter => {
                        if let Screen::AddTodo(s) = &mut self.screen {
                            let title = s.title.trim().to_string();
                            if title.is_empty() {
                                s.error = "Escribe el título del todo.".to_string();
                                return;
                            }
                            s.submitting = true;
                            s.error.clear();
                            let project = s.project.clone();
                            self.spawn_create_todo(project, title);
                        }
                    }
                    KeyCode::Char(c) => {
                        if let Screen::AddTodo(s) = &mut self.screen {
                            if !s.submitting { s.title.push(c); }
                        }
                    }
                    KeyCode::Backspace => {
                        if let Screen::AddTodo(s) = &mut self.screen {
                            if !s.submitting { s.title.pop(); }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn selected_project(&self) -> Option<Project> {
        let Screen::Main(s) = &self.screen else { return None };
        let idx = s.project_table.selected()?;
        self.sorted_projects().get(idx).map(|p| (*p).clone())
    }
}

fn list_prev(state: &mut TableState) {
    let i = state.selected().unwrap_or(0);
    state.select(Some(if i == 0 { 0 } else { i - 1 }));
}

fn list_next(state: &mut TableState, len: usize) {
    if len == 0 { return; }
    let i = state.selected().unwrap_or(0);
    state.select(Some((i + 1).min(len - 1)));
}
