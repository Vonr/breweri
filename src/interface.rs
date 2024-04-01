use std::{borrow::Cow, collections::HashSet, process::Stdio, sync::Arc, time::Duration};

use arc_swap::ArcSwap;
use compact_strings::CompactStrings;
use nohash_hasher::IntSet;
use tokio::{join, process::Command, time::sleep};
use tui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::shown::Shown;

pub async fn list() -> &'static mut CompactStrings {
    let formulae = Command::new("curl")
        .arg("-s")
        .arg("https://formulae.brew.sh/api/formula.json")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let formulae_jq = Command::new("jq")
        .arg("-r")
        .arg(".[]|.name")
        .stdin(Stdio::from(
            formulae.stdout.unwrap().into_owned_fd().unwrap(),
        ))
        .stdout(Stdio::piped())
        .spawn();
    let casks = Command::new("curl")
        .arg("-s")
        .arg("https://formulae.brew.sh/api/cask.json")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let casks_jq = Command::new("jq")
        .arg("-r")
        .arg(".[]|.full_token")
        .stdin(Stdio::from(casks.stdout.unwrap().into_owned_fd().unwrap()))
        .stdout(Stdio::piped())
        .spawn();

    let out = Box::leak(Box::new(CompactStrings::with_capacity(16 * 16384, 16384)));

    let Ok(formulae_jq) = formulae_jq else {
        return out;
    };

    let Ok(casks_jq) = casks_jq else {
        return out;
    };

    let (formulae, casks) = join!(formulae_jq.wait_with_output(), casks_jq.wait_with_output());

    let Ok(formulae) = formulae else {
        return out;
    };

    let Ok(casks) = casks else {
        return out;
    };

    let mut buf = Vec::with_capacity(128);

    for byte in formulae.stdout.into_iter().chain(casks.stdout.into_iter()) {
        if byte != b'\n' {
            buf.push(byte);
            continue;
        }

        if let Ok(s) = std::str::from_utf8(&buf) {
            out.push(s);
        }

        buf.clear();
    }

    out.shrink_to_fit();
    out.shrink_meta_to_fit();

    out
}

