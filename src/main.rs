use std::os::unix::prelude::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use std::{env, io};

use arc_swap::ArcSwap;
use atomic::Atomic;
use compact_strings::CompactStrings;
use config::Config;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use interface::{check_installed, format_results, get_info, list, search};
use message::Message;
use mode::Mode;
use nohash_hasher::IntSet;
use parking_lot::Mutex;
use shown::Shown;
use tui::style::{Color, Modifier, Style};
use tui::widgets::{BorderType, Wrap};
use tui::{
    backend::CrosstermBackend,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Terminal,
};

mod config;
mod interface;
mod macros;
mod message;
mod mode;
mod shown;

#[tokio::main]
async fn main() -> Result<(), io::Error> {
    let args = Config::new(env::args());

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut query = args.query.unwrap_or_default();
    let shown = Arc::new(ArcSwap::new(Arc::new(Shown::Few(Vec::new()))));
    let mode = Arc::new(Atomic::new(Mode::Insert));
    let mut current: usize = 0;
    let mut selected = IntSet::default();
    let mut info_scroll: u16 = 0;
    let info = Arc::new(Mutex::new(Vec::new()));
    let redraw = Arc::new(AtomicBool::new(true));
    let mut insert_pos: u16;

    let all_packages: Arc<OnceLock<&'static CompactStrings>> = Arc::new(OnceLock::new());
    let installed: Arc<OnceLock<IntSet<usize>>> = Arc::new(OnceLock::new());
    let error_msg = Arc::new(Atomic::new(Message::TrySearch));

    let shown_len = || {
        (*shown)
            .load()
            .len()
            .unwrap_or(all_packages.get().map(|p| p.len()).unwrap_or_default())
    };
    let real_idx = |idx| (*shown).load().get(idx).unwrap_or(idx);

    let mut _search_task = None;

    {
        let query = query.clone();
        insert_pos = query.len() as u16;
        let mode = mode.clone();
        let shown = shown.clone();
        let error_msg = error_msg.clone();
        let redraw = redraw.clone();
        let all_packages = all_packages.clone();
        let installed = installed.clone();

        _search_task = Some(tokio::spawn(async move {
            if query.is_empty() {
                error_msg.store(Message::ListingPackages, Ordering::Relaxed);
            } else {
                error_msg.store(Message::Searching, Ordering::Relaxed);
            }

            redraw.store(true, Ordering::Relaxed);

            if all_packages.get().is_none() {
                let result = list().await;
                all_packages.get_or_init(|| result);
            }

            if installed.get().is_none() {
                let result = check_installed(all_packages.get().unwrap()).await;
                installed.get_or_init(|| result);
            }

            shown.store(search(&query, all_packages.get().unwrap()).into());

            if !(*shown).load().is_empty() {
                mode.store(Mode::Select, Ordering::Relaxed);
            } else {
                error_msg.store(Message::NoResults, Ordering::Relaxed);
            }
            redraw.store(true, Ordering::Relaxed);
        }));
    }

    terminal.clear()?;

    loop {
        let mut line = current;
        let size = terminal.size();
        let Ok(size) = size else {
            continue;
        };

        if size.height < 10 || size.width < 10 {
            continue;
        }

        let per_page = (size.height - 5) as usize;
        let page = current / per_page;
        let skipped = page * per_page;
        line -= skipped;

        if redraw.swap(false, Ordering::Relaxed) {
            let shown_len_str_len = (shown_len() + 1).ilog10() as usize + 1;

            let formatted_shown = all_packages
                .get()
                .and_then(|all_packages| {
                    installed.get().map(|installed| {
                        format_results(
                            all_packages,
                            shown.clone(),
                            current,
                            &selected,
                            size.height as usize,
                            shown_len_str_len,
                            skipped,
                            installed,
                        )
                    })
                })
                .unwrap_or_default();

            if info.lock().is_empty() && !(*shown).load().is_empty() {
                let shown = shown.clone();
                let redraw = redraw.clone();
                let info = info.clone();
                let installed = installed.clone();
                let all_packages = all_packages.clone();
                if let Some(search_thread) = _search_task {
                    search_thread.abort();
                }
                _search_task = Some(tokio::spawn(async move {
                    let real_idx = (*shown).load().get(current).unwrap_or(current);
                    let newinfo = get_info(
                        all_packages.get().unwrap(),
                        real_idx,
                        installed.get().unwrap(),
                    )
                    .await;
                    *info.lock() = newinfo;
                    redraw.store(true, Ordering::Relaxed);
                }))
            }

            terminal.draw(|s| {
                let search_color;
                let shown_color;
                let bold_search_style;
                if matches!(mode.load(Ordering::Relaxed), Mode::Insert) {
                    search_color = Color::White;
                    shown_color = Color::Gray;
                    bold_search_style = Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(search_color)
                } else {
                    search_color = Color::Gray;
                    shown_color = Color::White;
                    bold_search_style = Style::default().fg(search_color);
                };

                let para = Paragraph::new(Line::from(vec![
                    Span::styled(" Search: ", bold_search_style),
                    Span::styled(
                        query
                            .chars()
                            .skip((query.len() + 13).saturating_sub(size.width as usize))
                            .take(size.width.saturating_sub(13) as usize)
                            .collect::<String>(),
                        Style::default().fg(search_color),
                    ),
                ]))
                .block(
                    Block::default()
                        .title(Span::styled(" breweri ", bold_search_style))
                        .title_alignment(Alignment::Center)
                        .border_style(Style::default().fg(search_color))
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded),
                )
                .alignment(Alignment::Left);
                let mut area = Rect {
                    x: 0,
                    y: 0,
                    width: size.width,
                    height: 3,
                };
                s.render_widget(para, area);

                let para = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(shown_color))
                    .border_type(BorderType::Rounded);
                area = Rect {
                    x: 0,
                    y: 3,
                    width: size.width,
                    height: size.height - 3,
                };
                s.render_widget(para, area);

                let para = Paragraph::new(formatted_shown).alignment(Alignment::Left);
                area = Rect {
                    x: 2,
                    y: 4,
                    width: size.width - 2,
                    height: size.height - 4,
                };
                s.render_widget(para, area);

                if (*shown).load().is_empty() {
                    let area = Rect {
                        x: size.width / 4 + 1,
                        y: size.height / 2 - 2,
                        width: size.width / 2,
                        height: 4,
                    };
                    let no_shown = Paragraph::new(error_msg.load(Ordering::Relaxed).as_str())
                        .block(
                            Block::default()
                                .title(Span::styled(
                                    " No Results ",
                                    Style::default().add_modifier(Modifier::BOLD),
                                ))
                                .title_alignment(Alignment::Center)
                                .borders(Borders::ALL)
                                .border_type(BorderType::Rounded),
                        )
                        .wrap(Wrap { trim: true })
                        .alignment(Alignment::Center);
                    s.render_widget(Clear, area);
                    s.render_widget(no_shown, area);
                } else {
                    area = Rect {
                        x: size.width / 2,
                        y: 4,
                        width: size.width / 2 - 1,
                        height: size.height - 5,
                    };
                    let border = Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(shown_color))
                        .border_type(BorderType::Rounded);
                    s.render_widget(Clear, area);
                    s.render_widget(border, area);

                    let (info, no_info) = {
                        let info_lock = info.lock();
                        (info_lock.clone(), info_lock.is_empty())
                    };

                    area = Rect {
                        x: size.width / 2 + 2,
                        y: 5,
                        width: size.width / 2 - 5,
                        height: 2 + no_info as u16 * 2,
                    };
                    let actions = Paragraph::new(if no_info {
                        vec![
                            Line::from(Span::styled(
                                "Press ENTER to (re)install selected packages",
                                Style::default()
                                    .fg(Color::Green)
                                    .add_modifier(Modifier::BOLD),
                            )),
                            Line::from(Span::styled(
                                "Press Shift-R to uninstall selected packages",
                                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                            )),
                            Line::default(),
                            Line::from(Span::styled(
                                "Finding info...",
                                Style::default().fg(Color::Gray),
                            )),
                        ]
                    } else {
                        vec![
                            Line::from(Span::styled(
                                "Press ENTER to (re)install selected packages",
                                Style::default()
                                    .fg(Color::Green)
                                    .add_modifier(Modifier::BOLD),
                            )),
                            Line::from(Span::styled(
                                "Press Shift-R to uninstall selected packages",
                                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                            )),
                        ]
                    })
                    .alignment(Alignment::Left);
                    s.render_widget(actions, area);

                    area = Rect {
                        x: size.width / 2 + 2,
                        y: 8 - no_info as u16,
                        width: size.width / 2 - 5,
                        height: size.height - 10 - no_info as u16,
                    };

                    let info = Paragraph::new(info)
                        .wrap(Wrap { trim: false })
                        .scroll((info_scroll, 0));
                    s.render_widget(info, area);
                }
            })?;

            match mode.load(Ordering::Relaxed) {
                Mode::Insert => {
                    terminal.set_cursor((insert_pos + 10).min(size.width.saturating_sub(3)), 1)?;
                    terminal.show_cursor()?;
                }
                Mode::Select => {
                    terminal.set_cursor(2, line as u16 + 4)?;
                    terminal.hide_cursor()?;
                }
            }
        }

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }

        let e = event::read()?;

        let Event::Key(k) = e else {
            if matches!(e, Event::Resize(..)) {
                redraw.store(true, Ordering::Relaxed);
            };
            continue;
        };

        match mode.load(Ordering::Relaxed) {
            Mode::Insert => match k.code {
                KeyCode::Esc => {
                    if !(*shown).load().is_empty() {
                        current = 0;
                        redraw.store(true, Ordering::Relaxed);
                        mode.store(Mode::Select, Ordering::Relaxed);
                    }
                }
                KeyCode::Left => {
                    if k.modifiers.contains(KeyModifiers::CONTROL) {
                        let boundary = last_word_end(query.as_bytes(), insert_pos);
                        insert_pos = boundary as u16;
                    } else if insert_pos > 0 {
                        insert_pos -= 1;
                    } else {
                        insert_pos = query.len() as u16;
                    }
                    redraw.store(true, Ordering::Relaxed);
                }
                KeyCode::Right => {
                    if k.modifiers.contains(KeyModifiers::CONTROL) {
                        let boundary = next_word_start(query.as_bytes(), insert_pos);
                        insert_pos = boundary as u16;
                    } else if (insert_pos as usize) < query.len() {
                        insert_pos += 1;
                    } else {
                        insert_pos = 0;
                    }
                    redraw.store(true, Ordering::Relaxed);
                }
                KeyCode::Up | KeyCode::Home => {
                    insert_pos = 0;
                    redraw.store(true, Ordering::Relaxed);
                }
                KeyCode::Down | KeyCode::End => {
                    insert_pos = query.len() as u16;
                    redraw.store(true, Ordering::Relaxed);
                }
                KeyCode::Backspace => {
                    if insert_pos != 0 {
                        query.remove(insert_pos as usize - 1);
                        insert_pos -= 1;
                        redraw.store(true, Ordering::Relaxed);
                    }
                }
                KeyCode::Char(c) => match c {
                    'c' if k.modifiers == KeyModifiers::CONTROL => {
                        disable_raw_mode()?;
                        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

                        if let Some(search_thread) = _search_task {
                            search_thread.abort();
                        }

                        return Ok(());
                    }
                    'w' if k.modifiers == KeyModifiers::CONTROL => {
                        let boundary = last_word_end(query.as_bytes(), insert_pos);
                        query = query[..boundary].to_string() + &query[insert_pos as usize..];
                        insert_pos = boundary as u16;
                        redraw.store(true, Ordering::Relaxed);
                    }
                    _ => {
                        query.insert(insert_pos as usize, c);
                        insert_pos += 1;
                        redraw.store(true, Ordering::Relaxed);
                    }
                },
                KeyCode::Enter => {
                    info.lock().clear();
                    current = 0;
                    redraw.store(true, Ordering::Relaxed);
                    shown.store(Shown::Few(Vec::new()).into());
                    let mode = mode.clone();
                    let shown = shown.clone();
                    let error_msg = error_msg.clone();
                    let redraw = redraw.clone();
                    let query = query.clone();
                    let all_packages = all_packages.clone();
                    let installed = installed.clone();
                    if let Some(search_thread) = _search_task {
                        search_thread.abort();
                    }
                    _search_task = Some(tokio::spawn(async move {
                        error_msg.store(Message::Searching, Ordering::Relaxed);

                        if all_packages.get().is_none() {
                            let result = list().await;
                            all_packages.get_or_init(|| result);
                        }

                        if installed.get().is_none() {
                            let result = check_installed(all_packages.get().unwrap()).await;
                            installed.get_or_init(|| result);
                        }

                        shown.store(search(&query, all_packages.get().unwrap()).into());

                        if !(*shown).load().is_empty() {
                            mode.store(Mode::Select, Ordering::Relaxed);
                        } else {
                            error_msg.store(Message::NoResults, Ordering::Relaxed);
                        }
                        redraw.store(true, Ordering::Relaxed);
                    }));
                }
                _ => redraw.store(true, Ordering::Relaxed),
            },
            Mode::Select => match k.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if k.modifiers == KeyModifiers::CONTROL {
                        if info_scroll > 0 {
                            info_scroll -= 1;
                            redraw.store(true, Ordering::Relaxed);
                        }
                    } else {
                        if current > 0 {
                            current -= 1;
                        } else {
                            current = shown_len() - 1;
                        }
                        info.lock().clear();
                        redraw.store(true, Ordering::Relaxed);
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if k.modifiers == KeyModifiers::CONTROL {
                        if !info.lock().is_empty() {
                            info_scroll += 1;
                            redraw.store(true, Ordering::Relaxed);
                        }
                    } else {
                        let result_count = shown_len();

                        if result_count > 1 && current < result_count - 1 {
                            current += 1;
                        } else {
                            current = 0;
                        }
                        info.lock().clear();
                        redraw.store(true, Ordering::Relaxed);
                    }
                }
                KeyCode::Esc => {
                    insert_pos = query.len() as u16;
                    redraw.store(true, Ordering::Relaxed);
                    mode.store(Mode::Insert, Ordering::Relaxed);
                }
                KeyCode::Left | KeyCode::PageUp | KeyCode::Char('h') => {
                    let result_count = shown_len() - 1;
                    if result_count > per_page {
                        if current >= per_page {
                            current -= per_page;
                        } else if current % per_page == 0 {
                            current = result_count / per_page * per_page;
                        } else {
                            current = current / per_page * per_page;
                        }
                        info.lock().clear();
                        redraw.store(true, Ordering::Relaxed);
                    }
                }
                KeyCode::Right | KeyCode::PageDown | KeyCode::Char('l') => {
                    let shown_len = shown_len();

                    if shown_len > per_page {
                        if current == shown_len - 1 {
                            current = 0;
                        } else if current + per_page > shown_len - 1 {
                            current = shown_len - 1;
                        } else {
                            current += per_page;
                        }
                        info.lock().clear();
                        redraw.store(true, Ordering::Relaxed);
                    }
                }
                KeyCode::Home | KeyCode::Char('g') if current != 0 => {
                    info.lock().clear();
                    current = 0;
                    redraw.store(true, Ordering::Relaxed);
                }
                KeyCode::End | KeyCode::Char('G') if current != shown_len() - 1 => {
                    info.lock().clear();
                    current = shown_len() - 1;
                    redraw.store(true, Ordering::Relaxed);
                }
                KeyCode::Char(c) => match c {
                    ' ' => {
                        let real_current = real_idx(current);
                        if selected.contains(&real_current) {
                            selected.remove(&real_current);
                        } else {
                            selected.insert(real_current);
                        }
                        redraw.store(true, Ordering::Relaxed);
                    }
                    'i' | '/' => {
                        insert_pos = query.len() as u16;
                        redraw.store(true, Ordering::Relaxed);
                        mode.store(Mode::Insert, Ordering::Relaxed);
                    }
                    'q' => {
                        disable_raw_mode()?;
                        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

                        if let Some(search_thread) = _search_task {
                            search_thread.abort();
                        }

                        return Ok(());
                    }
                    'c' if k.modifiers.contains(KeyModifiers::CONTROL) => {
                        disable_raw_mode()?;
                        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

                        if let Some(search_thread) = _search_task {
                            search_thread.abort();
                        }

                        return Ok(());
                    }
                    'R' => {
                        let mut has_any = false;
                        let mut cmd = std::process::Command::new("brew");
                        cmd.arg("remove");
                        if selected.is_empty()
                            && installed.get().unwrap().contains(&real_idx(current))
                        {
                            cmd.arg(&(all_packages.get().unwrap()[real_idx(current)]));
                            has_any = true;
                        } else {
                            for i in selected.iter() {
                                if installed.get().unwrap().contains(i) {
                                    cmd.arg(&(all_packages.get().unwrap()[*i]));
                                    has_any = true;
                                }
                            }
                        }

                        if !has_any {
                            continue;
                        }

                        disable_raw_mode()?;
                        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                        terminal.show_cursor()?;

                        cmd.exec();

                        return Ok(());
                    }

                    _ => redraw.store(true, Ordering::Relaxed),
                },
                KeyCode::Enter => {
                    disable_raw_mode()?;
                    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                    terminal.show_cursor()?;

                    if let Some(search_thread) = _search_task {
                        search_thread.abort();
                    }

                    let mut cmd = std::process::Command::new("brew");
                    cmd.arg("reinstall");
                    if selected.is_empty() {
                        cmd.arg(&(all_packages.get().unwrap()[real_idx(current)]));
                    } else {
                        for i in selected.iter() {
                            cmd.arg(&(all_packages.get().unwrap()[*i]));
                        }
                    }

                    cmd.exec();

                    return Ok(());
                }
                _ => redraw.store(true, Ordering::Relaxed),
            },
        }
    }
}

#[inline(always)]
const fn is_word_boundary(byte: &u8) -> bool {
    matches!(*byte, b' ' | b'-' | b'_')
}

fn last_word_end(bytes: &[u8], pos: u16) -> usize {
    bytes
        .iter()
        .take(pos.saturating_sub(1) as usize)
        .rposition(is_word_boundary)
        .map(|i| i + 1)
        .unwrap_or_default()
}

fn next_word_start(bytes: &[u8], pos: u16) -> usize {
    let pos = pos as usize;
    bytes
        .iter()
        .skip(pos)
        .position(is_word_boundary)
        .map(|i| i + pos + 1)
        .unwrap_or(bytes.len())
}
