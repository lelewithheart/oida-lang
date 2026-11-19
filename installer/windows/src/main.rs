//! `oida-setup.exe`: installiert OIDA unter Windows, für den angemeldeten
//! Benutzer und ohne Admin-Rechte. Gebaut wird es mit `installer/bauen.ps1`.
//!
//! Der Compiler, die Beispiele und die VS-Code-Erweiterung stecken in dieser
//! einen Datei (siehe `build.rs` und `paket.rs`). Was OIDA zusätzlich braucht,
//! `clang` aus llvm-mingw, lädt der Installer auf Wunsch von GitHub nach.

#[cfg(windows)]
mod ablauf;
// Das Packen aus `paket.rs` braucht nur `build.rs`; hier wird nur ausgepackt.
#[cfg(windows)]
#[allow(dead_code)]
mod paket;
#[cfg(windows)]
mod sha256;
#[cfg(windows)]
mod system;

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    ablauf::haupt()
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("oida-setup is nur fia Windows. Unter Linux nimm install.sh oda des .deb-Paket.");
    std::process::ExitCode::from(1)
}
