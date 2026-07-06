use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, Wrap,
    },
    Frame,
};

use crate::app::*;
use crate::config::today_iso;
use crate::models::PendingTodo;
use crate::storage;

// ── colour palette ────────────────────────────────────────────────────────────

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const ERR: Color = Color::Red;
const OK: Color = Color::Green;
const WARN: Color = Color::Yellow;

pub fn render(f: &mut Frame, app: &App) {
    match &app.screen {
        Screen::Main(s) => render_main(f, app, s),
        Screen::EntryForm(s) => {
            render_main_bg(f, app);
            render_form(f, s);
        }
        Screen::TodoPicker(s) => {
            render_main_bg(f, app);
            render_todo_picker(f, s);
        }
        Screen::Timer(s) => {
            render_main_bg(f, app);
            render_timer(f, s);
        }
        Screen::ConfirmSend(s) => {
            render_main_bg(f, app);
            render_confirm(f, s);
        }
        Screen::Chat(s) => render_chat(f, app, s),
        Screen::AddTodo(s) => {
            render_main_bg(f, app);
            render_add_todo(f, s);
        }
    }

    render_notification(f, app);
}

// ── main screen ───────────────────────────────────────────────────────────────
//
//  ┌─ Projects ────────────┬─ Chat ─────────────────────────────┐
//  │                       │                                    │
//  │                       │                                    │
//  │                       ├─ Today logged ──┬─ Today staged ──┤
//  │                       │                 │                  │
//  └───────────────────────┴─────────────────┴──────────────────┘

fn render_main(f: &mut Frame, app: &App, state: &MainState) {
    let area = f.area();

    // Outer: header + body + footer
    let outer = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .split(area);

    // Header
    let loading = if app.loading { " ⟳" } else { "" };
    f.render_widget(
        Paragraph::new(format!(" Settlement — Basecamp{loading}"))
            .style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        outer[0],
    );

    // Body: left (projects) | right (chat + today panels)
    let columns = Layout::horizontal([
        Constraint::Percentage(45),
        Constraint::Fill(1),
    ])
    .split(outer[1]);

    render_projects(f, app, state, columns[0]);

    // Right column: chat (top) + today logged + today staged (bottom)
    let right = Layout::vertical([
        Constraint::Percentage(55),
        Constraint::Fill(1),
    ])
    .split(columns[1]);

    render_chat_preview(f, app, right[0]);

    let bottom = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Fill(1),
    ])
    .split(right[1]);

    render_today_logged(f, app, state, bottom[0]);
    render_today_staged(f, app, state, bottom[1]);

    // Footer
    let footer = match state.focus {
        MainFocus::Projects => " Enter log  s timer  f fav  Tab→  c send  g chat  r refresh  q quit",
        MainFocus::Logged   => " ←Tab  d delete*  c send  q quit  (*only staged rows)",
        MainFocus::Staged   => " ←Tab  d delete  c send  q quit",
    };
    f.render_widget(
        Paragraph::new(footer).style(Style::default().fg(DIM)),
        outer[2],
    );
}

fn render_main_bg(f: &mut Frame, _app: &App) {
    // render a dimmed version of main for context behind modals
    let area = f.area();
    let bg = Block::default().style(Style::default().fg(DIM));
    f.render_widget(bg, area);
}

