//! Treiber: führt die Phasen hintereinander aus und macht mit `clang`
//! aus dem LLVM-IR eine `.exe`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::fehler::Fehler;
use crate::woerterbuch::Woerterbuch;
use crate::{irgen, lexer, parser, typen};

/// Die Laufzeitbibliothek, beim Bauen des Compilers fertig übersetzt
/// (siehe `build.rs`). Sie wird in jedes OIDA-Programm gelinkt.
static LAUFZEIT: &[u8] = include_bytes!(env!("OIDA_LAUFZEIT_BIBLIOTHEK"));
const LAUFZEIT_KENNUNG: &str = env!("OIDA_LAUFZEIT_KENNUNG");

/// Das braucht die Laufzeitbibliothek (Rust-Standardbibliothek) von Windows.
/// Stammt aus `cargo rustc -- --print native-static-libs`.
const SYSTEM_BIBLIOTHEKEN: [&str; 6] =
    ["-lkernel32", "-lntdll", "-luserenv", "-lws2_32", "-ldbghelp", "-lsynchronization"];

/// Lexer, Parser, Typprüfung und IR-Erzeugung hintereinander.
pub fn zu_ir(quelltext: &str, wb: &Woerterbuch) -> Result<String, Vec<Fehler>> {
    let tokens = lexer::zerlegen(quelltext, wb).map_err(|f| vec![f])?;
    let mut programm = parser::parsen(tokens, wb).map_err(|f| vec![f])?;
    typen::pruefen(&mut programm, wb)?;
    Ok(irgen::erzeugen(&programm, wb))
}

/// Sucht das Wörterbuch:
/// 1. die Datei, die mit `--woerterbuch` angegeben wurde,
/// 2. die Datei aus der Umgebungsvariable `OIDA_WOERTERBUCH`,
/// 3. `woerterbuch.txt` neben der `oida.exe` oder bis zu zwei Ordner darüber
///    (so findet `target/debug/oida.exe` die Datei im Projektordner),
/// 4. das eingebaute Wörterbuch.
pub fn lade_woerterbuch(angabe: Option<&Path>) -> Result<Woerterbuch, String> {
    let aus_umgebung = env::var_os("OIDA_WOERTERBUCH").filter(|w| !w.is_empty()).map(PathBuf::from);
    let datei = match angabe.map(Path::to_path_buf).or(aus_umgebung) {
        Some(datei) => Some(datei),
        None => neben_dem_compiler(),
    };
    let Some(datei) = datei else { return Ok(Woerterbuch::eingebaut()) };
    let name = datei.display().to_string();
    let inhalt = fs::read_to_string(&datei)
        .map_err(|e| format!("Des Wörterbuch {name} kaun i ned lesn: {e}"))?;
    Woerterbuch::laden(&inhalt, &name).map_err(|e| format!("Im Wörterbuch passt wos ned.\n{e}"))
}

fn neben_dem_compiler() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let mut ordner = exe.parent()?;
    for _ in 0..3 {
        let datei = ordner.join("woerterbuch.txt");
        if datei.is_file() {
            return Some(datei);
        }
        ordner = ordner.parent()?;
    }
    None
}

/// Sucht `clang` aus llvm-mingw:
/// 1. Umgebungsvariable `OIDA_CLANG`,
/// 2. `x86_64-w64-mingw32-clang` im PATH,
/// 3. `%LOCALAPPDATA%\Programs\llvm-mingw\bin\clang.exe`,
/// 4. `clang` im PATH.
pub fn finde_clang() -> Result<PathBuf, String> {
    if let Some(pfad) = env::var_os("OIDA_CLANG").filter(|w| !w.is_empty()) {
        let pfad = PathBuf::from(pfad);
        return if pfad.is_file() {
            Ok(pfad)
        } else {
            Err(format!("OIDA_CLANG zagt auf {}, oba do is nix.", pfad.display()))
        };
    }
    if let Some(pfad) = im_pfad("x86_64-w64-mingw32-clang.exe") {
        return Ok(pfad);
    }
    if let Some(lokal) = env::var_os("LOCALAPPDATA") {
        let pfad = PathBuf::from(lokal).join("Programs").join("llvm-mingw").join("bin").join("clang.exe");
        if pfad.is_file() {
            return Ok(pfad);
        }
    }
    if let Some(pfad) = im_pfad("clang.exe") {
        return Ok(pfad);
    }
    Err("I find ka clang. Installier llvm-mingw (https://github.com/mstorsjo/llvm-mingw/releases, \
         de Datei mit ucrt-x86_64) und gib den bin-Ordner in den PATH, \
         oda setz OIDA_CLANG auf de clang.exe."
        .to_string())
}

