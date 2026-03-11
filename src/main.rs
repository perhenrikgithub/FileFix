use clap::{Parser, Subcommand};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use indicatif::{ProgressBar, ProgressStyle};
use inquire::{Confirm, MultiSelect, Select};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::Paragraph,
    Frame, Terminal,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, stdout};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use walkdir::WalkDir;

// ==========================================
// 0. FORMAT VALIDATIONS & DEFINITIONS
// ==========================================

const IMAGE_TYPES: &[&str] = &["heic", "heif", "tiff", "bmp", "jpg", "jpeg", "png"];
const DOC_TYPES: &[&str] = &["docx", "pptx"];
const NOTEBOOK_TYPES: &[&str] = &["ipynb"];

fn get_all_input_types() -> Vec<&'static str> {
    let mut types = vec![];
    types.extend_from_slice(IMAGE_TYPES);
    types.extend_from_slice(DOC_TYPES);
    types.extend_from_slice(NOTEBOOK_TYPES);
    types
}

fn get_available_target_formats(input_ext: &str) -> Vec<&'static str> {
    if DOC_TYPES.contains(&input_ext) || NOTEBOOK_TYPES.contains(&input_ext) {
        vec!["pdf"]
    } else if IMAGE_TYPES.contains(&input_ext) {
        let mut targets = IMAGE_TYPES.to_vec();
        targets.push("pdf");
        targets.retain(|&x| x != input_ext);
        targets
    } else {
        vec![]
    }
}

// ==========================================
// 1. VERBOSE CLI DEFINITION
// ==========================================
#[derive(Parser)]
#[command(author = "Per Henrik", version = "0.1", about = "FileFix", long_about = "A terminal-based file conversion tool for images, documents, and notebooks. Use the interactive dashboard or direct CLI commands to convert files in your default folder (~/Downloads).")]
struct Cli {
    #[arg(long, global = true)]
    default_folder: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Config { #[arg(long)] default_folder: PathBuf },
    ConvertSingle {
        #[arg(long)] to: String,
        #[arg(long)] file: PathBuf,
        #[arg(long)] full_path: bool,
        #[arg(long)] delete_original: bool,
        #[arg(long)] open_when_done: bool,
        #[arg(long)] overwrite: bool,
    },
    ConvertBatch {
        #[arg(long)] to: String,
        #[arg(long)] input_type: String,
        #[arg(long)] folder: Option<PathBuf>,
        #[arg(long)] delete_original: bool,
        #[arg(long)] open_when_done: bool,
        #[arg(long)] overwrite: bool,
    },
}

#[derive(Serialize, Deserialize)]
struct Config {
    default_folder: PathBuf,
}

// ==========================================
// MAIN ROUTER
// ==========================================
fn main() {
    let config_path = dirs::home_dir().unwrap().join(".filefix/config.toml");
    let default_folder = load_default_folder(&config_path);

    let args: Vec<String> = std::env::args().collect();

    // SCENARIO 1: No arguments -> Launch Fullscreen Bubble Tea Dashboard
    if args.len() == 1 {
        let launch_wizard = run_dashboard_tui().unwrap_or(false);
        if launch_wizard {
            launch_interactive_entry_point(&default_folder);
        }
        return;
    }

    // SCENARIO 2: `filefix heic` -> Launch direct Interactive Wizard
    if args.len() == 2 && !args[1].starts_with('-') && args[1] != "config" && args[1] != "help" {
        let ext = normalize_ext(&args[1]);
        if get_all_input_types().contains(&ext.as_str()) {
            run_interactive_wizard(&ext, &default_folder);
            return;
        }
    }

    // SCENARIO 3: Parse Verbose CLI Commands
    let cli = Cli::parse();
    let active_folder = cli.default_folder.unwrap_or(default_folder);

    match cli.command {
        Commands::Config { default_folder } => {
            if let Some(parent) = config_path.parent() { fs::create_dir_all(parent).ok(); }
            let conf = Config { default_folder: default_folder.clone() };
            fs::write(&config_path, toml::to_string(&conf).unwrap()).ok();
            println!("✅ Default folder set to: {:?}", default_folder);
        }
        Commands::ConvertSingle { to, file, full_path, delete_original, open_when_done, overwrite } => {
            let to_norm = normalize_ext(&to);
            let candidates = if full_path { vec![file.clone()] } else { vec![file.clone(), active_folder.join(file.file_name().unwrap())] };
            if let Some(valid_path) = candidates.into_iter().find(|p| p.exists()) {
                engine_convert(vec![valid_path], &to_norm, delete_original, open_when_done, overwrite);
            }
        }
        Commands::ConvertBatch { to, input_type, folder, delete_original, open_when_done, overwrite } => {
            let to_norm = normalize_ext(&to);
            let input_norm = normalize_ext(&input_type);
            let target_folder = folder.unwrap_or(active_folder);
            
            let files: Vec<PathBuf> = WalkDir::new(&target_folder).max_depth(1).into_iter().filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|ext| normalize_ext(&ext.to_string_lossy()) == input_norm).unwrap_or(false))
                .map(|e| e.path().to_path_buf()).collect();

            engine_convert(files, &to_norm, delete_original, open_when_done, overwrite);
        }
    }
}