fn render_projects(f: &mut Frame, app: &App, state: &MainState, area: Rect) {
    let sorted = app.sorted_projects();
    let count = sorted.len();

    let rows: Vec<Row> = sorted
        .iter()
        .map(|p| {
            let star = if app.favorites.contains(&p.id) { "★ " } else { "  " };
            let last = app.last_logged.get(&p.id)
                .map(|s| s[..10.min(s.len())].to_string())
                .unwrap_or_else(|| "—".to_string());
            Row::new(vec![
                Cell::from(format!("{star}{}", p.name)),
                Cell::from(last).style(Style::default().fg(DIM)),
            ])
        })
        .collect();

    let focused = state.focus == MainFocus::Projects;
    let border_style = if focused {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(DIM)
    };

    let table = Table::new(
        rows,
        [Constraint::Fill(1), Constraint::Length(12)],
    )
    .header(
        Row::new(vec!["Project", "Last logged"])
            .style(Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(format!(" Projects ({count}) "))
            .title_alignment(Alignment::Left),
    )
    .highlight_style(
        Style::default()
            .add_modifier(Modifier::REVERSED)
            .fg(if focused { ACCENT } else { Color::White }),
    );

    let mut ts = state.project_table.clone();
    f.render_stateful_widget(table, area, &mut ts);
}

fn render_today_logged(f: &mut Frame, app: &App, state: &MainState, area: Rect) {
    let focused = state.focus == MainFocus::Logged;
    let border_style = if focused { Style::default().fg(ACCENT) } else { Style::default().fg(DIM) };

    let committed_total: f64 = app.today_committed.iter().map(|e| e.hours).sum();
    let title = format!(" Logged today — {}h ", fmt_h(committed_total));

    let rows: Vec<Row> = app.today_committed.iter().map(|e| {
        Row::new(vec![
            Cell::from(e.project_name.as_str()),
            Cell::from(e.tag.as_deref().unwrap_or("—").to_string()).style(Style::default().fg(DIM)),
            Cell::from(fmt_h(e.hours)),
            Cell::from(truncate(&e.description, 20)).style(Style::default().fg(DIM)),
        ])
    }).collect();

    let table = Table::new(
        rows,
        [Constraint::Fill(1), Constraint::Length(10), Constraint::Length(6), Constraint::Length(20)],
    )
    .header(
        Row::new(vec!["Project", "Tag", "Hours", "Comment"])
            .style(Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
    )
    .block(Block::default().borders(Borders::ALL).border_style(border_style).title(title))
    .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut ts = state.logged_table.clone();
    f.render_stateful_widget(table, area, &mut ts);
}

fn render_today_staged(f: &mut Frame, app: &App, state: &MainState, area: Rect) {
    let today = today_iso();
    let staged: Vec<_> = storage::load_staged().into_iter().filter(|e| e.date == today).collect();

    let focused = state.focus == MainFocus::Staged;
    let border_style = if focused { Style::default().fg(ACCENT) } else { Style::default().fg(DIM) };

    let staged_total: f64 = staged.iter().map(|e| e.hours_float()).sum();
    let title = format!(" Staged — {}h ", fmt_h(staged_total));

    let rows: Vec<Row> = staged.iter().map(|e| {
        Row::new(vec![
            Cell::from(e.project_name.as_str()).style(Style::default().fg(WARN)),
            Cell::from(e.todo_title.as_deref().unwrap_or("—").to_string()).style(Style::default().fg(DIM)),
            Cell::from(e.hours.as_str()),
            Cell::from(truncate(&e.description, 20)),
        ])
    }).collect();

    let table = Table::new(
        rows,
        [Constraint::Fill(1), Constraint::Length(10), Constraint::Length(6), Constraint::Length(20)],
    )
    .header(
        Row::new(vec!["Project", "Tag", "Hours", "Comment"])
            .style(Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
    )
    .block(Block::default().borders(Borders::ALL).border_style(border_style).title(title))
    .highlight_style(Style::default().add_modifier(Modifier::REVERSED).fg(WARN));

    let mut ts = state.staged_table.clone();
    f.render_stateful_widget(table, area, &mut ts);
}

fn render_chat_preview(f: &mut Frame, app: &App, area: Rect) {
    let proj_idx = match &app.screen { Screen::Main(s) => s.project_table.selected().unwrap_or(0), _ => 0 };
    let proj_name = app.sorted_projects().get(proj_idx).map(|p| p.name.as_str()).unwrap_or("");

    let filter_label = app.todos_filter.label();
    let title = if proj_name.is_empty() {
        format!(" Todos [{filter_label}] ")
    } else {
        format!(" Todos [{filter_label}] — {proj_name} ")
    };

    let border_style = Style::default().fg(DIM);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title)
        .title_bottom(" t add  Shift+T filtrar  g chat ");

    if app.pending_todos_loading {
        f.render_widget(
            Paragraph::new("Cargando…").style(Style::default().fg(DIM)).block(block),
            area,
        );
        return;
    }

    // Apply filter
    let visible: Vec<&PendingTodo> = app.pending_todos.iter().filter(|t| {
        match app.todos_filter {
            TodoFilter::All => true,
            TodoFilter::Mine => {
                match (&t.assignee, &app.my_name) {
                    (Some(a), Some(me)) => a == me,
                    _ => false,
                }
            }
        }
    }).collect();

    if visible.is_empty() {
        let msg = if app.pending_todos_project_id.is_none() {
            "Selecciona un proyecto"
        } else if app.todos_filter == TodoFilter::Mine {
            "Sin tareas asignadas a ti ✓"
        } else {
            "Sin tareas pendientes ✓"
        };
        f.render_widget(
            Paragraph::new(msg).style(Style::default().fg(DIM)).block(block),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = visible.iter().map(|t| {
        let due = t.due_on.as_deref().unwrap_or("");
        let assignee = t.assignee.as_deref().unwrap_or("");
        let suffix = match (due.is_empty(), assignee.is_empty()) {
            (false, false) => format!("  [{due} · {assignee}]"),
            (false, true)  => format!("  [{due}]"),
            (true, false)  => format!("  [{assignee}]"),
            (true, true)   => String::new(),
        };
        ListItem::new(Line::from(vec![
            Span::raw("• "),
            Span::raw(t.title.as_str()),
            Span::styled(suffix, Style::default().fg(DIM)),
        ]))
    }).collect();

    let list = List::new(items).block(block);

    let mut ls = match &app.screen {
        Screen::Main(s) => s.todos_list.clone(),
        _ => ListState::default(),
    };
    f.render_stateful_widget(list, area, &mut ls);
}

// ── entry form ────────────────────────────────────────────────────────────────

fn render_form(f: &mut Frame, state: &FormState) {
    let area = centered_rect(60, 65, f.area());
    f.render_widget(Clear, area);

    let todo_line = state
        .selected_todo
        .as_ref()
        .map(|t| format!("Todo: {} (Ctrl+T to change)", t.title))
        .unwrap_or_else(|| "Todo: (none) — Ctrl+T to tag a specific todo".to_string());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(format!(" Log time — {} ", state.project.name));
    f.render_widget(block.clone(), area);

    let inner = block.inner(area);
    let rows = Layout::vertical([
        Constraint::Length(1), // title padding
        Constraint::Length(1), // hours label
        Constraint::Length(1), // hours input
        Constraint::Length(1), // gap
        Constraint::Length(1), // comment label
        Constraint::Length(1), // comment input
        Constraint::Length(1), // gap
        Constraint::Length(1), // date label
        Constraint::Length(1), // date input
        Constraint::Length(1), // gap
        Constraint::Length(1), // todo
        Constraint::Length(1), // error
        Constraint::Min(1),    // spacer
        Constraint::Length(1), // keybinds
    ])
    .split(inner);

    label(f, "Hours (e.g. 1.5 or 1:30)", rows[1]);
    input_field(f, &state.hours, state.field == FormField::Hours, rows[2]);
    label(f, "Comment", rows[4]);
    input_field(f, &state.comment, state.field == FormField::Comment, rows[5]);
    label(f, "Date", rows[7]);
    input_field(f, &state.date, state.field == FormField::Date, rows[8]);

    let todo_w = Paragraph::new(todo_line).style(Style::default().fg(DIM));
    f.render_widget(todo_w, rows[10]);

    if !state.error.is_empty() {
        let err = Paragraph::new(state.error.as_str()).style(Style::default().fg(ERR));
        f.render_widget(err, rows[11]);
    }

    let hints = Paragraph::new("Tab next field  Enter stage  Ctrl+T tag todo  Esc cancel")
        .style(Style::default().fg(DIM));
    f.render_widget(hints, rows[13]);
}

fn label(f: &mut Frame, text: &str, area: Rect) {
    f.render_widget(
        Paragraph::new(text).style(Style::default().add_modifier(Modifier::BOLD)),
        area,
    );
}

fn input_field(f: &mut Frame, value: &str, focused: bool, area: Rect) {
    let style = if focused {
        Style::default().fg(ACCENT).add_modifier(Modifier::UNDERLINED)
    } else {
        Style::default().fg(DIM)
    };
    let cursor = if focused { "▌" } else { "" };
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(style);
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(
        Paragraph::new(format!("{value}{cursor}")).style(style),
        inner,
    );
}

// ── todo picker ───────────────────────────────────────────────────────────────

fn render_todo_picker(f: &mut Frame, state: &TodoPickerState) {
    let area = centered_rect(60, 70, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(format!(" Tag a todo — {} ", state.project.name));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if state.loading {
        let p = Paragraph::new("Loading todos…").style(Style::default().fg(DIM));
        f.render_widget(p, inner);
        return;
    }

    if state.todos.is_empty() {
        let p = Paragraph::new("No todos found for this project.")
            .style(Style::default().fg(DIM));
        f.render_widget(p, inner);
        return;
    }

    let items: Vec<ListItem> = state
        .todos
        .iter()
        .map(|t| ListItem::new(t.title.as_str()))
        .collect();
    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED).fg(ACCENT))
        .highlight_symbol("▶ ");

    let mut ls = state.list.clone();
    f.render_stateful_widget(list, inner, &mut ls);

    // footer hint
    if inner.height > 2 {
        let hint_area = Rect { y: inner.y + inner.height - 1, height: 1, ..inner };
        let hint = Paragraph::new("Enter select  Esc cancel").style(Style::default().fg(DIM));
        f.render_widget(hint, hint_area);
    }
}

// ── timer ─────────────────────────────────────────────────────────────────────

fn render_timer(f: &mut Frame, state: &TimerState) {
    let area = centered_rect(40, 40, f.area());
    f.render_widget(Clear, area);

    let secs = state.elapsed_seconds() as u64;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    let elapsed = format!("{h:02}:{m:02}:{s:02}");

    let status = if state.running { "● Recording" } else { "⏸ Paused" };
    let status_style = if state.running {
        Style::default().fg(OK)
    } else {
        Style::default().fg(WARN)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(format!(" ⏱ {} ", state.project.name));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1), // elapsed
        Constraint::Length(1), // status
        Constraint::Length(1),
        Constraint::Length(1), // hints
    ])
    .split(inner);

    f.render_widget(
        Paragraph::new(elapsed)
            .alignment(Alignment::Center)
            .style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        rows[1],
    );
    f.render_widget(
        Paragraph::new(status)
            .alignment(Alignment::Center)
            .style(status_style),
        rows[2],
    );
    f.render_widget(
        Paragraph::new("Space/p pause  s stop (→ form)  Esc cancel")
            .alignment(Alignment::Center)
            .style(Style::default().fg(DIM)),
        rows[4],
    );
}

// ── confirm send ──────────────────────────────────────────────────────────────

fn render_confirm(f: &mut Frame, state: &ConfirmState) {
    let area = centered_rect(70, 70, f.area());
    f.render_widget(Clear, area);

    let title = if state.sending {
        " Sending… ".to_string()
    } else {
        format!(
            " Send {} staged {} to Basecamp? ",
            state.entries.len(),
            if state.entries.len() == 1 { "entry" } else { "entries" }
        )
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows: Vec<Row> = state
        .entries
        .iter()
        .map(|e| {
            Row::new(vec![
                Cell::from(e.date.as_str()),
                Cell::from(e.project_name.as_str()),
                Cell::from(e.todo_title.as_deref().unwrap_or("—").to_string()).style(Style::default().fg(DIM)),
                Cell::from(e.hours.as_str()),
                Cell::from(truncate(&e.description, 30)).style(Style::default().fg(DIM)),
            ])
        })
        .collect();

    let table_area = Rect { height: inner.height.saturating_sub(2), ..inner };
    let hint_area = Rect { y: inner.y + inner.height.saturating_sub(1), height: 1, ..inner };

    let table = Table::new(
        rows,
        [
            Constraint::Length(11),
            Constraint::Fill(1),
            Constraint::Length(12),
            Constraint::Length(6),
            Constraint::Length(30),
        ],
    )
    .header(
        Row::new(vec!["Date", "Project", "Tag", "Hours", "Comment"])
            .style(Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
    )
    .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut ts = state.table.clone();
    f.render_stateful_widget(table, table_area, &mut ts);

    let hint = if state.sending {
        Paragraph::new("Sending, please wait…").style(Style::default().fg(WARN))
    } else {
        Paragraph::new("Y / Enter = send,   Esc = cancel").style(Style::default().fg(DIM))
    };
    f.render_widget(hint, hint_area);
}

// ── chat screen ───────────────────────────────────────────────────────────────

fn render_chat(f: &mut Frame, _app: &App, state: &ChatState) {
    let area = f.area();
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .split(area);

    // header
    let room_title = state.current.as_ref()
        .map(|r| format!(" {} — {} ", r.project_name, r.title))
        .unwrap_or_else(|| " (no room selected) ".to_string());
    let hdr = Paragraph::new(format!(
        " Settlement — Campfire chat{}",
        if state.loading_rooms { " ⟳" } else { "" }
    ))
    .style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD));
    f.render_widget(hdr, chunks[0]);

    // body: rooms pane + chat pane
    let body = chunks[1];
    let cols = Layout::horizontal([Constraint::Length(32), Constraint::Fill(1)]).split(body);

    // rooms list
    let room_items: Vec<ListItem> = state
        .rooms
        .iter()
        .map(|r| ListItem::new(format!("{} / {}", r.project_name, r.title)))
        .collect();
    let rooms_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if !state.focus_input { Style::default().fg(ACCENT) } else { Style::default().fg(DIM) })
        .title(" Rooms ");
    let list = List::new(room_items)
        .block(rooms_block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED).fg(ACCENT))
        .highlight_symbol("▶ ");
    let mut ls = state.rooms_list.clone();
    f.render_stateful_widget(list, cols[0], &mut ls);

    // chat pane
    let chat_cols = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
    ])
    .split(cols[1]);

    let log_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if state.focus_input { Style::default().fg(ACCENT) } else { Style::default().fg(DIM) })
        .title(room_title.as_str());

    if state.loading_lines {
        f.render_widget(
            Paragraph::new("Loading…").style(Style::default().fg(DIM)).block(log_block),
            chat_cols[0],
        );
    } else {
        let lines_text: Vec<Line> = state
            .lines
            .iter()
            .map(|l| {
                Line::from(vec![
                    Span::styled(format!("{}: ", l.author), Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(l.text.as_str()),
                ])
            })
            .collect();
        f.render_widget(
            Paragraph::new(lines_text)
                .block(log_block)
                .wrap(Wrap { trim: false }),
            chat_cols[0],
        );
    }

    // input
    let input_style = if state.focus_input {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(DIM)
    };
    let cursor = if state.focus_input { "▌" } else { "" };
    let hint = if state.focus_input {
        "(Enter to send, Esc to unfocus)"
    } else {
        "(Enter to open room, Esc to go back)"
    };
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(input_style)
        .title(hint);
    f.render_widget(
        Paragraph::new(format!("{}{}", state.input, cursor)).block(input_block),
        chat_cols[1],
    );

    // footer
    let footer = Paragraph::new(" r Refresh  Esc back  q Quit").style(Style::default().fg(DIM));
    f.render_widget(footer, chunks[2]);
}

