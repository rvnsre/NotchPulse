use pulse_core::recipes::{Host, Platform, Recipe};
use std::{env, path::{Path, PathBuf}, process::Command};

pub fn executable(name: &str) -> Option<PathBuf> {
    let mut paths: Vec<_> = env::split_paths(&env::var_os("PATH").unwrap_or_default()).filter(|p| p.is_absolute()).collect();
    if let Some(home) = dirs::home_dir() { paths.push(home.join(".cargo/bin")); }
    #[cfg(target_os="macos")]
    paths.extend([PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/usr/local/bin")]);
    #[cfg(windows)]
    if let Some(local) = env::var_os("LOCALAPPDATA") { paths.push(PathBuf::from(local).join("Microsoft/WindowsApps")); }
    paths.into_iter()
        .flat_map(|p| {
            #[cfg(windows)]
            let names = vec![p.join(format!("{name}.exe")), p.join(name)];
            #[cfg(not(windows))]
            let names = vec![p.join(name)];
            names
        }).find(|p| is_executable(p))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.is_file() && path.metadata().is_ok_and(|m|m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    { path.is_file() }
}

pub fn detect() -> Host {
    let platform = if cfg!(windows) { Platform::Windows } else if cfg!(target_os="macos") { Platform::MacOS } else { Platform::Linux };
    let shell = if cfg!(windows) { "powershell".into() } else {
        env::var("SHELL").ok().and_then(|s| Path::new(&s).file_name().map(|n| n.to_string_lossy().into_owned()))
            .filter(|s| ["bash","zsh","fish","sh","nu"].contains(&s.as_str())).unwrap_or("sh".into())
    };
    let candidates: &[&str] = match platform {
        Platform::Windows => &["winget", "cargo"],
        Platform::MacOS => &["brew", "cargo"],
        Platform::Linux => &["apt", "pacman", "dnf", "cargo"],
    };
    let package_manager = candidates.iter().find(|n| executable(n).is_some()).copied().unwrap_or("git").into();
    Host { platform, shell, package_manager, arch: env::consts::ARCH.into() }
}

/// The caller supplies only a repository ID. This receives a backend-generated,
/// reviewed argv and starts a visible terminal. Never execute API-provided text.
pub fn execute(recipe: &Recipe, host: &Host) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Home directory unavailable")?;
    let program = executable(&recipe.program).ok_or_else(||format!("{} is unavailable. Install it first or use Copy.",recipe.program))?;
    let argv = std::iter::once(program.to_string_lossy().into_owned()).chain(recipe.args.iter().cloned())
        .map(|arg|pulse_core::recipes::quote(&arg,&host.platform)).collect::<Vec<_>>().join(" ");
    let script = if host.platform==Platform::Windows {format!("& {argv}")} else {argv};
    let mut command = match host.platform {
        Platform::Windows => windows_terminal(&script)?,
        Platform::MacOS => {
            use base64::Engine;
            // AppleScript string only contains base64, never raw external text.
            let payload = base64::engine::general_purpose::STANDARD.encode(script.as_bytes());
            let script = format!("tell application \"Terminal\"\nactivate\ndo script \"cd ~; /bin/zsh -lc \\\"$(printf %s {payload} | /usr/bin/base64 -D)\\\"\"\nend tell");
            let mut c = Command::new("/usr/bin/osascript");
            c.args(["-e", &script]); c
        }
        Platform::Linux => {
            // POSIX quoting is valid for bash/zsh/sh. Fish/nu commands are copied
            // in that host's style, but execution uses a known POSIX launcher.
            let shell = if host.shell == "zsh" { "zsh" } else { "bash" };
            let shell_path = executable(shell).ok_or("Install bash or zsh to run terminal actions")?;
            let script = format!("{script}; result=$?; printf '\\n[NotchPulse] exit %s - press Enter to close' \"$result\"; read -r _");
            let mut found = None;
            for (terminal, flag) in [("gnome-terminal","--"),("konsole","-e"),("xfce4-terminal","-x"),("xterm","-e"),("kitty","--")] {
                if let Some(path) = executable(terminal) {
                    let mut c = Command::new(path);
                    c.arg(flag).arg(&shell_path).args(["-lc", &script]); found = Some(c); break;
                }
            }
            found.ok_or("No supported terminal found. Use Copy instead.")?
        }
    };
    command.current_dir(home);
    // Reap the short-lived launcher without retaining handles on each click.
    let mut child = command.spawn().map_err(|e| format!("Terminal could not start: {e}"))?;
    std::thread::spawn(move || { let _ = child.wait(); });
    Ok(())
}

fn windows_terminal(script: &str) -> Result<Command, String> {
    use base64::Engine;
    // Windows PowerShell -EncodedCommand uses UTF-16LE. No command-line quoting
    // ambiguities, cmd.exe expansion, or downloaded execution-policy bypass.
    let payload = format!("Set-Location $HOME; {script}");
    let bytes: Vec<_> = payload.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    let ps = env::var_os("SystemRoot").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"))
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    if !ps.is_file() { return Err("Windows PowerShell is unavailable".into()); }
    let mut c = if let Some(wt) = executable("wt") {
        let mut c = Command::new(wt); c.arg("new-tab").arg(&ps); c
    } else { Command::new(ps) };
    c.args(["-NoLogo", "-NoProfile", "-NoExit", "-EncodedCommand", &encoded]);
    Ok(c)
}

