use std::io::{IsTerminal, stdin, stdout};
use std::time::Duration;

use anyhow::Result;
use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};
use console::style;
use dialoguer::{Confirm, theme::ColorfulTheme};
use indicatif::{ProgressBar, ProgressStyle};

use crate::error::PromptError;

#[derive(Debug, Clone, Copy)]
pub enum Tone {
    Error,
    Success,
    Info,
    Warning,
}

#[derive(Debug, Clone)]
pub struct TableRow {
    pub field: String,
    pub value: String,
}

impl TableRow {
    pub fn new(field: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            value: value.into(),
        }
    }
}

pub fn banner(tone: Tone, message: &str) -> String {
    let label = match tone {
        Tone::Error => style("error").red().bold(),
        Tone::Success => style("success").green().bold(),
        Tone::Info => style("info").cyan().bold(),
        Tone::Warning => style("warning").yellow().bold(),
    };

    format!("{label} {}", style(message).bold())
}

pub fn render_table(rows: &[TableRow]) -> String {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec![
        Cell::new("Field").add_attribute(Attribute::Bold),
        Cell::new("Value").add_attribute(Attribute::Bold),
    ]);
    for row in rows {
        table.add_row(vec![
            Cell::new(row.field.as_str()).fg(Color::Cyan),
            Cell::new(row.value.as_str()),
        ]);
    }

    table.to_string()
}

pub fn render_list(title: &str, items: &[String]) -> Option<String> {
    if items.is_empty() {
        return None;
    }

    let mut lines = vec![style(title).bold().to_string()];
    lines.extend(
        items
            .iter()
            .map(|item| format!("- {}", render_inline_markup(item))),
    );
    Some(lines.join("\n"))
}

pub fn render_steps(title: &str, steps: &[String]) -> Option<String> {
    if steps.is_empty() {
        return None;
    }

    let mut lines = vec![style(title).bold().to_string()];
    lines.extend(
        steps
            .iter()
            .enumerate()
            .map(|(index, step)| format!("{}. {}", index + 1, render_inline_markup(step))),
    );
    Some(lines.join("\n"))
}

pub fn join_blocks(blocks: Vec<String>) -> String {
    blocks
        .into_iter()
        .filter(|block| !block.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub struct Spinner {
    progress: Option<ProgressBar>,
}

impl Spinner {
    pub fn new(message: &str) -> Self {
        if !stdout().is_terminal() {
            return Self { progress: None };
        }

        let progress = ProgressBar::new_spinner();
        progress.set_style(
            ProgressStyle::with_template("{spinner} {msg}")
                .expect("spinner template should be valid"),
        );
        progress.enable_steady_tick(Duration::from_millis(80));
        progress.set_message(message.to_string());
        Self {
            progress: Some(progress),
        }
    }

    pub fn finish(self) {
        if let Some(progress) = self.progress {
            progress.finish_and_clear();
        }
    }
}

pub fn can_prompt() -> bool {
    stdin().is_terminal() && stdout().is_terminal()
}

pub fn confirm(prompt: &str, default: bool) -> Result<bool> {
    Ok(Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(default)
        .interact()
        .map_err(PromptError::from)?)
}

pub fn render_error(error: &anyhow::Error) -> String {
    let mut blocks = vec![format!(
        "{} {}",
        style("error").red().bold(),
        render_inline_markup(&error.to_string())
    )];

    let causes: Vec<String> = error
        .chain()
        .skip(1)
        .map(|cause| format!("- {}", render_inline_markup(&cause.to_string())))
        .collect();

    if !causes.is_empty() {
        blocks.push(format!(
            "{}\n{}",
            style("Caused by").bold(),
            causes.join("\n")
        ));
    }

    join_blocks(blocks)
}

fn render_inline_markup(text: &str) -> String {
    let mut rendered = String::new();
    let mut segments = text.split('`').peekable();
    let mut is_code = false;

    while let Some(segment) = segments.next() {
        let is_last = segments.peek().is_none();
        if is_code && is_last {
            rendered.push('`');
            rendered.push_str(segment);
        } else if is_code {
            rendered.push_str(&style(segment).cyan().bold().to_string());
        } else {
            rendered.push_str(segment);
        }
        is_code = !is_code;
    }

    rendered
}

#[cfg(test)]
mod tests {
    use console::strip_ansi_codes;

    use super::{render_error, render_inline_markup};

    #[test]
    fn renders_inline_code_without_literal_backticks() {
        let rendered = render_inline_markup("Run `autoloop status` to inspect the workspace");
        let plain = strip_ansi_codes(&rendered).to_string();

        assert_eq!(plain, "Run autoloop status to inspect the workspace");
        assert!(!plain.contains('`'));
    }

    #[test]
    fn preserves_unmatched_backticks_as_literal_text() {
        let rendered = render_inline_markup("Run `autoloop status");
        let plain = strip_ansi_codes(&rendered).to_string();

        assert_eq!(plain, "Run `autoloop status");
    }

    #[test]
    fn renders_error_without_default_rust_prefix() {
        let error = anyhow::anyhow!("run `autoloop init` first");
        let rendered = render_error(&error);
        let plain = strip_ansi_codes(&rendered).to_string();

        assert!(plain.starts_with("error run autoloop init first"));
        assert!(!plain.contains("Error:"));
    }
}