// ── add todo modal ────────────────────────────────────────────────────────────

fn render_add_todo(f: &mut Frame, state: &AddTodoState) {
    let area = centered_rect(55, 30, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(format!(" Añadir todo — {} ", state.project.name));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // label
        Constraint::Length(1), // input
        Constraint::Length(1), // error
        Constraint::Fill(1),
        Constraint::Length(1), // hints
    ])
    .split(inner);

    f.render_widget(
        Paragraph::new("Título").style(Style::default().add_modifier(Modifier::BOLD)),
        rows[0],
    );

    let cursor = if state.submitting { "" } else { "▌" };
    let input_style = Style::default().fg(ACCENT).add_modifier(Modifier::UNDERLINED);
    let input_block = Block::default().borders(Borders::LEFT).border_style(input_style);
    let input_inner = input_block.inner(rows[1]);
    f.render_widget(input_block, rows[1]);
    f.render_widget(
        Paragraph::new(format!("{}{cursor}", state.title)).style(input_style),
        input_inner,
    );

    if !state.error.is_empty() {
        f.render_widget(
            Paragraph::new(state.error.as_str()).style(Style::default().fg(ERR)),
            rows[2],
        );
    }
    if state.submitting {
        f.render_widget(
            Paragraph::new("Creando…").style(Style::default().fg(WARN)),
            rows[2],
        );
    }

    f.render_widget(
        Paragraph::new("Enter confirmar  Esc cancelar").style(Style::default().fg(DIM)),
        rows[4],
    );
}

// ── notification overlay ──────────────────────────────────────────────────────

fn render_notification(f: &mut Frame, app: &App) {
    let Some(ref n) = app.notification else { return };
    let area = f.area();
    let msg = truncate(&n.message, (area.width as usize).saturating_sub(4));
    let w = (msg.len() as u16 + 4).min(area.width);
    let h = 3u16;
    let x = area.x + area.width.saturating_sub(w);
    let y = area.y;
    let notif_area = Rect { x, y, width: w, height: h };

    let style = if n.error {
        Style::default().fg(Color::Black).bg(ERR)
    } else {
        Style::default().fg(Color::Black).bg(OK)
    };

    f.render_widget(Clear, notif_area);
    f.render_widget(
        Paragraph::new(msg.as_str())
            .block(Block::default().borders(Borders::ALL).border_style(style))
            .style(style),
        notif_area,
    );
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Format a float without trailing zeros (like Python's {:g})
fn fmt_h(h: f64) -> String {
    if h.fract() == 0.0 {
        format!("{}", h as i64)
    } else {
        let s = format!("{:.4}", h);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
