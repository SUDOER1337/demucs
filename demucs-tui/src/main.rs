use std::fs::OpenOptions;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Gauge, List, ListItem, Paragraph};
use ratatui::Frame;
use ratatui::Terminal;

// ─── Logging ─────────────────────────────────────────────────────────────────

/// Log a message to /tmp/demucs-tui.log with a millisecond timestamp.
macro_rules! log {
    ($($arg:tt)*) => {{
        let _ = $crate::log_line(&format!($($arg)*));
    }};
}

pub fn log_line(msg: &str) -> std::io::Result<()> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/demucs-tui.log")?;
    writeln!(f, "[{}] {}", ts, msg)
}

// ─── App State ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Browser,
    Settings,
    Progress,
    Results,
}

struct App {
    screen: Screen,
    current_dir: PathBuf,
    entries: Vec<PathBuf>,
    browser_scroll: usize,
    show_audio_only: bool,
    selected_model: usize,
    models: Vec<&'static str>,
    two_stems: bool,
    selected_stem: usize,
    stems: Vec<&'static str>,
    output_format: usize,
    formats: Vec<&'static str>,
    shifts: usize,
    progress_pct: f64,
    progress_msg: String,
    worker: Option<Child>,
    output_files: Vec<String>,
    output_dir: String,
    processed_track: String,
    worker_rx: Option<mpsc::Receiver<String>>,
    status_msg: String,
    searching: bool,
    search_query: String,
    search_matches: Vec<usize>,
    search_idx: usize,
    selected_device: usize,
    devices: Vec<&'static str>,
    _config_file: Option<tempfile::NamedTempFile>,
    worker_stderr_lines: Vec<String>,
    worker_last_msg_time: std::time::Instant,
}

impl App {
    fn new() -> Self {
        Self {
            screen: Screen::Browser,
            current_dir: dirs_audio_or_home(),
            entries: Vec::new(),
            browser_scroll: 0,
            show_audio_only: true,
            selected_model: 0,
            models: vec![
                "htdemucs",
                "htdemucs_ft",
                "htdemucs_6s",
                "hdemucs_mmi",
                "mdx_extra",
                "mdx",
            ],
            two_stems: false,
            selected_stem: 0,
            stems: vec!["vocals", "drums", "bass", "other"],
            output_format: 0,
            formats: vec!["wav", "flac", "mp3"],
            shifts: 1,
            progress_pct: 0.0,
            progress_msg: String::new(),
            worker: None,
            output_files: Vec::new(),
            output_dir: String::new(),
            processed_track: String::new(),
            worker_rx: None,
            status_msg: String::new(),
            searching: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_idx: 0,
            selected_device: 0,
            devices: vec!["auto", "cuda", "cpu"],
            _config_file: None,
            worker_stderr_lines: Vec::new(),
            worker_last_msg_time: std::time::Instant::now(),
        }
    }

    fn refresh_dir(&mut self) {
        self.entries = list_dir(&self.current_dir, self.show_audio_only);
        self.browser_scroll = 0;
        self.search_matches.clear();
        self.search_query.clear();
        self.search_idx = 0;
        self.searching = false;
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

const AUDIO_EXTS: &[&str] = &["mp3", "wav", "flac", "ogg", "opus", "m4a", "aac", "wma"];

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTS.contains(&e))
        .unwrap_or(false)
}

fn compute_search_matches(entries: &[PathBuf], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }
    let lower = query.to_lowercase();
    entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let name = e
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if name.contains(&lower) {
                Some(i)
            } else {
                None
            }
        })
        .collect()
}

fn list_dir(dir: &Path, audio_only: bool) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    if let Ok(read) = std::fs::read_dir(dir) {
        for entry in read.flatten() {
            let path = entry.path();
            if audio_only {
                if path.is_dir() || is_audio(&path) {
                    entries.push(path);
                }
            } else {
                entries.push(path);
            }
        }
    }
    entries.sort_by(|a, b| {
        let a_dir = a.is_dir();
        let b_dir = b.is_dir();
        if a_dir != b_dir {
            b_dir.cmp(&a_dir)
        } else {
            a.file_name().cmp(&b.file_name())
        }
    });
    entries
}

fn dirs_audio_or_home() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default();
    if cwd.join("demucs").is_dir() || cwd.join("separated").is_dir() {
        return cwd;
    }
    let candidates = ["~/Music", "~/music", "~/Downloads", "."];
    for dir in &candidates {
        let p = PathBuf::from(shellexpand::tilde(dir).as_ref());
        if p.is_dir() {
            return p;
        }
    }
    PathBuf::from(".")
}

