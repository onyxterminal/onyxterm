use std::path::Path;
use terminal_core::ForegroundJob;

pub const MAX_TITLE_CHARS: usize = 40;
const CWD_TAIL_SEGMENTS: usize = 2;
const HOME_MARK: &str = "~";
const ELLIPSIS: &str = "\u{2026}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OscTitle {
    pub text: String,
    pub owner: Option<i32>,
}

pub struct TitleSource<'a> {
    pub name: Option<&'a str>,
    pub job: Option<&'a ForegroundJob>,
    pub osc: Option<&'a OscTitle>,
    pub cwd: &'a Path,
    pub home: &'a Path,
}

pub fn display_title(source: &TitleSource) -> String {
    if let Some(name) = source.name {
        return name.to_string();
    }
    match source.job {
        Some(job) => truncate_title(&job_title(job, source.osc)),
        None => short_cwd(source.cwd, source.home),
    }
}

pub fn title_tooltip(source: &TitleSource) -> String {
    let cwd = source.cwd.display();
    let full_title = source.name.map(str::to_string).or_else(|| source.job.map(|job| job_title(job, source.osc)));
    match full_title {
        Some(title) => format!("{title}\n{cwd}"),
        None => cwd.to_string(),
    }
}

fn job_title(job: &ForegroundJob, osc: Option<&OscTitle>) -> String {
    osc.filter(|osc| osc.owner == Some(job.process_group) && !osc.text.trim().is_empty())
        .map(|osc| osc.text.clone())
        .unwrap_or_else(|| command_line(&job.argv))
}

pub fn command_line(argv: &[String]) -> String {
    let Some((program, args)) = argv.split_first() else {
        return String::new();
    };
    let program = Path::new(program).file_name().map(|name| name.to_string_lossy()).unwrap_or(program.into());
    std::iter::once(program.as_ref()).chain(args.iter().map(String::as_str)).collect::<Vec<_>>().join(" ")
}

pub fn truncate_title(text: &str) -> String {
    if text.chars().count() <= MAX_TITLE_CHARS {
        return text.to_string();
    }
    let kept: String = text.chars().take(MAX_TITLE_CHARS - 1).collect();
    format!("{}{ELLIPSIS}", kept.trim_end())
}

pub fn short_cwd(path: &Path, home: &Path) -> String {
    let (prefix, relative) = match path.strip_prefix(home) {
        Ok(relative) => (HOME_MARK.to_string(), relative),
        Err(_) => (String::new(), path.strip_prefix("/").unwrap_or(path)),
    };
    let segments: Vec<String> =
        relative.components().map(|part| part.as_os_str().to_string_lossy().into_owned()).collect();
    let tail_start = segments.len().saturating_sub(CWD_TAIL_SEGMENTS);
    let shown = segments[tail_start..].join("/");
    match (prefix.is_empty(), tail_start > 0, shown.is_empty()) {
        (false, _, true) => prefix,
        (false, false, false) => format!("{prefix}/{shown}"),
        (false, true, false) => format!("{prefix}/{ELLIPSIS}/{shown}"),
        (true, false, _) => format!("/{shown}"),
        (true, true, _) => format!("/{ELLIPSIS}/{shown}"),
    }
}