// ==========================================
// 2. THE BUBBLE TEA ARCHITECTURE (DASHBOARD)
// ==========================================
struct DashboardModel {
    should_quit: bool,
    launch_wizard: bool,
}

enum Msg {
    Quit,
    StartWizard,
    None,
}

fn update(model: &mut DashboardModel, msg: Msg) {
    match msg {
        Msg::Quit => model.should_quit = true,
        Msg::StartWizard => {
            model.launch_wizard = true;
            model.should_quit = true;
        }
        Msg::None => {}
    }
}

fn view(frame: &mut Frame, _model: &DashboardModel) {
    let area = frame.size();
    
    let ascii_logo = r#"
███████╗██╗██╗     ███████╗███████╗██╗██╗  ██╗
██╔════╝██║██║     ██╔════╝██╔════╝██║╚██╗██╔╝
█████╗  ██║██║     █████╗  █████╗  ██║ ╚███╔╝ 
██╔══╝  ██║██║     ██╔══╝  ██╔══╝  ██║ ██╔██╗ 
██║     ██║███████╗███████╗██║     ██║██╔╝ ██╗
╚═╝     ╚═╝╚══════╝╚══════╝╚═╝     ╚═╝╚═╝  ╚═╝
"#;

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(2),
            Constraint::Length(8),
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Min(2),
            Constraint::Length(1),
        ])
        .margin(2)
        .split(area);

    let logo = Paragraph::new(ascii_logo).style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)).alignment(Alignment::Center);
    let title = Paragraph::new("Welcome to FileFix").style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)).alignment(Alignment::Center);
    let desc = Paragraph::new("Convert Images, Documents, and Notebooks directly from your terminal.").style(Style::default().fg(Color::DarkGray)).alignment(Alignment::Center);
    let footer = Paragraph::new("Press [ENTER] to start wizard • Press [Q] or [ESC] to quit").style(Style::default().fg(Color::Yellow)).alignment(Alignment::Center);

    frame.render_widget(logo, layout[1]);
    frame.render_widget(title, layout[2]);
    frame.render_widget(desc, layout[3]);
    frame.render_widget(footer, layout[5]);
}

fn run_dashboard_tui() -> Result<bool, io::Error> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut model = DashboardModel { should_quit: false, launch_wizard: false };

    while !model.should_quit {
        terminal.draw(|f| view(f, &model))?;
        let msg = if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => Msg::Quit,
                    KeyCode::Enter => Msg::StartWizard,
                    _ => Msg::None,
                }
            } else { Msg::None }
        } else { Msg::None };
        update(&mut model, msg);
    }

    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(model.launch_wizard)
}

// ==========================================
// 3. THE INTERACTIVE WIZARD (INQUIRE)
// ==========================================
fn launch_interactive_entry_point(folder: &Path) {
    println!("\n✨ FileFix Interactive Wizard");
    let target_ext = match Select::new(
        "What file format are you looking to convert FROM?",
        get_all_input_types(),
    ).prompt() {
        Ok(ext) => ext,
        Err(_) => return,
    };
    run_interactive_wizard(target_ext, folder);
}