fn find_repo_dir() -> PathBuf {
    log!("find_repo_dir: start | cwd={:?}", std::env::current_dir());

    // 1. Environment variable override (set by run.sh, justfile, or user)
    if let Ok(dir) = std::env::var("DEMUCS_REPO_DIR") {
        log!("find_repo_dir: step1 DEMUCS_REPO_DIR={:?}", dir);
        let p = PathBuf::from(dir);
        if p.join("demucs").is_dir() {
            log!("find_repo_dir: step1 FOUND at {:?}", p);
            return p;
        }
        log!(
            "find_repo_dir: step1 {:?} has no demucs/ subdir, continuing",
            p
        );
    } else {
        log!("find_repo_dir: step1 DEMUCS_REPO_DIR not set");
    }

    // 2. Detect from executable path
    let exe = std::env::current_exe().ok();
    if let Some(exe) = exe {
        log!("find_repo_dir: step2 exe={:?}", exe);
        let p = exe.parent().unwrap().to_path_buf();
        log!("find_repo_dir: step2 exe parent={:?}", p);
        // Running from target/debug or target/release (cargo build in-repo)
        if p.ends_with("debug") || p.ends_with("release") {
            let inner = p.parent().unwrap().parent().unwrap().to_path_buf();
            log!("find_repo_dir: step2 inner (target/../..)={:?}", inner);
            if inner.join("demucs").is_dir() {
                log!("find_repo_dir: step2 FOUND at {:?}", inner);
                return inner;
            }
            if let Some(parent) = inner.parent() {
                log!("find_repo_dir: step2 inner parent={:?}", parent);
                if parent.join("demucs").is_dir() {
                    log!("find_repo_dir: step2 FOUND at parent {:?}", parent);
                    return parent.to_path_buf();
                }
            }
        } else {
            log!("find_repo_dir: step2 not in debug/release dir, skip exe detect");
        }
    } else {
        log!("find_repo_dir: step2 current_exe() returned None");
    }

    // 3. Walk up from current working directory
    let mut cwd = std::env::current_dir().unwrap_or_default();
    log!("find_repo_dir: step3 walking up from {:?}", cwd);
    loop {
        if cwd.join("demucs").is_dir() {
            log!("find_repo_dir: step3 FOUND at {:?}", cwd);
            return cwd;
        }
        if !cwd.pop() {
            log!("find_repo_dir: step3 reached filesystem root, not found");
            break;
        }
    }

    // 4. Check ~/.demucs_repo config file
    if let Ok(home) = std::env::var("HOME") {
        let cfg = PathBuf::from(&home).join(".demucs_repo");
        log!("find_repo_dir: step4 checking config file {:?}", cfg);
        if let Ok(dir) = std::fs::read_to_string(&cfg) {
            let trimmed = dir.trim().to_string();
            log!("find_repo_dir: step4 config file contains {:?}", trimmed);
            let p = PathBuf::from(&trimmed);
            if p.join("demucs").is_dir() {
                log!("find_repo_dir: step4 FOUND at {:?}", p);
                return p;
            }
            log!(
                "find_repo_dir: step4 config path {:?} has no demucs/, ignoring",
                p
            );
        } else {
            log!("find_repo_dir: step4 config file not found or unreadable");
        }
    }

    // 5. Walk up from home directory
    if let Ok(home) = std::env::var("HOME") {
        let mut p = PathBuf::from(&home);
        log!("find_repo_dir: step5 walking up from home {:?}", p);
        loop {
            if p.join("demucs").is_dir() {
                log!("find_repo_dir: step5 FOUND at {:?}", p);
                return p;
            }
            if !p.pop() {
                log!("find_repo_dir: step5 home walk reached root, not found");
                break;
            }
        }
    }

    log!("find_repo_dir: ALL STEPS FAILED — returning \".\"");
    PathBuf::from(".")
}

// ─── Worker ───────────────────────────────────────────────────────────────────

