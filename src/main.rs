mod bar;
mod color;
mod cost;
mod icons;

use clap::Parser;
use color::{Color, ColorMode, Style};
use serde::Deserialize;
use std::io::Read;

#[derive(Parser)]
#[command(name = "constatus", about = "Configurable status line for Claude Code")]
struct Cli {
    /// Format string with placeholders: {model}, {dir}, {branch}, {context}, {bar}, {cache},
    /// {input}, {output}, {cost}, {duration}.
    /// Sections separated by ' | '. Prefix with '?' to hide when empty.
    #[arg(short, long)]
    format: Option<String>,

    /// Preset format profile
    #[arg(short, long, default_value = "default")]
    preset: Preset,

    /// Fallback text when no data is available
    #[arg(short = 'F', long, default_value = "Claude Ready")]
    fallback: String,

    /// Separator between sections
    #[arg(short, long, default_value = " │ ")]
    separator: String,

    /// Color mode
    #[arg(short, long, default_value = "auto")]
    color: ColorMode,

    /// Show Nerd Font icons
    #[arg(short = 'i', long, default_value_t = true)]
    icons: bool,

    /// Disable Nerd Font icons
    #[arg(long)]
    no_icons: bool,

    /// Progress bar width (chars)
    #[arg(short = 'w', long, default_value_t = 10)]
    bar_width: usize,
}

#[derive(Clone, Copy, Default)]
enum Preset {
    Minimal,
    #[default]
    Default,
    Full,
}

impl std::str::FromStr for Preset {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "minimal" | "min" => Ok(Self::Minimal),
            "default" => Ok(Self::Default),
            "full" => Ok(Self::Full),
            _ => Err(format!("unknown preset: {s} (use minimal, default, full)")),
        }
    }
}

impl Preset {
    fn format_str(self) -> &'static str {
        match self {
            Self::Minimal => "{model} | {dir} | ?{branch}",
            Self::Default => {
                "{model} | {dir} | ?{branch} | {bar} {context}% | ?{cache} cached | ?{cost}"
            }
            Self::Full => {
                "{model} | {dir} | ?{branch} | {bar} {context}% | ?{cache} cached | ?In: {input} | ?Out: {output} | ?{cost} | ?{duration}"
            }
        }
    }
}

#[derive(Deserialize, Default, Debug)]
struct StatusInput {
    model: Option<ModelInfo>,
    workspace: Option<Workspace>,
    context_window: Option<ContextWindow>,
    #[serde(default)]
    conversation: Option<Conversation>,
}

#[derive(Deserialize, Default, Debug)]
struct ModelInfo {
    display_name: Option<String>,
}

#[derive(Deserialize, Default, Debug)]
struct Workspace {
    current_dir: Option<String>,
}

#[derive(Deserialize, Default, Debug)]
struct ContextWindow {
    remaining_percentage: Option<f64>,
    current_usage: Option<CurrentUsage>,
}

