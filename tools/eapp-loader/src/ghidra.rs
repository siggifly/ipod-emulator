//! Ghidra: the MCP bridge, the launcher, and a query client.
//!
//! **Ghidra answers "who *could* call this"; only the emulator answers "who did".** Every decisive
//! finding in this project has been an arrival counter, and several wrong ones came from reading a
//! static fact as a dynamic one. Pair anything here with `ipod-boot from-idle`, which makes the
//! dynamic half a three-second question.
//!
//! Three modes, and the reasons they are separate:
//!
//! * **`bridge`** — the stdio MCP server, which is what an AI session talks to. Registered as
//!   `claude mcp add ghidra -- <path>/ipod-boot ghidra bridge`. It is a thin indirection on
//!   purpose: the real entry point lives inside a 179 MB third-party checkout that is deliberately
//!   not in git, and registering a path into gitignored material means the integration breaks
//!   silently the moment that tree is rebuilt, moved, or cloned fresh. An MCP server that fails to
//!   start looks, from inside a session, exactly like a server with nothing to say.
//! * **`serve`** — brings Ghidra up **with the program in it**, which is the only state worth
//!   calling "up". The headless server answers `/check_connection` cheerfully and can never hold a
//!   program: both routes that would load one return *"requires GUI mode"*. So it starts the GUI
//!   and then **verifies** a program is really loaded. A launcher that cannot fail is not a
//!   launcher, it is a wish.
//! * **`q`** — the REST client, for when the MCP server is not loaded. MCP servers are loaded when
//!   a session starts, so a freshly registered one is unreachable until the session restarts.

use std::io::{Read, Write};
use std::net::TcpStream;

/// Where the plugin listens. `GHIDRA_URL` overrides, same as the scripts this replaced.
fn url() -> String {
    std::env::var("GHIDRA_URL").unwrap_or_else(|_| "http://127.0.0.1:8089".into())
}

/// `host:port` out of a `http://host:port` base.
fn authority(base: &str) -> String {
    base.trim_start_matches("http://").trim_end_matches('/').to_string()
}

/// The smallest HTTP/1.1 client that can talk to a plugin on loopback.
///
/// Deliberately not a dependency: this crate has two, and neither is an HTTP stack. It speaks to
/// `127.0.0.1` only, reads until the peer closes, and returns the body. It is **not** a general
/// client — no chunked decoding, no redirects, no TLS — and it does not need to be.
fn request(method: &str, path: &str, body: Option<&str>, timeout_s: u64) -> Result<String, String> {
    let base = url();
    let auth = authority(&base);
    let stream = TcpStream::connect(&auth).map_err(|e| format!("{auth}: {e}"))?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(timeout_s)))
        .and_then(|_| stream.set_write_timeout(Some(std::time::Duration::from_secs(timeout_s))))
        .map_err(|e| e.to_string())?;
    let mut stream = stream;
    let b = body.unwrap_or("");
    let req = format!(
        "{method} /{} HTTP/1.1\r\nHost: {auth}\r\nConnection: close\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{b}",
        path.trim_start_matches('/'),
        b.len()
    );
    stream.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut raw = String::new();
    // A closed connection is the end of the body; `Connection: close` above is what makes that true.
    let _ = stream.read_to_string(&mut raw);
    Ok(match raw.split_once("\r\n\r\n") {
        Some((_, body)) => body.to_string(),
        None => raw,
    })
}

fn get(path: &str, timeout_s: u64) -> Result<String, String> {
    request("GET", path, None, timeout_s)
}

/// Is anything answering at all?
fn up() -> bool {
    get("check_connection", 5).map(|b| !b.trim().is_empty()).unwrap_or(false)
}

/// Is a program actually **loaded**? This is the question `check_connection` cannot answer, and the
/// difference between the two is the whole reason `serve` exists.
fn loaded() -> bool {
    get("get_metadata", 10).map(|b| b.contains("\"program_name\"")).unwrap_or(false)
}

/// Pull one string field out of a JSON object.
///
/// A parser would be a dependency for one field. This handles the shape the plugin actually
/// returns — `"key": "…"` with backslash escapes — and returns `None` rather than guessing on
/// anything else, so a caller can fall back to printing the body.
fn json_str(body: &str, key: &str) -> Option<String> {
    let at = body.find(&format!("\"{key}\""))?;
    let rest = &body[at + key.len() + 2..];
    let start = rest.find('"')? + 1;
    let mut out = String::new();
    let mut it = rest[start..].chars();
    while let Some(c) = it.next() {
        match c {
            '"' => return Some(out),
            '\\' => match it.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(o) => out.push(o),
                None => break,
            },
            o => out.push(o),
        }
    }
    None
}

/// `ipod-boot ghidra bridge` — exec the stdio MCP bridge.
///
/// Execs rather than spawns: the bridge speaks MCP over this process's stdin and stdout, and the
/// fewer processes in that path the better.
pub fn bridge(args: &[String]) -> Result<(), String> {
    let home = std::env::var("GHIDRA_MCP_HOME").unwrap_or_else(|_| {
        format!("{}/resources/vendor/ghidra-mcp", repo_root().display())
    });
    if !std::path::Path::new(&home).is_dir() {
        return Err(format!(
            "ghidra-mcp checkout not found at {home}\n\
             set GHIDRA_MCP_HOME, or re-clone it there — see tools/ghidra/README.md"
        ));
    }
    let mut cmd = std::process::Command::new("uv");
    cmd.args(["run", "--project", &home, "bridge-mcp-ghidra"]).args(args);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = cmd.exec(); // only returns on failure
        Err(format!("uv: {e}"))
    }
    #[cfg(not(unix))]
    {
        let st = cmd.status().map_err(|e| format!("uv: {e}"))?;
        std::process::exit(st.code().unwrap_or(1));
    }
}