fn start_worker(app: &mut App, track: &Path) -> Result<()> {
    log!("start_worker: ─── begin ───");
    let repo_dir = find_repo_dir();
    log!("start_worker: repo_dir={:?} | track={:?}", repo_dir, track);

    // Log all env overrides so the user can see what's happening
    log!(
        "start_worker: env DEMUCS_REPO_DIR={:?}",
        std::env::var("DEMUCS_REPO_DIR").ok()
    );
    log!(
        "start_worker: env DEMUCS_PYTHON={:?}",
        std::env::var("DEMUCS_PYTHON").ok()
    );
    log!(
        "start_worker: env DEMUCS_WORKER_PATH={:?}",
        std::env::var("DEMUCS_WORKER_PATH").ok()
    );

    // Paths can be overridden by env vars (most reliable for Nix / container deploys).
    // When both are set, find_repo_dir() is effectively unused.
    let worker_py = std::env::var("DEMUCS_WORKER_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            repo_dir
                .join("demucs-tui")
                .join("worker")
                .join("demucs_worker.py")
        });
    let venv_python = std::env::var("DEMUCS_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_dir.join("venv").join("bin").join("python3"));

    log!(
        "start_worker: resolved worker_py={:?} (exists={})",
        worker_py,
        worker_py.exists()
    );
    log!(
        "start_worker: resolved venv_python={:?} (exists={})",
        venv_python,
        venv_python.exists()
    );

    if !worker_py.exists() {
        log!(
            "start_worker: FAIL — worker_py not found at {:?}",
            worker_py
        );
        anyhow::bail!(
            "Worker script not found at: {}\n\
             Set DEMUCS_REPO_DIR to the repo root or DEMUCS_WORKER_PATH to the worker script.\n\
             Searched in: {}",
            worker_py.display(),
            repo_dir.display()
        );
    }
    if !venv_python.exists() {
        log!(
            "start_worker: FAIL — venv_python not found at {:?}",
            venv_python
        );
        anyhow::bail!(
            "Python interpreter not found at: {}\n\
             Set DEMUCS_REPO_DIR to the repo root, or DEMUCS_PYTHON to the interpreter path.\n\
             Searched in: {}",
            venv_python.display(),
            repo_dir.display()
        );
    }

    let output_dir = if app.output_dir.is_empty() {
        repo_dir.join("separated").to_string_lossy().to_string()
    } else {
        app.output_dir.clone()
    };

    let cfg = serde_json::json!({
        "track": track.to_string_lossy(),
        "model": app.models[app.selected_model],
        "shifts": app.shifts,
        "two_stems": if app.two_stems { serde_json::Value::String(app.stems[app.selected_stem].to_string()) } else { serde_json::Value::Null },
        "format": app.formats[app.output_format],
        "output_dir": output_dir,
        "split": true,
        "clip_mode": "rescale",
        "bits_per_sample": 16,
        "as_float": false,
        "jobs": 0,
        "device": app.devices[app.selected_device],
    });

    let config_file = tempfile::Builder::new()
        .prefix("demucs_tui_config_")
        .suffix(".json")
        .tempfile()?;
    let config_path = config_file.path().to_path_buf();
    std::fs::write(&config_path, serde_json::to_string_pretty(&cfg)?)?;
    log!(
        "start_worker: config written to {:?} | cfg={}",
        config_path,
        serde_json::to_string(&cfg).unwrap_or_default()
    );

    log!(
        "start_worker: spawning — cmd={:?} arg={:?} arg={:?}",
        venv_python,
        worker_py,
        config_path
    );

    let mut child = match Command::new(venv_python.to_string_lossy().as_ref())
        .arg(worker_py.to_string_lossy().as_ref())
        .arg(&config_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            log!("start_worker: FAIL — spawn error: {}", e);
            anyhow::bail!(
                "Failed to start Python worker:\n  cmd: {} {}\n  error: {}\n\n\
                 Ensure '{}' is a valid Python interpreter with demucs installed.\n\
                 If using Nix, set DEMUCS_PYTHON to a Python with demucs in its path.",
                venv_python.display(),
                worker_py.display(),
                e,
                venv_python.display()
            );
        }
    };

    log!("start_worker: spawned pid={}", child.id());

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel();

    // Thread: stdout JSON lines
    let tx_out = tx.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(l) => {
                    if tx_out.send(l).is_err() {
                        log!("start_worker stdout thread: channel closed, stopping");
                        break;
                    }
                }
                Err(e) => {
                    log!("start_worker stdout thread: read error: {}", e);
                    break;
                }
            }
        }
        log!("start_worker stdout thread: exiting");
    });

    // Thread: stderr lines → send as error-type messages
    let tx_err = tx.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            match line {
                Ok(l) if !l.is_empty() => {
                    log!("start_worker stderr: {}", l);
                    let msg =
                        serde_json::json!({"type": "error", "msg": format!("[stderr] {}", l)});
                    if tx_err.send(msg.to_string()).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    log!("start_worker stderr thread: read error: {}", e);
                    break;
                }
            }
        }
        log!("start_worker stderr thread: exiting");
    });

    app.worker = Some(child);
    log!("start_worker: done, screen -> Progress");
    app.worker_rx = Some(rx);
    app._config_file = Some(config_file);
    app.progress_pct = 0.0;
    app.progress_msg = "Starting...".to_string();
    app.status_msg.clear();
    app.worker_stderr_lines.clear();
    app.worker_last_msg_time = std::time::Instant::now();
    Ok(())
}