fn run_interactive_wizard(target_ext: &str, folder: &Path) {
    println!("🔍 Searching for .{} files in {:?}...\n", target_ext, folder);

    let mut found_files = vec![];
    for entry in WalkDir::new(folder).max_depth(1).into_iter().filter_map(|e| e.ok()) {
        if let Some(ext) = entry.path().extension() {
            if normalize_ext(&ext.to_string_lossy()) == target_ext {
                found_files.push(entry.file_name().to_string_lossy().to_string());
            }
        }
    }

    if found_files.is_empty() {
        println!("❌ No .{} files found. Try placing some in your default folder.", target_ext);
        return;
    }

    let total_found = found_files.len();

    let selected_names = match MultiSelect::new(
        "Which files would you like to convert? (Space to select, Enter to confirm)",
        found_files,
    ).prompt() {
        Ok(names) if names.is_empty() => { println!("No files selected. Exiting."); return; }
        Ok(names) => names,
        Err(_) => return,
    };

    let formats = get_available_target_formats(target_ext);
    if formats.is_empty() {
        println!("❌ No supported conversion formats available for {}.", target_ext);
        return;
    }

    let to_format = if formats.len() == 1 {
        println!("ℹ️  {} can only be converted to {}. Auto-selecting {}.", target_ext, formats[0], formats[0]);
        formats[0]
    } else {
        match Select::new("Convert to which format?", formats).prompt() {
            Ok(f) => f,
            Err(_) => return,
        }
    };

    let configure_advanced = Confirm::new("Configure advanced options? (Delete original, Overwrite, Open)")
        .with_default(false).prompt().unwrap_or(false);

    let (mut delete_original, mut overwrite, mut open_when_done) = (false, false, false);
    if configure_advanced {
        delete_original = Confirm::new("Delete original files after conversion?").with_default(false).prompt().unwrap_or(false);
        overwrite = Confirm::new("Force overwrite of files if name matches?").with_default(false).with_help_message("If disabled, converted files will be renamed with (1), (2) suffixes to avoid overwriting.").prompt().unwrap_or(false);
        open_when_done = Confirm::new("Open converted files when done?").with_default(false).prompt().unwrap_or(false);
    }

    // --- POWER USER TIP ---
    let mut flags = String::new();
    if delete_original { flags.push_str(" --delete-original"); }
    if overwrite { flags.push_str(" --overwrite"); }
    if open_when_done { flags.push_str(" --open-when-done"); }

    println!("\n💡 Power User Tip: Run this directly next time:");
    if selected_names.len() == total_found {
        println!("   filefix convert-batch --input-type {} --to {}{}", target_ext, to_format, flags);
    } else if selected_names.len() == 1 {
        println!("   filefix convert-single --file \"{}\" --to {}{}", selected_names[0], to_format, flags);
    } else {
        println!("   filefix convert-batch --input-type {} --to {}{}", target_ext, to_format, flags);
    }
    println!();
    // -----------------------

    let files_to_process: Vec<PathBuf> = selected_names.into_iter().map(|n| folder.join(n)).collect();
    engine_convert(files_to_process, to_format, delete_original, open_when_done, overwrite);
}

