use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy)]
enum Action {
    Extract,
    List,
    Test,
}

enum Backend {
    SevenZip(String),
    Unar(String),
}

struct Options {
    action: Action,
    archive: PathBuf,
    output: Option<PathBuf>,
    overwrite: bool,
    quiet: bool,
    animate: bool,
    guided: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("\n  {}  {message}\n", paint("Error", "1;31"));
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        return if io::stdin().is_terminal() {
            interactive()
        } else {
            Err("No archive supplied. Try `smartextract --help`.".into())
        };
    }
    if args.iter().any(|a| a == "-h" || a == "--help") {
        help();
        return Ok(());
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("smartextract {VERSION}");
        return Ok(());
    }
    execute(parse(&args)?)
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut action = Action::Extract;
    let mut archive = None;
    let mut output = None;
    let mut overwrite = false;
    let mut quiet = false;
    let mut animate = true;
    let mut i = 0;
    if matches!(
        args.first().map(String::as_str),
        Some("extract" | "x" | "list" | "l" | "test" | "t")
    ) {
        action = match args[0].as_str() {
            "list" | "l" => Action::List,
            "test" | "t" => Action::Test,
            _ => Action::Extract,
        };
        i += 1;
    }
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                output = Some(PathBuf::from(args.get(i).ok_or("--output needs a folder")?));
            }
            "-y" | "--overwrite" => overwrite = true,
            "-q" | "--quiet" => quiet = true,
            "--no-animation" => animate = false,
            arg if arg.starts_with('-') => return Err(format!("Unknown option `{arg}`.")),
            arg if archive.is_none() => archive = Some(PathBuf::from(arg)),
            arg => return Err(format!("Unexpected argument `{arg}`.")),
        }
        i += 1;
    }
    Ok(Options {
        action,
        archive: archive.ok_or("Please choose an archive.")?,
        output,
        overwrite,
        quiet,
        animate,
        guided: false,
    })
}

fn execute(opts: Options) -> Result<(), String> {
    if !opts.archive.is_file() {
        return Err(format!("Archive not found: {}", opts.archive.display()));
    }
    ensure_supported(&opts.archive)?;
    let requested = opts
        .output
        .clone()
        .unwrap_or_else(|| default_output(&opts.archive));
    let output = if opts.output.is_none() && requested.exists() {
        unique_destination(&requested)
    } else {
        requested
    };
    let backend = find_backend(&opts.archive, opts.action).ok_or_else(dependency_message)?;
    if !opts.quiet && !opts.guided {
        show_job(&opts, &output);
    }

    let before = folder_stats(&output);

    let mut cmd = build_command(&backend, &opts, &output)?;
    let capture = !matches!(opts.action, Action::List);
    let animated = capture && !opts.quiet && opts.animate && io::stdout().is_terminal();
    let started = Instant::now();
    let (status, diagnostics) = if animated {
        run_animated(&mut cmd, opts.action, started)?
    } else if capture {
        let result = cmd
            .output()
            .map_err(|e| format!("Could not start extraction engine: {e}"))?;
        (
            result.status,
            String::from_utf8_lossy(&result.stderr).into_owned(),
        )
    } else {
        (
            cmd.status()
                .map_err(|e| format!("Could not start extraction engine: {e}"))?,
            String::new(),
        )
    };
    if !status.success() {
        let detail = useful_error(&diagnostics);
        return Err(if detail.is_empty() {
            format!("Extraction engine failed ({status}).")
        } else {
            format!("{detail}\n         No success was reported; check the archive and try again.")
        });
    }
    let after = folder_stats(&output);
    if matches!(opts.action, Action::Extract) && after == before && before.0 > 0 {
        return Err(
            "Nothing was extracted because every destination file already exists.\n         Try `--overwrite` or choose another output folder."
                .to_string(),
        );
    }
    if !opts.quiet {
        match opts.action {
            Action::Extract => show_success(&output, started.elapsed()),
            Action::List => {}
            Action::Test => println!(
                "  {}  Archive verified in {}\n",
                paint("◆", "1;32"),
                format_duration(started.elapsed())
            ),
        }
    }
    Ok(())
}