fn im_pfad(name: &str) -> Option<PathBuf> {
    let pfad = env::var_os("PATH")?;
    env::split_paths(&pfad).map(|ordner| ordner.join(name)).find(|datei| datei.is_file())
}

/// Legt die eingebaute Laufzeitbibliothek als Datei ab (einmal pro Version)
/// und gibt zurück, wo sie liegt.
fn laufzeit_datei() -> Result<PathBuf, String> {
    let ordner = env::temp_dir().join("oida");
    let ziel = ordner.join(format!("oida-laufzeit-{LAUFZEIT_KENNUNG}.a"));
    if ziel.metadata().map(|m| m.len() == LAUFZEIT.len() as u64).unwrap_or(false) {
        return Ok(ziel);
    }
    let fehler = |e: std::io::Error| format!("I kaun de Laufzeitbibliothek ned oblegn ({}): {e}", ziel.display());
    fs::create_dir_all(&ordner).map_err(fehler)?;
    // Erst unter einem eigenen Namen schreiben und dann umbenennen, damit zwei
    // Compiler gleichzeitig einander keine halbe Datei unterschieben.
    let vorlaeufig = ordner.join(format!("oida-laufzeit-{LAUFZEIT_KENNUNG}.{}.tmp", eindeutig()));
    fs::write(&vorlaeufig, LAUFZEIT).map_err(fehler)?;
    if fs::rename(&vorlaeufig, &ziel).is_err() {
        let _ = fs::remove_file(&vorlaeufig);
        if !ziel.is_file() {
            return Err(format!("I kaun de Laufzeitbibliothek ned oblegn: {}", ziel.display()));
        }
    }
    Ok(ziel)
}

fn eindeutig() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{}-{nanos}", std::process::id())
}

/// Übersetzt LLVM-IR mit `clang` und linkt die Laufzeitbibliothek dazu.
pub fn baue_exe(ir: &str, ausgabe: &Path, optimieren: bool) -> Result<(), String> {
    let clang = finde_clang()?;
    let laufzeit = laufzeit_datei()?;
    let ir_datei = env::temp_dir().join("oida").join(format!("oida-{}.ll", eindeutig()));
    fs::write(&ir_datei, ir).map_err(|e| format!("I kaun {} ned schreibn: {e}", ir_datei.display()))?;

    let ergebnis = Command::new(&clang)
        .arg(format!("--target={}", irgen::ZIEL))
        .arg(if optimieren { "-O2" } else { "-O0" })
        .arg(&ir_datei)
        .arg(&laufzeit)
        .arg("-o")
        .arg(ausgabe)
        // Alles in die .exe hinein, damit sie ohne zusätzliche DLLs läuft.
        .arg("-static")
        // Was vom Programm nicht gebraucht wird, fliegt raus.
        .args(["-Wl,--gc-sections", "-Wl,-s"])
        .args(SYSTEM_BIBLIOTHEKEN)
        .output();
    let _ = fs::remove_file(&ir_datei);

    let ergebnis = ergebnis.map_err(|e| format!("I kaun {} ned startn: {e}", clang.display()))?;
    if !ergebnis.status.success() {
        return Err(format!(
            "clang hot si aufgregt. Des is a Fehler im OIDA-Compiler, ned in deim Programm:\n{}",
            String::from_utf8_lossy(&ergebnis.stderr)
        ));
    }
    Ok(())
}