// ─── Main Loop ────────────────────────────────────────────────────────────────

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new();
    app.refresh_dir();

    loop {
        terminal.draw(|f| render(f, &app))?;

        if event::poll(Duration::from_millis(100))? {
            let ev = event::read()?;
            if handle_event(&mut app, ev) {
                break;
            }
        }

        if app.screen == Screen::Progress {
            poll_worker(&mut app);
        }
    }

    Ok(())
}

fn handle_event(app: &mut App, ev: Event) -> bool {
    if let Event::Key(key) = ev {
        if key.kind != KeyEventKind::Press {
            return false;
        }
        match app.screen {
            Screen::Browser => return handle_browser(app, key.code),
            Screen::Settings => {
                handle_settings(app, key.code);
            }
            Screen::Progress => {
                handle_progress(app, key.code);
            }
            Screen::Results => {
                handle_results(app, key.code);
            }
        }
    }
    false
}

// ─── Browser ──────────────────────────────────────────────────────────────────

fn handle_browser(app: &mut App, key: KeyCode) -> bool {
    if app.searching {
        return handle_browser_search(app, key);
    }
    match key {
        KeyCode::Char('q') | KeyCode::Esc => return true,
        KeyCode::Up | KeyCode::Char('k') => {
            app.browser_scroll = app.browser_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = app.entries.len().saturating_sub(1);
            app.browser_scroll = app.browser_scroll.saturating_add(1).min(max);
        }
        KeyCode::Enter => {
            if let Some(entry) = app.entries.get(app.browser_scroll) {
                if entry.is_dir() {
                    app.current_dir = entry.clone();
                    app.refresh_dir();
                } else if is_audio(entry) {
                    app.processed_track = entry.to_string_lossy().to_string();
                    app.screen = Screen::Settings;
                }
            }
        }
        KeyCode::Char('.') => {
            app.show_audio_only = !app.show_audio_only;
            app.refresh_dir();
        }
        KeyCode::Char('h') | KeyCode::Left => {
            if let Some(parent) = app.current_dir.parent() {
                if !parent.to_string_lossy().is_empty() {
                    app.current_dir = parent.to_path_buf();
                    app.refresh_dir();
                }
            }
        }
        KeyCode::Char('s') => {
            if let Some(audio) = app.entries.iter().find(|e| !e.is_dir() && is_audio(e)) {
                app.processed_track = audio.to_string_lossy().to_string();
                app.screen = Screen::Settings;
            }
        }
        KeyCode::Char('/') => {
            app.searching = true;
            app.search_query.clear();
            app.search_matches.clear();
            app.search_idx = 0;
        }
        KeyCode::Char('n') => {
            if !app.search_matches.is_empty() {
                app.search_idx = (app.search_idx + 1) % app.search_matches.len();
                app.browser_scroll = app.search_matches[app.search_idx];
            }
        }
        KeyCode::Char('N') if !app.search_matches.is_empty() => {
            app.search_idx = if app.search_idx == 0 {
                app.search_matches.len() - 1
            } else {
                app.search_idx - 1
            };
            app.browser_scroll = app.search_matches[app.search_idx];
        }
        _ => {}
    }
    false
}

fn handle_browser_search(app: &mut App, key: KeyCode) -> bool {
    match key {
        KeyCode::Esc => {
            app.searching = false;
            app.search_query.clear();
            app.search_matches.clear();
            app.search_idx = 0;
        }
        KeyCode::Enter => {
            app.searching = false;
        }
        KeyCode::Backspace => {
            app.search_query.pop();
            app.search_matches = compute_search_matches(&app.entries, &app.search_query);
            app.search_idx = 0;
            if !app.search_matches.is_empty() {
                app.browser_scroll = app.search_matches[0];
            }
        }
        KeyCode::Char(c) => {
            app.search_query.push(c);
            app.search_matches = compute_search_matches(&app.entries, &app.search_query);
            app.search_idx = 0;
            if !app.search_matches.is_empty() {
                app.browser_scroll = app.search_matches[0];
            }
        }
        _ => {}
    }
    false
}

// ─── Settings ─────────────────────────────────────────────────────────────────