// ==========================================
// 4. THE CORE ENGINE
// ==========================================
fn engine_convert(files: Vec<PathBuf>, convert_to: &str, delete_original: bool, open_when_done: bool, overwrite: bool) {
    if files.is_empty() { return; }

    ensure_dependencies(&files);

    let mut safe_open = open_when_done;
    if safe_open && files.len() > 7 {
        safe_open = Confirm::new(&format!("⚠️ You are converting {} files. Are you sure you want to open all of them when done?", files.len()))
            .with_default(false).prompt().unwrap_or(false);
        if !safe_open { println!("Okay, files will not be opened when done."); }
    }

    let pb = ProgressBar::new(files.len() as u64);
    let total = files.len() as u64;
    let start = Instant::now();

    pb.set_style(ProgressStyle::with_template("{spinner} Converting files {pos}/{len} • {msg}").unwrap());
    pb.enable_steady_tick(std::time::Duration::from_millis(120));

    files.into_par_iter().for_each(|file| {
        let input_ext_norm = normalize_ext(&file.extension().unwrap_or_default().to_string_lossy());

        let supported_targets = get_available_target_formats(&input_ext_norm);
        if !supported_targets.contains(&convert_to) {
            pb.println(format!("⚠️ Unsupported conversion: {} to {}", input_ext_norm, convert_to));
            pb.inc(1);
            return;
        }

        if input_ext_norm == convert_to {
            pb.println(format!("⚠️ Skipping {:?}: input matches output", file.file_name().unwrap()));
            pb.inc(1);
            return;
        }

        let output_file = if overwrite { file.with_extension(convert_to) } else { unique_output_path(&file, convert_to) };
        pb.set_message(file.file_name().unwrap().to_string_lossy().to_string());

        let mut result: Result<(), String> = Err("Unsupported conversion type".to_string());

        if IMAGE_TYPES.contains(&input_ext_norm.as_str()) {
            // ImageMagick Pipeline
            match Command::new("magick").arg(&file).arg(&output_file).output() {
                Ok(out) if out.status.success() => result = Ok(()),
                Ok(out) => result = Err(extract_error_reason(&out)),
                Err(e) => result = Err(e.to_string()),
            }

        } else if DOC_TYPES.contains(&input_ext_norm.as_str()) && convert_to == "pdf" {
            // LibreOffice Pipeline
            let mut hasher = DefaultHasher::new();
            file.hash(&mut hasher);
            std::thread::current().id().hash(&mut hasher);
            let hash = hasher.finish();
            
            // Isolate LibreOffice outputs and UserInstallation to avoid thread clashes and overwrites
            let temp_work_dir = std::env::temp_dir().join(format!("filefix_lo_{}", hash));
            fs::create_dir_all(&temp_work_dir).ok();
            let env_arg = format!("-env:UserInstallation=file://{}", to_file_url(&temp_work_dir.join("profile")));

            match Command::new(get_soffice_cmd())
                .arg(&env_arg)
                .args(["--headless", "--convert-to", "pdf"])
                .arg(&file)
                .arg("--outdir")
                .arg(&temp_work_dir)
                .output() {
                Ok(out) if out.status.success() => {
                    let generated_out = temp_work_dir.join(file.file_stem().unwrap()).with_extension("pdf");
                    if generated_out.exists() {
                        if fs::copy(&generated_out, &output_file).is_ok() {
                            result = Ok(());
                        } else {
                            result = Err("LibreOffice generated the PDF, but failed to copy it".to_string());
                        }
                    } else {
                        result = Err("LibreOffice succeeded but no output PDF was found".to_string());
                    }
                },
                Ok(out) => result = Err(extract_error_reason(&out)),
                Err(e) => result = Err(e.to_string()),
            }
            let _ = fs::remove_dir_all(&temp_work_dir);

        } else if NOTEBOOK_TYPES.contains(&input_ext_norm.as_str()) && convert_to == "pdf" {
            // Jupyter Pipeline
            let mut hasher = DefaultHasher::new();
            file.hash(&mut hasher);
            std::thread::current().id().hash(&mut hasher);
            let hash = hasher.finish();

            let temp_work_dir = std::env::temp_dir().join(format!("filefix_jup_{}", hash));
            fs::create_dir_all(&temp_work_dir).ok();

            let jupyter_bin = get_venv_bin("jupyter");
            let python_bin = get_venv_bin("python");

            // Re-usable custom executor using OUR ISOLATED VENV
            let run_custom_html_to_pdf = || -> Result<(), String> {
                // Step 1: Export to HTML using JupyterLab template (Modern UI + MathJax 3)
                let html_out = Command::new(&jupyter_bin)
                    .args(["nbconvert", "--to", "html", "--template", "lab"])
                    .arg(&file)
                    .arg("--output-dir")
                    .arg(&temp_work_dir)
                    .output()
                    .map_err(|e| e.to_string())?;

                if !html_out.status.success() {
                    return Err(format!("HTML Generation failed: {}", extract_error_reason(&html_out)));
                }

                let html_file_path = temp_work_dir.join(file.file_stem().unwrap()).with_extension("html");
                if !html_file_path.exists() {
                    return Err("HTML file was not generated by nbconvert.".to_string());
                }

                // Step 2: Use Playwright explicitly to render the PDF and WAIT for MathJax
                let py_script = r#"
import sys, os
from playwright.sync_api import sync_playwright

html_file = f"file://{os.path.abspath(sys.argv[1])}"
pdf_file = sys.argv[2]

with sync_playwright() as p:
    # Disable web security to allow local HTML to fetch MathJax from CDNs
    browser = p.chromium.launch(args=["--disable-web-security"])
    page = browser.new_page()
    page.goto(html_file, wait_until="networkidle")
    
    # Wait for MathJax to finish rendering equations (2.5 seconds)
    page.wait_for_timeout(2500)
    
    page.emulate_media(media="print")
    page.pdf(path=pdf_file, format="A4", print_background=True, margin={"top": "1cm", "bottom": "1cm", "left": "1cm", "right": "1cm"})
    browser.close()
"#;
                let script_path = temp_work_dir.join("render_pdf.py");
                fs::write(&script_path, py_script).map_err(|e| e.to_string())?;

                let pdf_out = Command::new(&python_bin)
                    .arg(&script_path)
                    .arg(&html_file_path)
                    .arg(&output_file)
                    .output()
                    .map_err(|e| e.to_string())?;

                if !pdf_out.status.success() {
                    return Err(format!("Playwright PDF rendering failed: {}", extract_error_reason(&pdf_out)));
                }

                Ok(())
            };

            // 1. FIRST TRY: Custom HTML -> Playwright pipeline (Ensures MathJax works)
            result = run_custom_html_to_pdf();

            // 2. FALLBACK: standard ipynb -> latex -> PDF (if the HTML route crashes entirely)
            if let Err(err_html) = &result {
                let fallback_out = Command::new(&jupyter_bin)
                    .args(["nbconvert", "--to", "pdf"])
                    .arg(&file)
                    .arg("--output-dir")
                    .arg(&temp_work_dir)
                    .output();
                
                match fallback_out {
                    Ok(out) if out.status.success() => {
                        let generated_out = temp_work_dir.join(file.file_stem().unwrap()).with_extension("pdf");
                        if generated_out.exists() && fs::copy(&generated_out, &output_file).is_ok() {
                            result = Ok(());
                        }
                    },
                    Ok(out) => result = Err(format!("HTML->PDF err: {} | LaTeX->PDF err: {}", err_html, extract_error_reason(&out))),
                    Err(e) => result = Err(format!("HTML->PDF err: {} | LaTeX->PDF err: {}", err_html, e)),
                }
            }
            let _ = fs::remove_dir_all(&temp_work_dir);
        }

        // --- Execute file handling ---
        match result {
            Ok(_) => {
                if delete_original { fs::remove_file(&file).ok(); }
                if safe_open { open_file(&output_file); }
            }
            Err(e) => {
                // Now displays WHY it failed
                pb.println(format!("⚠️ Failed to convert {:?}: {}", file.file_name().unwrap(), e));
            }
        }

        pb.inc(1);
    });

    pb.finish_and_clear();
    println!("✔ Converted {} files in {:.2}s", total, start.elapsed().as_secs_f32());
}