fn repo_root() -> std::path::PathBuf {
    // The binary lives in a target directory; the tree is found from the source path at build time,
    // which is stable for a developer tool and is how the scripts resolved it too.
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().to_path_buf()
}

/// `ipod-boot ghidra serve [--status]`
pub fn serve(status_only: bool) -> Result<(), String> {
    if status_only {
        if loaded() {
            println!("{}", get("get_metadata", 10)?);
            return Ok(());
        }
        return Err(if up() {
            "plugin is up but NO PROGRAM IS LOADED — this is the state that looks like success"
                .into()
        } else {
            format!("nothing at {}", url())
        });
    }

    if loaded() {
        println!("already up with a program loaded");
        return Ok(());
    }

    if !up() {
        let project = std::env::var("PROJECT").unwrap_or_else(|_| {
            format!("{}/resources/derived/ghidra/retailos.gpr", repo_root().display())
        });
        if !std::path::Path::new(&project).is_file() {
            return Err(format!(
                "no Ghidra project at {project} — build it first:\n  \
                 analyzeHeadless <resources>/derived/ghidra retailos \\\n    \
                 -import <resources>/derived/fw/OSOS_correct.bin \\\n    \
                 -processor ARM:LE:32:v4t -loader BinaryLoader -loader-baseAddr 0x0\n\
                 Flat at base 0 because that is where RetailOS executes — the low alias, not the \
                 0x10000000 view it is loaded through, so Ghidra's addresses match the emulator's."
            ));
        }
        eprintln!("starting Ghidra on {project} …");
        std::process::Command::new("ghidraRun")
            .arg(&project)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("ghidraRun: {e}"))?;
        // The GUI takes the better part of a minute to get its class search and plugins up.
        for _ in 0..40 {
            if up() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
        if !up() {
            return Err("Ghidra did not come up".into());
        }
    }

    let program = std::env::var("PROGRAM").unwrap_or_else(|_| "/OSOS_correct.bin".into());
    eprintln!("opening {program} in a CodeBrowser …");
    let _ = request(
        "POST",
        "tool/launch_codebrowser",
        Some(&format!("{{\"path\":\"{program}\"}}")),
        120,
    );
    for _ in 0..30 {
        if loaded() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
    if !loaded() {
        return Err(
            "CodeBrowser did not end up with a program loaded — do NOT trust query results".into(),
        );
    }
    println!("{}", get("get_metadata", 10)?);
    Ok(())
}

/// `ipod-boot ghidra q xref|fn|dec|raw ARG`
pub fn query(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: ipod-boot ghidra q xref ADDR | fn ADDR | dec ADDR | raw ENDPOINT";
    // The guard is reachability, then usefulness — and they are different questions.
    //
    // The script this replaced tested the body for the literal "Connection OK". The plugin does not
    // say that and evidently has not for some time: it answers `Connected: GhidraMCP plugin running
    // with program '…'`, so the check failed while Ghidra was up with a program loaded, and the
    // tool reported "no Ghidra server" to anyone who ran it. The port reproduced the stale string
    // faithfully, and testing the failure path is what found it.
    if !up() {
        return Err(format!(
            "no Ghidra server at {} — start it with `ipod-boot ghidra serve`",
            url()
        ));
    }
    if !loaded() {
        eprintln!(
            "warning: the plugin is up but NO PROGRAM IS LOADED — every answer below is empty \
             rather than wrong, which is harder to notice. `ipod-boot ghidra serve` fixes it."
        );
    }
    let cmd = args.first().map(String::as_str).ok_or(USAGE)?;
    let arg = args.get(1).map(String::as_str);
    match cmd {
        "xref" => println!("{}", get(&format!("get_xrefs_to?address={}", arg.ok_or(USAGE)?), 10)?),
        "fn" => {
            println!("{}", get(&format!("get_function_by_address?address={}", arg.ok_or(USAGE)?), 10)?)
        }
        "dec" => {
            let b = get(
                &format!("decompile_function_by_address?address={}", arg.ok_or(USAGE)?),
                30,
            )?;
            println!("{}", json_str(&b, "decompiled").unwrap_or(b));
        }
        "raw" => println!("{}", get(arg.ok_or(USAGE)?, 30)?),
        other => return Err(format!("unknown query `{other}`\n{USAGE}")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_json_string_field_is_extracted_with_its_escapes() {
        let b = r#"{"ok":true,"decompiled":"void f(void)\n{\n  return;\n}","n":2}"#;
        assert_eq!(json_str(b, "decompiled").unwrap(), "void f(void)\n{\n  return;\n}");
    }

    /// The negative control: a missing key returns None rather than a guess, so the caller can fall
    /// back to printing the whole body instead of printing something invented.
    #[test]
    fn a_missing_key_is_none_rather_than_a_guess() {
        assert_eq!(json_str(r#"{"ok":true}"#, "decompiled"), None);
    }

    #[test]
    fn the_authority_is_taken_off_the_url() {
        assert_eq!(authority("http://127.0.0.1:8089"), "127.0.0.1:8089");
        assert_eq!(authority("http://127.0.0.1:8089/"), "127.0.0.1:8089");
    }
}
