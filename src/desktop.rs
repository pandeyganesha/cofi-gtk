// ─────────────────────────────────────────────────────────────────────────────
// desktop.rs
//
// Reads every *.desktop file from the standard XDG locations and turns them
// into a flat Vec<DesktopEntry>.  Also provides launch() to exec an app.
//
// Follows the same rules as rofi / wofi / tofi:
//   • Only Type=Application entries are included.
//   • NoDisplay=true and Hidden=true entries are skipped.
//   • ~/.local/share/applications overrides /usr/share/applications when both
//     have the same Name.
//   • Field codes (%f %F %u %U …) are stripped from the Exec line.
// ─────────────────────────────────────────────────────────────────────────────

use std::collections::HashMap;
use std::path::Path;

// ── Data type ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct DesktopEntry {
    /// Human-readable name shown on screen (from Name= in the .desktop file).
    pub name: String,

    /// Shell command to run (from Exec=, field codes stripped).
    pub exec: String,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Load all applications, shuffled randomly so the layout looks organic.
pub fn load_apps() -> Vec<DesktopEntry> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());

    // Process system directories first (lower priority).
    // Then the user directory last so its entries win on duplicates.
    let dirs: Vec<String> = vec![
        "/usr/share/applications".to_string(),
        "/usr/local/share/applications".to_string(),
        format!("{home}/.local/share/applications"),
    ];

    // Use a HashMap keyed by Name so later dirs silently override earlier ones.
    let mut seen: HashMap<String, DesktopEntry> = HashMap::new();

    for dir in &dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for file in entries.flatten() {
                let path = file.path();
                if path.extension().map(|e| e == "desktop").unwrap_or(false) {
                    if let Some(entry) = parse_desktop_file(&path) {
                        // Insert always — last write wins (user > system).
                        seen.insert(entry.name.clone(), entry);
                    }
                }
            }
        }
    }

    let mut apps: Vec<DesktopEntry> = seen.into_values().collect();

    // Shuffle for a random layout — no two runs look exactly the same.
    shuffle(&mut apps);

    apps
}

/// Spawn the app described by an Exec= string and forget about it.
/// cofi exits right after calling this, so we just need the child process
/// to start — we don't need to wait for it or track it.
pub fn launch(exec: &str) {
    // Split the Exec string into the binary and its arguments.
    // e.g.  "firefox --private-window"  →  ["firefox", "--private-window"]
    let parts: Vec<&str> = exec.split_whitespace().collect();
    if parts.is_empty() {
        return;
    }

    // Spawn the process detached from cofi's process group.
    // std::process::Command::spawn() is non-blocking — it starts the child
    // and returns immediately.  Since cofi is about to exit anyway, the child
    // will be re-parented to init by the OS automatically.
    match std::process::Command::new(parts[0])
        .args(&parts[1..])
        // Redirect stdin/stdout/stderr so the child doesn't inherit cofi's
        // Wayland socket or terminal (if any).
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_child) => {
            // Don't call child.wait() — we intentionally leave the child
            // running independently.  The OS will reap it when it exits.
        }
        Err(e) => {
            eprintln!("[cofi] Failed to launch '{exec}': {e}");
        }
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn parse_desktop_file(path: &Path) -> Option<DesktopEntry> {
    let text = std::fs::read_to_string(path).ok()?;

    let mut name:       Option<String> = None;
    let mut exec:       Option<String> = None;
    let mut is_app                      = false;
    let mut hidden                      = false;
    let mut in_section                  = false; // inside [Desktop Entry]?

    for raw_line in text.lines() {
        let line = raw_line.trim();

        // ── Section headers ──────────────────────────────────────────────────
        if line.starts_with('[') {
            in_section = line == "[Desktop Entry]";
            continue;
        }

        if !in_section {
            continue;
        }

        // ── Key = Value pairs ────────────────────────────────────────────────
        if let Some(val) = line.strip_prefix("Type=") {
            if val == "Application" {
                is_app = true;
            }
        } else if let Some(val) = line.strip_prefix("Name=") {
            // Only take the first Name= we see (no locale variants).
            if name.is_none() {
                name = Some(val.to_string());
            }
        } else if let Some(val) = line.strip_prefix("Exec=") {
            if exec.is_none() {
                exec = Some(strip_field_codes(val));
            }
        } else if line == "NoDisplay=true" || line == "Hidden=true" {
            hidden = true;
        }
    }

    if !is_app || hidden {
        return None;
    }

    Some(DesktopEntry {
        name: name?,
        exec: exec?,
    })
}

/// Strip XDG Desktop Entry field codes like %f %F %u %U %d %i %c %k …
fn strip_field_codes(exec: &str) -> String {
    let mut out = String::with_capacity(exec.len());
    let mut chars = exec.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            // Consume the next character (the field code letter) and skip both.
            chars.next();
        } else {
            out.push(c);
        }
    }

    out.trim().to_string()
}

/// A very fast, dependency-free shuffle using a xorshift PRNG seeded from the
/// process ID.  Good enough for random visual layout — not cryptographic.
fn shuffle(v: &mut Vec<DesktopEntry>) {
    if v.len() < 2 {
        return;
    }
    // Seed from PID xor a fixed constant.
    let mut state: u64 = (std::process::id() as u64).wrapping_mul(6364136223846793005)
        ^ 1442695040888963407;

    for i in (1..v.len()).rev() {
        // xorshift64
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state as usize) % (i + 1);
        v.swap(i, j);
    }
}