// ==========================================
// HELPER FUNCTIONS
// ==========================================

fn extract_error_reason(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    let err_lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    let all_lines: Vec<&str> = stderr.lines().chain(stdout.lines()).filter(|l| !l.trim().is_empty()).collect();
    
    // Python Tracebacks & System errors usually push their final reason to stderr
    let target_lines = if !err_lines.is_empty() { err_lines } else { all_lines };
    
    if target_lines.is_empty() {
        return "Unknown error (No output provided by underlying tool)".to_string();
    }
    
    // Grab the last line (which for Python stack traces is usually the core Exception)
    let last_line = target_lines.last().unwrap().trim();
    
    // Cap at a reasonable line limit to not break UI formatting
    let max_len = 100;
    if last_line.len() > max_len {
        format!("{}...", &last_line[..max_len])
    } else {
        last_line.to_string()
    }
}

fn get_venv_path() -> PathBuf {
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".filefix")
        .join("venv")
}

fn get_venv_bin(binary: &str) -> PathBuf {
    let venv = get_venv_path();
    #[cfg(target_os = "windows")]
    {
        venv.join("Scripts").join(format!("{}.exe", binary))
    }
    #[cfg(not(target_os = "windows"))]
    {
        venv.join("bin").join(binary)
    }
}

fn load_default_folder(config_path: &PathBuf) -> PathBuf {
    if config_path.exists() {
        if let Ok(conf) = toml::from_str::<Config>(&fs::read_to_string(config_path).unwrap_or_default()) {
            return conf.default_folder;
        }
    }
    dirs::download_dir().expect("Cannot determine Downloads folder")
}

fn normalize_ext(ext: &str) -> String {
    let ext_l = ext.to_lowercase();
    if ext_l == "jpeg" { "jpg".to_string() } else { ext_l }
}

fn unique_output_path(original: &Path, convert_to: &str) -> PathBuf {
    let mut output = original.with_extension(convert_to);
    let mut counter = 1;
    while output.exists() {
        let stem = original.file_stem().unwrap().to_string_lossy();
        let parent = original.parent().unwrap_or_else(|| Path::new("."));
        output = parent.join(format!("{}({}).{}", stem, counter, convert_to));
        counter += 1;
    }
    output
}

