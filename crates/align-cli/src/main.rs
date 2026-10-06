use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use align_lib::{Command, Config, Error};
use serde::{Deserialize, Serialize};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
align — line up text in columns by literal or regex patterns

USAGE
    align [global flags] <pattern> [flags] [<pattern> [flags] ...]  < input
    align --json           read {\"pattern\", \"lines\"} JSON on stdin (editor plugins)

Arguments are joined with spaces and parsed as one string, so to include a
space in a literal, quote it inside the argument: align \"'= '\".

PATTERNS (matched left to right; each is searched after the previous match)
    =  ->  foo         literal (no spaces); a '-' that isn't a flag is literal
    '= '  \"'\"  `\"`      quoted literal
    /=+/i  /\\d+/g       regex; flags i m s x (fancy_regex only) g (= -n *)

GLOBAL FLAGS
    -g PAT     only align lines matching PAT      -e   only lines matching every pattern
    -v PAT     skip lines matching PAT            -d   delete lines with no match
    -E ENGINE  fancy_regex (default) | regress    -D   delete lines missing a pattern

PATTERN FLAGS (after a pattern; before the first pattern = default for all)
    -n N|*     match up to N times (default 1)    -p N   spaces on both sides (default 1)
    -l / -r    line up left / right edges         -pl N  -pr N   one side only
    -j         right-justify text before match    -f C   fill char for the gap
    -c PAT     insert fill before last PAT in     -w PAT word-boundary chars
               the text before the match          -W     no word boundary
    -C         same as -j

OTHER
    -h, --help     --version     --config-path

CONFIG
    ~/.config/align/config.toml (created on first run; override with $ALIGN_CONFIG,
    or ALIGN_CONFIG= for built-in defaults)
";

#[derive(Deserialize)]
struct JsonRequest {
    /// The pattern string, exactly as typed.
    pattern: Option<String>,
    /// Legacy (v1): argv-style list, joined with spaces.
    args: Option<Vec<String>>,
    lines: Vec<String>,
    tabstop: Option<usize>,
}

#[derive(Serialize)]
struct JsonResponse {
    output: Option<Vec<String>>,
    error: Option<JsonError>,
}

#[derive(Serialize)]
struct JsonError {
    message: String,
    /// 0-based character offset into the pattern, if known.
    col: Option<usize>,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("-h" | "--help") => {
            print!("{HELP}");
            return if args.is_empty() { ExitCode::from(2) } else { ExitCode::SUCCESS };
        }
        Some("--version" | "-V") if args.len() == 1 => {
            println!("align {VERSION}");
            return ExitCode::SUCCESS;
        }
        Some("--config-path") if args.len() == 1 => {
            match config_path() {
                Some(p) => println!("{}", p.display()),
                None => println!("(none)"),
            }
            return ExitCode::SUCCESS;
        }
        Some("--json") if args.len() == 1 => return run_json(),
        _ => {}
    }

    let pattern = args.join(" ");
    let config = match load_config() {
        Ok(c) => c,
        Err(e) => return fail(&e.to_string()),
    };
    let command = match Command::parse(&pattern, &config) {
        Ok(c) => c,
        Err(e) => return fail(&describe(&pattern, &e)),
    };

    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        return fail(&format!("reading stdin: {e}"));
    }
    let newline = if input.contains("\r\n") { "\r\n" } else { "\n" };
    let trailing = input.ends_with('\n');
    let lines: Vec<&str> = input.lines().collect();

    let output = command.apply(&lines);
    let mut out = io::stdout().lock();
    let mut text = output.join(newline);
    if trailing && !output.is_empty() {
        text.push_str(newline);
    }
    if out.write_all(text.as_bytes()).and_then(|_| out.flush()).is_err() {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn fail(message: &str) -> ExitCode {
    eprintln!("align: {message}");
    ExitCode::from(2)
}

/// The error plus the pattern with a caret under the offending column.
fn describe(pattern: &str, e: &Error) -> String {
    match e.col {
        Some(col) => format!("{}\n    {pattern}\n    {}^", e.message, " ".repeat(col)),
        None => e.message.clone(),
    }
}

fn run_json() -> ExitCode {
    let mut input = String::new();
    let response = match io::stdin().read_to_string(&mut input) {
        Err(e) => error_response(format!("reading stdin: {e}"), None),
        Ok(_) => match serde_json::from_str::<JsonRequest>(&input) {
            Err(e) => error_response(format!("invalid JSON request: {e}"), None),
            Ok(req) => handle(req),
        },
    };
    println!("{}", serde_json::to_string(&response).expect("serializable"));
    ExitCode::SUCCESS
}

fn handle(req: JsonRequest) -> JsonResponse {
    let pattern = match (req.pattern, req.args) {
        (Some(p), _) => p,
        (None, Some(args)) => args.join(" "),
        (None, None) => return error_response("request needs \"pattern\"".into(), None),
    };
    let mut config = match load_config() {
        Ok(c) => c,
        Err(e) => return error_response(e.to_string(), None),
    };
    if let Some(ts) = req.tabstop.filter(|&t| t > 0) {
        config.tabstop = ts;
    }
    match Command::parse(&pattern, &config) {
        Ok(cmd) => JsonResponse { output: Some(cmd.apply(&req.lines)), error: None },
        Err(e) => error_response(e.message, e.col),
    }
}

fn error_response(message: String, col: Option<usize>) -> JsonResponse {
    JsonResponse { output: None, error: Some(JsonError { message, col }) }
}

fn config_path() -> Option<PathBuf> {
    match std::env::var_os("ALIGN_CONFIG") {
        Some(p) if p.is_empty() => None,
        Some(p) => Some(PathBuf::from(p)),
        None => directories::ProjectDirs::from("", "", "Align")
            .map(|d| d.config_dir().join("config.toml")),
    }
}

fn load_config() -> Result<Config, Error> {
    let Some(path) = config_path() else {
        return Ok(Config::default());
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => Config::from_toml(&text)
            .map_err(|e| Error::new(format!("{} ({})", e.message, path.display()))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // First run: leave a documented, fully commented config behind.
            if std::env::var_os("ALIGN_CONFIG").is_none() {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(&path, Config::template());
            }
            Ok(Config::default())
        }
        Err(e) => Err(Error::new(format!("reading {}: {e}", path.display()))),
    }
}