fn handle_settings(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Esc => {
            app.screen = Screen::Browser;
        }
        KeyCode::Up | KeyCode::Char('k') => {}
        KeyCode::Down | KeyCode::Char('j') => {}
        KeyCode::Right => {
            if key == KeyCode::Right {
                // cycle model forward
                app.selected_model = (app.selected_model + 1) % app.models.len();
            }
        }
        KeyCode::Left => {
            app.selected_model = if app.selected_model == 0 {
                app.models.len() - 1
            } else {
                app.selected_model - 1
            };
        }
        KeyCode::Char(' ') => {
            app.two_stems = !app.two_stems;
        }
        KeyCode::Tab => {
            // cycle stems
            if app.two_stems {
                app.selected_stem = (app.selected_stem + 1) % app.stems.len();
            }
        }
        KeyCode::Char('f') => {
            app.output_format = (app.output_format + 1) % app.formats.len();
        }
        KeyCode::Char('d') => {
            app.selected_device = (app.selected_device + 1) % app.devices.len();
        }
        KeyCode::Char('[') => {
            app.shifts = app.shifts.saturating_sub(1).max(1);
        }
        KeyCode::Char(']') => {
            app.shifts = app.shifts.saturating_add(1).min(25);
        }
        KeyCode::Enter => {
            let track = PathBuf::from(&app.processed_track);
            log!("handle_settings: Enter — track={:?} | model={} | shifts={} | two_stems={} | format={}",
                track, app.models[app.selected_model], app.shifts, app.two_stems, app.formats[app.output_format]);
            if !track.exists() {
                log!("handle_settings: track not found at {:?}", track);
                app.status_msg = "Track file not found!".to_string();
                return;
            }
            app.screen = Screen::Progress;
            if let Err(e) = start_worker(app, &track) {
                log!("handle_settings: start_worker error: {}", e);
                app.status_msg = format!("Error: {}", e);
                app.screen = Screen::Settings;
            }
        }
        _ => {}
    }
}

// ─── Progress ─────────────────────────────────────────────────────────────────

fn handle_progress(app: &mut App, key: KeyCode) {
    if matches!(key, KeyCode::Esc | KeyCode::Char('q')) {
        log!("handle_progress: cancel requested (key={:?})", key);
        if let Some(ref mut child) = app.worker {
            log!("handle_progress: killing worker pid={}", child.id());
            let _ = child.kill();
            let _ = child.wait();
        }
        app.worker = None;
        app.worker_rx = None;
        app.screen = Screen::Browser;
        log!("handle_progress: cancelled, back to browser");
    }
}

fn poll_worker(app: &mut App) {
    let Some(rx) = app.worker_rx.as_ref() else {
        return;
    };

    // Drain all pending messages
    loop {
        match rx.try_recv() {
            Ok(line) => {
                app.worker_last_msg_time = std::time::Instant::now();
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&line) {
                    let msg = data
                        .get("msg")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    match data["type"].as_str() {
                        Some("progress") => {
                            app.progress_msg = msg;
                            if let Some(pct) = data["pct"].as_f64() {
                                app.progress_pct = pct;
                            }
                        }
                        Some("done") => {
                            log!("poll_worker: done received — {:?}", line);
                            app.output_files = data["files"]
                                .as_array()
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default();
                            app.output_dir = data["output_dir"].as_str().unwrap_or("").to_string();
                            app.progress_pct = 100.0;
                            app.progress_msg = "Complete!".to_string();
                            app.worker = None;
                            app.worker_rx = None;
                            app.screen = Screen::Results;
                            log!("poll_worker: done processed, screen -> Results");
                            return;
                        }
                        Some("error") => {
                            let err_msg =
                                data["msg"].as_str().unwrap_or("Unknown error").to_string();
                            log!("poll_worker: error received — {:?}", err_msg);
                            app.status_msg = err_msg.clone();
                            app.worker_stderr_lines.push(err_msg);
                            // Don't exit yet — let the dead-worker check below handle it
                        }
                        _ => {
                            log!("poll_worker: unknown message type — {:?}", line);
                        }
                    }
                } else {
                    log!("poll_worker: non-JSON line from worker — {:?}", line);
                    // Store non-JSON output (likely Python traceback) for display
                    app.worker_stderr_lines.push(line);
                }
            }
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => {
                log!("poll_worker: channel disconnected");
                app.worker = None;
                app.worker_rx = None;
                break;
            }
        }
    }

    // Check if the worker process has died (no messages for 30s and process gone)
    let stale_secs = app.worker_last_msg_time.elapsed().as_secs();
    if let Some(ref mut worker) = app.worker {
        match worker.try_wait() {
            Ok(Some(status)) => {
                // Process exited
                log!(
                    "poll_worker: worker exited with status={} after {}s of no messages",
                    status,
                    stale_secs
                );
                let exit_msg = format!(
                    "Worker process exited with code: {}",
                    status.code().unwrap_or(-1)
                );
                if app.worker_stderr_lines.is_empty() {
                    app.status_msg = exit_msg;
                } else {
                    // Show last few stderr lines
                    let tail: Vec<_> = app.worker_stderr_lines.iter().rev().take(3).collect();
                    app.status_msg = format!(
                        "{}\n{}",
                        exit_msg,
                        tail.iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join("\n")
                    );
                }
                log!("poll_worker: {}", app.status_msg);
                app.worker = None;
                app.worker_rx = None;
                app.screen = Screen::Settings;
                return;
            }
            Ok(None) => {
                // Still running — check for stall
                if stale_secs >= 30 && app.progress_msg == "Starting..." {
                    log!(
                        "poll_worker: worker alive but no messages for {}s — might be loading model",
                        stale_secs
                    );
                    app.progress_msg = format!("Loading model... ({}s)", stale_secs);
                }
            }
            Err(e) => {
                log!("poll_worker: try_wait error: {}", e);
            }
        }
    }
}