fn build_command(backend: &Backend, opts: &Options, output: &Path) -> Result<Command, String> {
    match backend {
        Backend::SevenZip(program) => {
            let mut cmd = Command::new(program);
            match opts.action {
                Action::Extract => {
                    fs::create_dir_all(output)
                        .map_err(|e| format!("Could not create {}: {e}", output.display()))?;
                    cmd.arg("x")
                        .arg(&opts.archive)
                        .arg(format!("-o{}", output.display()))
                        .arg(if opts.overwrite { "-aoa" } else { "-aos" });
                }
                Action::List => {
                    cmd.arg("l").arg(&opts.archive);
                }
                Action::Test => {
                    cmd.arg("t").arg(&opts.archive);
                }
            }
            cmd.arg("-bsp0");
            Ok(cmd)
        }
        Backend::Unar(program) => {
            fs::create_dir_all(output)
                .map_err(|e| format!("Could not create {}: {e}", output.display()))?;
            let mut cmd = Command::new(program);
            cmd.arg("-D")
                .arg("-o")
                .arg(output)
                .arg(if opts.overwrite { "-f" } else { "-s" })
                .arg(&opts.archive);
            Ok(cmd)
        }
    }
}

fn interactive() -> Result<(), String> {
    show_logo();
    println!(
        "\n  {}",
        paint("Drop an archive here or enter its path", "2")
    );
    print!("  {}  ", paint("›", "1;38;5;81"));
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| e.to_string())?;
    let path = input.trim().trim_matches(['\'', '"']);
    if path.is_empty() {
        return Err("No archive selected.".into());
    }
    let archive = PathBuf::from(path);
    if !archive.is_file() {
        return Err(format!("Archive not found: {}", archive.display()));
    }
    ensure_supported(&archive)?;
    let requested = default_output(&archive);
    let destination = if requested.exists() {
        unique_destination(&requested)
    } else {
        requested
    };
    let size = fs::metadata(&archive)
        .map(|m| human_size(m.len()))
        .unwrap_or_else(|_| "—".into());
    let kind = archive
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or("archive")
        .to_ascii_uppercase();
    println!("\n  {}  {}", paint("Archive", "2"), archive.display());
    println!("  {}   {}", paint("Output", "2"), destination.display());
    println!(
        "  {}     {} {}",
        paint("Type", "2"),
        paint(&kind, "1;38;5;141"),
        paint(&format!("· {size}"), "2")
    );
    if destination != default_output(&archive) {
        println!(
            "  {}  {}",
            paint("Note", "2"),
            paint(
                "The original destination exists, so a new folder will be used.",
                "33"
            )
        );
    }
    print!("  {}  ", paint("Extract? [Y/n]", "1"));
    io::stdout().flush().map_err(|e| e.to_string())?;
    input.clear();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| e.to_string())?;
    if matches!(input.trim().to_ascii_lowercase().as_str(), "n" | "no") {
        return Ok(());
    }
    execute(Options {
        action: Action::Extract,
        archive,
        output: Some(destination),
        overwrite: false,
        quiet: false,
        animate: true,
        guided: true,
    })
}

fn show_logo() {
    println!(
        "\n  {}{}",
        paint("smart", "1;38;5;81"),
        paint("/extract", "1;38;5;141")
    );
    println!("  {}", paint("archives, made effortless", "2"));
}

fn show_job(opts: &Options, output: &Path) {
    show_logo();
    let size = fs::metadata(&opts.archive)
        .map(|m| human_size(m.len()))
        .unwrap_or_else(|_| "—".into());
    let kind = opts
        .archive
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or("archive")
        .to_ascii_uppercase();
    println!("\n  {}  {}", paint("Archive", "2"), opts.archive.display());
    if matches!(opts.action, Action::Extract) {
        println!("  {}   {}", paint("Output", "2"), output.display());
    }
    println!(
        "  {}     {} {}\n",
        paint("Type", "2"),
        paint(&kind, "1;38;5;141"),
        paint(&format!("· {size}"), "2")
    );
}

fn run_animated(
    cmd: &mut Command,
    action: Action,
    started: Instant,
) -> Result<(ExitStatus, String), String> {
    cmd.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start extraction engine: {e}"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or("Could not capture extraction errors.")?;
    let reader = thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let dots = [
        "●·····",
        "·●····",
        "··●···",
        "···●··",
        "····●·",
        "·····●",
        "····●·",
        "···●··",
        "··●···",
        "·●····",
    ];
    let label = match action {
        Action::Extract => "Extracting",
        Action::Test => "Verifying",
        Action::List => "Reading",
    };
    let mut tick = 0;
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            print!("\r\x1b[2K");
            io::stdout().flush().map_err(|e| e.to_string())?;
            return Ok((status, reader.join().unwrap_or_default()));
        }
        print!(
            "\r\x1b[2K  {}  {}  {}  {}",
            paint(frames[tick % frames.len()], "1;38;5;81"),
            paint(label, "1"),
            paint(dots[tick % dots.len()], "38;5;141"),
            paint(&format_duration(started.elapsed()), "2")
        );
        io::stdout().flush().map_err(|e| e.to_string())?;
        tick += 1;
        thread::sleep(Duration::from_millis(80));
    }
}