pub fn search(query: &str, packages: &CompactStrings) -> Shown {
    if query.is_empty() {
        Shown::All
    } else {
        Shown::Few(
            packages
                .iter()
                .enumerate()
                .filter(|(_, package)| package.contains(query))
                .map(|(i, _)| i)
                .collect(),
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub fn format_results<'line>(
    packages: &'static CompactStrings,
    shown: Arc<ArcSwap<Shown>>,
    current: usize,
    selected: &IntSet<usize>,
    height: usize,
    pad_to: usize,
    skip: usize,
    installed: &IntSet<usize>,
) -> Vec<Line<'line>> {
    use crate::{cows, style};

    const INDEX_STYLE: Style = style!(Color::Gray);
    const INSTALLED_STYLE: Style = style! {
        fg: Color::Green,
        mod: Modifier::BOLD,
    };
    const INSTALLED_SELECTED_STYLE: Style = style! {
        fg: Color::Yellow,
        bg: Color::Red,
        mod: Modifier::BOLD,
    };
    const UNINSTALLED_STYLE: Style = style! {
        fg: Color::LightBlue,
        mod: Modifier::BOLD,
    };
    const UNINSTALLED_SELECTED_STYLE: Style = style! {
        fg: Color::Blue,
        bg: Color::Red,
        mod: Modifier::BOLD,
    };
    const DEFAULT_STYLE: Style = style!();

    const PADDINGS: [Cow<'static, str>; 16] = cows!(
        " ",
        "  ",
        "   ",
        "    ",
        "     ",
        "      ",
        "       ",
        "        ",
        "         ",
        "          ",
        "           ",
        "            ",
        "             ",
        "              ",
        "               ",
        "                "
    );

    const SELECTED: Span = Span {
        content: Cow::Borrowed("!"),
        style: style! { fg: Color::Yellow, mod: Modifier::BOLD, },
    };

    match (*shown).load().get_vec() {
        Some(shown) => shown
            .iter()
            .skip(skip)
            .take(height - 5)
            .copied()
            .enumerate()
            .map(|(i, package_idx)| {
                let real_index = shown[skip + i];
                let index = i + skip + 1;

                let index_span = Span::styled(index.to_string(), INDEX_STYLE);
                let padding_span = Span {
                    content: PADDINGS[pad_to - index.ilog10() as usize].clone(),
                    style: DEFAULT_STYLE,
                };
                let line_span = Span::styled(
                    &packages[package_idx],
                    match (installed.contains(&real_index), current == index - 1) {
                        (true, true) => INSTALLED_SELECTED_STYLE,
                        (true, false) => INSTALLED_STYLE,
                        (false, true) => UNINSTALLED_SELECTED_STYLE,
                        (false, false) => UNINSTALLED_STYLE,
                    },
                );

                let spans = if selected.contains(&real_index) {
                    vec![index_span, padding_span, line_span, SELECTED]
                } else {
                    vec![index_span, padding_span, line_span]
                };
                Line::from(spans)
            })
            .collect(),
        None => packages
            .iter()
            .enumerate()
            .skip(skip)
            .take(height - 5)
            .map(|(i, line)| {
                let index_span = Span::styled((i + 1).to_string(), INDEX_STYLE);
                let padding_span = Span {
                    content: PADDINGS[pad_to - (i + 1).ilog10() as usize].clone(),
                    style: DEFAULT_STYLE,
                };
                let line_span = Span::styled(
                    line,
                    match (installed.contains(&i), current == i) {
                        (true, true) => INSTALLED_SELECTED_STYLE,
                        (true, false) => INSTALLED_STYLE,
                        (false, true) => UNINSTALLED_SELECTED_STYLE,
                        (false, false) => UNINSTALLED_STYLE,
                    },
                );

                let spans = if selected.contains(&i) {
                    vec![index_span, padding_span, line_span, SELECTED]
                } else {
                    vec![index_span, padding_span, line_span]
                };
                Line::from(spans)
            })
            .collect(),
    }
}

pub async fn get_info<'line>(
    all_packages: &CompactStrings,
    index: usize,
    installed_cache: &IntSet<usize>,
) -> Vec<Line<'line>> {
    if index >= all_packages.len() {
        return Vec::new();
    }

    let mut cmd = Command::new("brew");

    if !installed_cache.contains(&index) {
        // Debounce so that we don't spam requests
        sleep(Duration::from_millis(200)).await;
    };
    cmd.arg("info");

    cmd.arg(&all_packages[index]);

    let output = cmd_output(cmd).await;
    let lines = output.lines().map(ToOwned::to_owned).collect::<Vec<_>>();

    const KEY_STYLE: Style = Style {
        fg: None,
        bg: None,
        underline_color: None,
        add_modifier: Modifier::BOLD,
        sub_modifier: Modifier::empty(),
    };

    let mut info = Vec::with_capacity(lines.len());
    for mut line in lines {
        if line.starts_with(' ') {
            info.push(Line::from(line));
            continue;
        }

        if let Some(idx) = line.find(':') {
            let value = line.split_off(idx + 1);
            info.push(Line::from(vec![
                Span::styled(line, KEY_STYLE),
                Span::raw(value),
            ]));
        }
    }

    info
}

pub async fn check_installed(packages: &CompactStrings) -> IntSet<usize> {
    const FORMULAE_PATH: &str = "/usr/local/Cellar";
    const CASKS_PATH: &str = "/usr/local/Caskroom";

    let mut out = IntSet::default();
    let Ok(formulae) = std::fs::read_dir(FORMULAE_PATH) else {
        return out;
    };
    let Ok(casks) = std::fs::read_dir(CASKS_PATH) else {
        return out;
    };

    let mut set = HashSet::new();
    for entry in formulae.chain(casks).filter_map(Result::ok) {
        let Ok(ft) = entry.file_type() else {
            continue;
        };

        if !ft.is_dir() {
            continue;
        }

        set.insert(entry.file_name().into_string().unwrap());
    }

    for (pos, _) in packages
        .iter()
        .enumerate()
        .filter(|(_, p)| set.contains(*p))
    {
        out.insert(pos);
    }
    out
}

async fn cmd_output(mut cmd: Command) -> String {
    cmd.output()
        .await
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .unwrap_or_default()
}
