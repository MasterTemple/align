use align_lib::{align, Config};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Read, Write};

// ── JSON mode types ───────────────────────────────────────────────────────────
//
// The Neovim plugin (and any other embedding tool) communicates with this binary
// via a single JSON request/response on stdin/stdout.  This completely sidesteps
// shell quoting: patterns that contain `#`, `;`, `"`, etc. are passed as plain
// JSON strings and never touch a shell.
//
// Request:
//   { "args": ["=", "-p", "2"], "lines": ["foo = 1", "foobar = 2"] }
//
// Response (success):
//   { "output": ["foo    = 1", "foobar = 2"], "error": null }
//
// Response (error):
//   { "output": null, "error": "no patterns given" }

#[derive(Deserialize)]
struct JsonRequest {
    /// The argument list — same strings the CLI would receive after the binary name.
    args: Vec<String>,
    /// Input lines (no trailing newlines).
    lines: Vec<String>,
}

#[derive(Serialize)]
struct JsonResponse {
    output: Option<Vec<String>>,
    error: Option<String>,
}

// ── entry point ───────────────────────────────────────────────────────────────

fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();

    // `--json` flag: read one JSON request from stdin, write one JSON response.
    if raw_args.first().map(|s| s.as_str()) == Some("--json") {
        run_json_mode();
        return;
    }

    // Legacy CLI mode — identical behaviour to the original binary.
    if raw_args.is_empty() {
        eprintln!("Usage: align [global-flags] <pattern> [flags] [<pattern> [flags] ...]");
        eprintln!("       align --json   # read {{args, lines}} JSON from stdin");
        eprintln!("Reads lines from stdin and aligns matched patterns.");
        std::process::exit(1);
    }

    let config = Config::load();

    let stdin = io::stdin();
    let lines: Vec<String> = stdin
        .lock()
        .lines()
        .map(|l| l.expect("Failed to read line"))
        .collect();

    match align(&raw_args, &lines, &config) {
        Ok(output) => {
            let stdout = io::stdout();
            let mut out = stdout.lock();
            for line in &output {
                writeln!(out, "{}", line).expect("Failed to write");
            }
        }
        Err(e) => {
            eprintln!("align: {}", e);
            std::process::exit(1);
        }
    }
}

fn run_json_mode() {
    // Read all of stdin as a single JSON object.
    let mut input = String::new();
    io::stdin()
        .lock()
        .read_to_string(&mut input)
        .expect("Failed to read stdin");

    let response: JsonResponse = match serde_json::from_str::<JsonRequest>(&input) {
        Err(e) => JsonResponse {
            output: None,
            error: Some(format!("json parse error: {}", e)),
        },
        Ok(req) => {
            let config = Config::load();
            match align(&req.args, &req.lines, &config) {
                Ok(output) => JsonResponse {
                    output: Some(output),
                    error: None,
                },
                Err(e) => JsonResponse {
                    output: None,
                    error: Some(e),
                },
            }
        }
    };

    let json = serde_json::to_string(&response).expect("serialization failed");
    println!("{}", json);
}