fn show_success(output: &Path, elapsed: Duration) {
    let (files, bytes) = folder_stats(output);
    let noun = if files == 1 { "file" } else { "files" };
    println!("  {}  {}", paint("◆", "1;38;5;84"), paint("Finished", "1"));
    println!(
        "     {} {} {} {}",
        files,
        noun,
        paint("·", "2"),
        paint(
            &format!("{} · {}", human_size(bytes), format_duration(elapsed)),
            "2"
        )
    );
    println!("     {}\n", paint(&output.display().to_string(), "38;5;81"));
}

fn useful_error(detail: &str) -> String {
    let useful: Vec<&str> = detail
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("error")
                || lower.contains("unsupported")
                || lower.contains("password")
                || lower.contains("failed")
        })
        .take(4)
        .collect();
    if useful.is_empty() {
        detail
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(3)
            .collect::<Vec<_>>()
            .join("\n         ")
    } else {
        useful.join("\n         ")
    }
}

fn find_backend(path: &Path, action: Action) -> Option<Backend> {
    let rar = path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|e| e.eq_ignore_ascii_case("rar"));
    if rar
        && matches!(action, Action::Extract)
        && let Some(program) = find_program(&["unar"])
    {
        return Some(Backend::Unar(program));
    }
    find_program(&["7zz", "7z", "7za"]).map(Backend::SevenZip)
}

fn find_program(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find(|name| {
            Command::new(name)
                .arg("--help")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok()
        })
        .map(|s| (*s).into())
}

fn dependency_message() -> String {
    "No compatible extraction engine found. Install `unar` for RAR and `p7zip-full` for ZIP/7z."
        .into()
}

fn ensure_supported(path: &Path) -> Result<(), String> {
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let supported = [
        ".zip", ".7z", ".rar", ".zipx", ".tar", ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.bz2",
        ".tbz2",
    ];
    if supported.iter().any(|ext| name.ends_with(ext)) {
        Ok(())
    } else {
        Err("Unsupported archive type. Try ZIP, 7z, RAR, or TAR.".into())
    }
}

fn default_output(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("extracted");
    let lower = name.to_ascii_lowercase();
    let suffixes = [
        ".tar.gz", ".tar.xz", ".tar.bz2", ".tbz2", ".tgz", ".txz", ".zipx", ".zip", ".7z", ".rar",
        ".tar",
    ];
    let stem = suffixes
        .iter()
        .find(|s| lower.ends_with(**s))
        .map(|s| &name[..name.len() - s.len()])
        .unwrap_or(name);
    path.parent()
        .unwrap_or(Path::new("."))
        .join(if stem.is_empty() { "extracted" } else { stem })
}

fn unique_destination(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("extracted");
    for number in 1..10_000 {
        let candidate = parent.join(format!("{name} ({number})"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{name} (new)"))
}

fn folder_stats(root: &Path) -> (u64, u64) {
    let mut result = (0, 0);
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                pending.push(entry.path());
            } else if meta.is_file() {
                result.0 += 1;
                result.1 += meta.len();
            }
        }
    }
    result
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs_f32();
    if seconds < 10.0 {
        format!("{seconds:.1}s")
    } else {
        format!("{}s", seconds.round())
    }
}

fn paint(text: &str, code: &str) -> String {
    if io::stdout().is_terminal() && env::var_os("NO_COLOR").is_none() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.into()
    }
}

fn help() {
    println!(
        r#"smart/extract {VERSION}

USAGE
  smartextract <archive> [options]
  smartextract extract <archive> [options]
  smartextract list <archive>
  smartextract test <archive>

OPTIONS
  -o, --output <folder>  Choose the destination folder
  -y, --overwrite        Replace existing files
  -q, --quiet            Only show errors
      --no-animation     Disable motion effects
  -h, --help             Show help
  -V, --version          Show version

Run without arguments for the guided interface.
Supported: ZIP, 7z, RAR, ZIPX, and common TAR formats."#
    );
}