fn open_file(path: &Path) {
    #[cfg(target_os = "macos")] { Command::new("open").arg(path).status().ok(); }
    #[cfg(target_os = "linux")] { Command::new("xdg-open").arg(path).status().ok(); }
    #[cfg(target_os = "windows")] { Command::new("cmd").arg("/C").arg("start").arg(path).status().ok(); }
}

fn get_soffice_cmd() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        if Path::new("/Applications/LibreOffice.app/Contents/MacOS/soffice").exists() {
            return "/Applications/LibreOffice.app/Contents/MacOS/soffice";
        }
    }
    "soffice"
}

fn to_file_url(path: &Path) -> String {
    let mut s = path.to_string_lossy().replace("\\", "/");
    if !s.starts_with('/') {
        s = format!("/{}", s);
    }
    s
}

fn command_exists(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn ensure_python_venv() -> Result<(), String> {
    let venv_path = get_venv_path();
    let marker_file = venv_path.join(".setup_complete");

    // If the marker exists, the venv is fully set up and ready to go
    if marker_file.exists() && get_venv_bin("jupyter").exists() {
        return Ok(());
    }

    println!("⚠️ Jupyter dependencies not found. Setting up Python environment...");
    println!("If this is the first time you're converting notebooks, this setup is required to enable PDF conversion. This is a one-time setup.");
    println!("\n⏳ [1/4] Creating dedicated Python environment for FileFix...");
    let system_python = if command_exists("python3") { "python3" } else { "python" };
    
    let status = Command::new(system_python)
        .args(["-m", "venv"])
        .arg(&venv_path)
        .status()
        .map_err(|e| format!("Failed to execute python: {}", e))?;

    if !status.success() {
        return Err("Failed to create Python virtual environment. Do you have python3-venv installed?".to_string());
    }

    println!("⏳ [2/4] Installing Jupyter and WebPDF dependencies (this may take a minute)...");
    let pip_exe = get_venv_bin("pip");
    let status = Command::new(&pip_exe)
        .args(["install", "-q", "jupyter", "nbconvert[webpdf]", "playwright"])
        .status()
        .map_err(|e| format!("Failed to run pip: {}", e))?;

    if !status.success() {
        return Err("Failed to install Python dependencies via pip.".to_string());
    }

    println!("⏳ [3/4] Installing headless browser for PDF conversion...");
    let playwright_exe = get_venv_bin("playwright");
    let status = Command::new(&playwright_exe)
        .args(["install", "chromium"])
        .status()
        .map_err(|e| format!("Failed to run playwright: {}", e))?;

    if !status.success() {
        return Err("Failed to install Playwright Chromium browser.".to_string());
    }

    // Mark as successfully completed
    fs::write(&marker_file, "done").ok();
    println!("✅ [4/4] Setup complete! Resuming conversion...\n");

    Ok(())
}

fn ensure_dependencies(files: &[PathBuf]) {
    let mut needs_magick = false;
    let mut needs_soffice = false;
    let mut needs_jupyter = false;

    for f in files {
        let ext = normalize_ext(&f.extension().unwrap_or_default().to_string_lossy());
        if IMAGE_TYPES.contains(&ext.as_str()) { needs_magick = true; }
        if DOC_TYPES.contains(&ext.as_str()) { needs_soffice = true; }
        if NOTEBOOK_TYPES.contains(&ext.as_str()) { needs_jupyter = true; }
    }

    let mut missing = vec![];

    if needs_magick && !command_exists("magick") {
        missing.push("ImageMagick (magick)".to_string());
    }

    if needs_soffice && !command_exists(get_soffice_cmd()) {
        missing.push("LibreOffice (soffice)".to_string());
    }

    if needs_jupyter {
        if !command_exists("python3") && !command_exists("python") {
            missing.push("Python (python3 or python) - Required to build Jupyter environment".to_string());
        } else if let Err(e) = ensure_python_venv() {
            missing.push(format!("Jupyter Environment Setup Failed: {}", e));
        }
    }

    if !missing.is_empty() {
        println!("\n❌ Missing required dependencies or setups:");
        for m in missing {
            println!("  • {}", m);
        }
        println!("\nPlease resolve the above issues and try again.");
        std::process::exit(1);
    }
}