// ─── Results ──────────────────────────────────────────────────────────────────

fn handle_results(app: &mut App, key: KeyCode) {
    if matches!(key, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter) {
        app.screen = Screen::Browser;
        app.output_files.clear();
        app.output_dir.clear();
        app.progress_pct = 0.0;
    }
}

// ─── Rendering ────────────────────────────────────────────────────────────────

fn render(f: &mut Frame, app: &App) {
    match app.screen {
        Screen::Browser => render_browser(f, app),
        Screen::Settings => render_settings(f, app),
        Screen::Progress => render_progress(f, app),
        Screen::Results => render_results(f, app),
    }
}

fn render_browser(f: &mut Frame, app: &App) {
    let area = block_frame(f, " demucs-tui — Browser ", None);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(area);

    if app.searching {
        let search_display = if app.search_matches.is_empty() {
            format!(" /{}_  (no matches)", app.search_query)
        } else {
            format!(
                " /{}_  ({}/{})",
                app.search_query,
                app.search_idx + 1,
                app.search_matches.len()
            )
        };
        let info = Line::from(vec![Span::styled(
            search_display,
            Style::default().fg(Color::Yellow),
        )]);
        f.render_widget(Paragraph::new(info), chunks[0]);
    } else {
        let filter_indicator = if app.show_audio_only { "audio" } else { "all" };
        let info = Line::from(vec![
            Span::raw(" \u{1F4C1} "),
            Span::styled(
                app.current_dir.to_string_lossy().to_string(),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw(format!("  ({})  ", app.entries.len())),
            Span::styled(
                format!("[{}]", filter_indicator),
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        f.render_widget(Paragraph::new(info), chunks[0]);
    }

    let visible = (chunks[1].height as usize).saturating_sub(2);
    let start = app
        .browser_scroll
        .min(app.entries.len().saturating_sub(visible));
    let end = (start + visible).min(app.entries.len());

    let items: Vec<ListItem> = app.entries[start..end]
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let is_selected = i + start == app.browser_scroll;
            let is_match =
                !app.search_query.is_empty() && app.search_matches.contains(&(i + start));
            let name = entry
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let prefix = if entry.is_dir() {
                "\u{1F4C1} "
            } else {
                "\u{1F3B5} "
            };
            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else if is_match {
                Style::default().fg(Color::Yellow)
            } else if entry.is_dir() {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            ListItem::new(format!("{}{}", prefix, name)).style(style)
        })
        .collect();

    f.render_widget(
        List::new(items).block(Block::default().borders(Borders::NONE)),
        chunks[1],
    );

    let has_matches = !app.search_query.is_empty() && !app.search_matches.is_empty();
    let help = if app.searching {
        Line::from(vec![
            Span::styled(" Esc ", Style::default().fg(Color::DarkGray)),
            Span::raw("cancel  "),
            Span::styled("Enter", Style::default().fg(Color::DarkGray)),
            Span::raw(" confirm"),
        ])
    } else if has_matches {
        Line::from(vec![
            Span::styled(
                " \u{2191}\u{2193}/jk ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw("navigate  "),
            Span::styled("n/N", Style::default().fg(Color::DarkGray)),
            Span::raw(" search  "),
            Span::styled("Enter", Style::default().fg(Color::DarkGray)),
            Span::raw(" select  "),
            Span::styled("h/\u{2190}", Style::default().fg(Color::DarkGray)),
            Span::raw(" parent  "),
            Span::styled(".", Style::default().fg(Color::DarkGray)),
            Span::raw(" filter  "),
            Span::styled("s", Style::default().fg(Color::DarkGray)),
            Span::raw(" quick  "),
            Span::styled("q/Esc", Style::default().fg(Color::DarkGray)),
            Span::raw(" quit"),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                " \u{2191}\u{2193}/jk ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw("navigate  "),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::raw(" search  "),
            Span::styled("Enter", Style::default().fg(Color::DarkGray)),
            Span::raw(" select  "),
            Span::styled("h/\u{2190}", Style::default().fg(Color::DarkGray)),
            Span::raw(" parent  "),
            Span::styled(".", Style::default().fg(Color::DarkGray)),
            Span::raw(" filter  "),
            Span::styled("s", Style::default().fg(Color::DarkGray)),
            Span::raw(" quick  "),
            Span::styled("q/Esc", Style::default().fg(Color::DarkGray)),
            Span::raw(" quit"),
        ])
    };
    f.render_widget(
        Paragraph::new(help).style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );
}

fn render_settings(f: &mut Frame, app: &App) {
    let area = block_frame(f, " demucs-tui — Settings ", None);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(8),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .split(area);

    let track_name = Path::new(&app.processed_track)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let track_line = Line::from(vec![
        Span::raw(" Track: "),
        Span::styled(
            &track_name,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(Paragraph::new(track_line), chunks[0]);

    // Model
    let model_items: Vec<ListItem> = app
        .models
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let selected = i == app.selected_model;
            let style = if selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default()
            };
            ListItem::new(format!(
                " {} {}",
                if selected { "\u{25B6}" } else { " " },
                m
            ))
            .style(style)
        })
        .collect();
    f.render_widget(
        List::new(model_items).block(
            Block::default()
                .title(" Model (\u{2190}\u{2192} change) ")
                .borders(Borders::ALL),
        ),
        chunks[1],
    );

    // Two-stems
    let stem_char = if app.two_stems { "\u{2714}" } else { " " };
    let stem_name = if app.two_stems {
        app.stems[app.selected_stem]
    } else {
        "- Space to enable -"
    };
    let stem_text = format!(
        " Two stems: [{}]  {}  (Tab: cycle stem)",
        stem_char, stem_name
    );
    f.render_widget(
        Paragraph::new(stem_text).block(Block::default().title(" Stems ").borders(Borders::ALL)),
        chunks[2],
    );

    // Format
    let fmt_text = format!(" Output: {}", app.formats[app.output_format]);
    let fmt_hint = "  (f: cycle)";
    f.render_widget(
        Paragraph::new(format!("{}{}", fmt_text, fmt_hint))
            .block(Block::default().title(" Format ").borders(Borders::ALL)),
        chunks[3],
    );

    // Device
    let dev_text = format!(" Device: {}  (d: cycle)", app.devices[app.selected_device]);
    f.render_widget(
        Paragraph::new(dev_text).block(Block::default().title(" Compute ").borders(Borders::ALL)),
        chunks[4],
    );

    // Shifts
    let shift_text = format!(" Shifts: {}  ([ decrease, ] increase)", app.shifts);
    f.render_widget(
        Paragraph::new(shift_text).block(Block::default().title(" Quality ").borders(Borders::ALL)),
        chunks[5],
    );

    // Status
    if !app.status_msg.is_empty() {
        f.render_widget(
            Paragraph::new(app.status_msg.clone()).style(Style::default().fg(Color::Red)),
            chunks[6],
        );
    }

    // Help
    let help = Line::from(vec![
        Span::styled(" \u{2190}\u{2192} ", Style::default().fg(Color::DarkGray)),
        Span::raw("model  "),
        Span::styled("Space", Style::default().fg(Color::DarkGray)),
        Span::raw(" stems  "),
        Span::styled("f", Style::default().fg(Color::DarkGray)),
        Span::raw(" format  "),
        Span::styled("d", Style::default().fg(Color::DarkGray)),
        Span::raw(" device  "),
        Span::styled("[", Style::default().fg(Color::DarkGray)),
        Span::raw("/"),
        Span::styled("]", Style::default().fg(Color::DarkGray)),
        Span::raw(" shifts  "),
        Span::styled("Enter", Style::default().fg(Color::Green)),
        Span::raw(" start  "),
        Span::styled("Esc", Style::default().fg(Color::DarkGray)),
        Span::raw(" back"),
    ]);
    f.render_widget(
        Paragraph::new(help).style(Style::default().fg(Color::DarkGray)),
        chunks[6],
    );
}

fn render_progress(f: &mut Frame, app: &App) {
    let area = block_frame(f, " demucs-tui — Separating ", None);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(area);

    let track_name = Path::new(&app.processed_track)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" File: "),
            Span::styled(&track_name, Style::default().fg(Color::Yellow)),
        ])),
        chunks[0],
    );

    let label = format!(" {:.0}% \u{2014} {}", app.progress_pct, app.progress_msg);
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" Progress "))
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
        .percent(app.progress_pct as u16)
        .label(label);
    f.render_widget(gauge, chunks[1]);

    // Show last few stderr lines if any
    if !app.worker_stderr_lines.is_empty() {
        let tail: Vec<_> = app.worker_stderr_lines.iter().rev().take(3).collect();
        let stderr_text: Vec<Line> = tail
            .iter()
            .map(|l| Line::from(Span::styled(l.as_str(), Style::default().fg(Color::Red))))
            .collect();
        f.render_widget(
            Paragraph::new(stderr_text)
                .block(Block::default().title(" Errors ").borders(Borders::ALL)),
            chunks[3],
        );
    }

    let help = Line::from(vec![
        Span::styled(" Esc/q ", Style::default().fg(Color::DarkGray)),
        Span::raw(" cancel"),
    ]);
    f.render_widget(
        Paragraph::new(help).style(Style::default().fg(Color::DarkGray)),
        chunks[4],
    );
}