#[derive(Deserialize, Default, Debug)]
struct CurrentUsage {
    cache_read_input_tokens: Option<u64>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

#[derive(Deserialize, Default, Debug)]
struct Conversation {
    started_at: Option<String>,
}

fn main() {
    let cli = Cli::parse();
    let use_icons = cli.icons && !cli.no_icons;
    let style = Style::new(cli.color);

    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() || raw.trim().is_empty() {
        print!("{}", style.dim(&cli.fallback));
        return;
    }

    let status: StatusInput = match serde_json::from_str(&raw) {
        Ok(s) => s,
        Err(_) => {
            print!("{}", style.dim(&cli.fallback));
            return;
        }
    };

    let format_str = cli
        .format
        .as_deref()
        .unwrap_or_else(|| cli.preset.format_str());

    let fields = extract_fields(&status, &style, use_icons, cli.bar_width);

    let sections: Vec<&str> = format_str.split(" | ").collect();
    let mut parts: Vec<String> = Vec::new();

    for section in &sections {
        let optional = section.starts_with('?');
        let template = if optional { &section[1..] } else { section };

        let result = substitute(template, &fields);

        if optional && has_empty_placeholder(template, &fields) {
            continue;
        }

        parts.push(result);
    }

    if parts.is_empty() {
        print!("{}", style.dim(&cli.fallback));
    } else {
        let sep = style.dim(&cli.separator);
        print!("{}", parts.join(&sep));
    }
}

struct Fields {
    model: String,
    dir: String,
    branch: String,
    context: String,
    bar: String,
    cache: String,
    input: String,
    output: String,
    cost: String,
    duration: String,
}

fn extract_fields(
    status: &StatusInput,
    style: &Style,
    use_icons: bool,
    bar_width: usize,
) -> Fields {
    let model_raw = status
        .model
        .as_ref()
        .and_then(|m| m.display_name.as_deref())
        .unwrap_or_default();

    let model = if model_raw.is_empty() {
        String::new()
    } else {
        let icon = if use_icons {
            format!("{} ", icons::MODEL)
        } else {
            String::new()
        };
        style.bold_fg(model_color(model_raw), &format!("{icon}{model_raw}"))
    };

    let dir_raw = status
        .workspace
        .as_ref()
        .and_then(|w| w.current_dir.as_deref())
        .map(|d| d.rsplit('/').next().unwrap_or(d))
        .unwrap_or_default();

    let dir = if dir_raw.is_empty() {
        String::new()
    } else {
        let icon = if use_icons {
            format!("{} ", icons::FOLDER)
        } else {
            String::new()
        };
        style.fg(Color::Blue, &format!("{icon}{dir_raw}"))
    };

    let workspace_dir = status
        .workspace
        .as_ref()
        .and_then(|w| w.current_dir.as_deref());

    let branch_raw = workspace_dir
        .and_then(|dir| {
            std::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .current_dir(dir)
                .stderr(std::process::Stdio::null())
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        })
        .unwrap_or_default();

    let branch = if branch_raw.is_empty() {
        String::new()
    } else {
        let icon = if use_icons {
            format!("{} ", icons::BRANCH)
        } else {
            String::new()
        };
        style.fg(Color::Yellow, &format!("{icon}{branch_raw}"))
    };

    let remaining = status
        .context_window
        .as_ref()
        .and_then(|c| c.remaining_percentage);

    let used = remaining.map(|p| 100.0 - p);

    let context = used
        .map(|p| {
            let icon = if use_icons {
                format!("{} ", icons::context_icon(p))
            } else {
                String::new()
            };
            let text = format!("{icon}{p:.0}");
            style.fg(usage_color(p), &text)
        })
        .unwrap_or_default();

    let bar_str = used
        .map(|p| {
            let raw_bar = bar::render(p, bar_width);
            style.fg(usage_color(p), &raw_bar)
        })
        .unwrap_or_default();

    let usage = status
        .context_window
        .as_ref()
        .and_then(|c| c.current_usage.as_ref());

    let cache_tokens = usage.and_then(|u| u.cache_read_input_tokens).unwrap_or(0);
    let in_tokens = usage.and_then(|u| u.input_tokens).unwrap_or(0);
    let out_tokens = usage.and_then(|u| u.output_tokens).unwrap_or(0);

    let cache = if cache_tokens > 0 {
        let icon = if use_icons {
            format!("{} ", icons::CACHE)
        } else {
            String::new()
        };
        style.fg(
            Color::Magenta,
            &format!("{icon}{}", format_tokens(cache_tokens)),
        )
    } else {
        String::new()
    };

    let input = if in_tokens > 0 {
        let icon = if use_icons {
            format!("{} ", icons::INPUT)
        } else {
            String::new()
        };
        style.fg(
            Color::Cyan,
            &format!("{icon}{}", format_tokens(in_tokens)),
        )
    } else {
        String::new()
    };

    let output = if out_tokens > 0 {
        let icon = if use_icons {
            format!("{} ", icons::OUTPUT)
        } else {
            String::new()
        };
        style.fg(
            Color::Green,
            &format!("{icon}{}", format_tokens(out_tokens)),
        )
    } else {
        String::new()
    };

    let cost_val = if in_tokens > 0 || out_tokens > 0 {
        cost::estimate(model_raw, in_tokens, out_tokens, cache_tokens)
    } else {
        0.0
    };
    let cost_str = if cost_val > 0.0 {
        let icon = if use_icons {
            format!("{} ", icons::COST)
        } else {
            String::new()
        };
        let formatted = cost::format_cost(cost_val);
        style.fg(Color::Yellow, &format!("{icon}{formatted}"))
    } else {
        String::new()
    };

    let duration = status
        .conversation
        .as_ref()
        .and_then(|c| c.started_at.as_deref())
        .and_then(parse_duration)
        .map(|d| {
            let icon = if use_icons {
                format!("{} ", icons::CLOCK)
            } else {
                String::new()
            };
            style.dim(&format!("{icon}{d}"))
        })
        .unwrap_or_default();

    Fields {
        model,
        dir,
        branch,
        context,
        bar: bar_str,
        cache,
        input,
        output,
        cost: cost_str,
        duration,
    }
}

fn substitute(template: &str, f: &Fields) -> String {
    template
        .replace("{model}", &f.model)
        .replace("{dir}", &f.dir)
        .replace("{branch}", &f.branch)
        .replace("{context}", &f.context)
        .replace("{bar}", &f.bar)
        .replace("{cache}", &f.cache)
        .replace("{input}", &f.input)
        .replace("{output}", &f.output)
        .replace("{cost}", &f.cost)
        .replace("{duration}", &f.duration)
}

fn has_empty_placeholder(template: &str, f: &Fields) -> bool {
    (template.contains("{model}") && f.model.is_empty())
        || (template.contains("{dir}") && f.dir.is_empty())
        || (template.contains("{branch}") && f.branch.is_empty())
        || (template.contains("{context}") && f.context.is_empty())
        || (template.contains("{bar}") && f.bar.is_empty())
        || (template.contains("{cache}") && f.cache.is_empty())
        || (template.contains("{input}") && f.input.is_empty())
        || (template.contains("{output}") && f.output.is_empty())
        || (template.contains("{cost}") && f.cost.is_empty())
        || (template.contains("{duration}") && f.duration.is_empty())
}

fn model_color(name: &str) -> Color {
    let lower = name.to_lowercase();
    if lower.contains("opus") {
        Color::Magenta
    } else if lower.contains("haiku") {
        Color::Green
    } else {
        Color::Cyan
    }
}

fn usage_color(pct: f64) -> Color {
    if pct >= 70.0 {
        Color::Red
    } else if pct >= 50.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

fn parse_duration(iso: &str) -> Option<String> {
    // Parse ISO 8601 timestamp, compute elapsed from now
    // Expected format: "2025-03-07T14:30:00Z" or similar
    let ts = iso
        .replace('T', " ")
        .replace('Z', "")
        .trim()
        .to_string();

    // Simple parser: try to get epoch seconds via date command fallback
    // For robustness, just parse the components directly
    let parts: Vec<&str> = ts.split(&['-', ' ', ':'][..]).collect();
    if parts.len() < 6 {
        return None;
    }

    let year: i64 = parts[0].parse().ok()?;
    let month: i64 = parts[1].parse().ok()?;
    let day: i64 = parts[2].parse().ok()?;
    let hour: i64 = parts[3].parse().ok()?;
    let min: i64 = parts[4].parse().ok()?;
    let sec: i64 = parts[5].split('.').next()?.parse().ok()?;

    // Rough epoch calculation (good enough for elapsed time display)
    let days_approx = (year - 1970) * 365 + (year - 1969) / 4 + month_days(month) + day - 1;
    let start_epoch = days_approx * 86400 + hour * 3600 + min * 60 + sec;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;

    let elapsed = now - start_epoch;
    if elapsed < 0 {
        return None;
    }

    Some(format_elapsed(elapsed as u64))
}

fn month_days(month: i64) -> i64 {
    match month {
        1 => 0,
        2 => 31,
        3 => 59,
        4 => 90,
        5 => 120,
        6 => 151,
        7 => 181,
        8 => 212,
        9 => 243,
        10 => 273,
        11 => 304,
        12 => 334,
        _ => 0,
    }
}

fn format_elapsed(secs: u64) -> String {
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    if hours > 0 {
        format!("{hours}h{mins:02}m")
    } else if mins > 0 {
        format!("{mins}m")
    } else {
        format!("{secs}s")
    }
}