fn render_results(f: &mut Frame, app: &App) {
    let area = block_frame(f, " demucs-tui — Complete ", None);

    let file_count = app.output_files.len() as u16;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(file_count + 2),
            Constraint::Min(1),
        ])
        .split(area);

    let track_name = Path::new(&app.processed_track)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" \u{2705} "),
            Span::styled(
                &track_name,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" separated"),
        ])),
        chunks[0],
    );

    let file_items: Vec<ListItem> = app
        .output_files
        .iter()
        .map(|f| {
            let path = Path::new(f);
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let size = std::fs::metadata(f)
                .map(|m| {
                    let s = m.len();
                    if s > 1_000_000 {
                        format!("{:.1} MB", s as f64 / 1_000_000.0)
                    } else if s > 1_000 {
                        format!("{:.1} KB", s as f64 / 1_000.0)
                    } else {
                        format!("{} B", s)
                    }
                })
                .unwrap_or_default();
            ListItem::new(format!("  \u{1F3B5} {:<30} {}", name, size))
        })
        .collect();
    f.render_widget(
        List::new(file_items).block(Block::default().title(" Output ").borders(Borders::ALL)),
        chunks[1],
    );

    if !app.output_dir.is_empty() {
        let dir_line = Line::from(vec![
            Span::styled(" \u{1F4C1} ", Style::default().fg(Color::Cyan)),
            Span::raw(&app.output_dir),
        ]);
        f.render_widget(Paragraph::new(dir_line), chunks[2]);
    }

    let help = Line::from(vec![
        Span::styled(" Enter/Esc/q ", Style::default().fg(Color::DarkGray)),
        Span::raw(" back to browser"),
    ]);
    f.render_widget(
        Paragraph::new(help).style(Style::default().fg(Color::DarkGray)),
        Rect::new(
            area.x,
            area.y + area.height.saturating_sub(1),
            area.width,
            1,
        ),
    );
}

fn block_frame(f: &mut Frame, title: &str, _extra: Option<&str>) -> Rect {
    let block = Block::default()
        .title(title)
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));
    let area = block.inner(f.area());
    f.render_widget(block, f.area());
    area
}

// ─── Entry Point ─────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    // Log the startup banner
    let pid = std::process::id();
    let exe = std::env::current_exe().ok();
    let cwd = std::env::current_dir().ok();
    log!("=== demucs-tui start ===");
    log!("main: pid={} | exe={:?} | cwd={:?}", pid, exe, cwd);
    log!("main: args={:?}", std::env::args().collect::<Vec<_>>());
    log!("main: HOME={:?}", std::env::var("HOME").ok());
    log!(
        "main: DEMUCS_REPO_DIR={:?}",
        std::env::var("DEMUCS_REPO_DIR").ok()
    );

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_app(&mut terminal);

    disable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(LeaveAlternateScreen)?;

    if let Err(e) = &result {
        log!("main: error — {}", e);
        eprintln!("Error: {}", e);
    }
    log!("=== demucs-tui exit ===");
    result
}
