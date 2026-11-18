//! Leistungsvergleich: dieselben Programme in OIDA und in anderen Sprachen.
//!
//! Aufruf (im Projektordner):
//!
//! ```text
//! cargo run --release -p oida-messen                  alles messen
//! cargo run --release -p oida-messen -- fib collatz   nur diese Programme
//! cargo run --release -p oida-messen -- --ohne python diese Sprache auslassen
//! ```
//!
//! Die Programme liegen in `leistung/programme/<name>/`, je eines pro Sprache.
//! Gemessen wird:
//!  - Zeit: vom Starten bis zum Ende des Programms, die schnellste von mehreren Runden
//!  - Speicher: der höchste Arbeitsspeicher, den das Programm gebraucht hat
//!  - Bauen: wie lange das Übersetzen dauert, und wie groß das Ergebnis ist
//!
//! Jede Sprache muss dieselbe Ausgabe liefern, sonst zählt die Messung nicht.
//! Das Ergebnis steht am Ende in `leistung/ERGEBNIS.md`.

use std::ffi::c_void;
use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// ------------------------------------------------------- Windows fragen

#[repr(C)]
#[derive(Default)]
struct SpeicherZaehler {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn K32GetProcessMemoryInfo(prozess: *mut c_void, zaehler: *mut SpeicherZaehler, groesse: u32) -> i32;
}

/// Der höchste Arbeitsspeicher eines (beendeten) Prozesses in Bytes.
fn speicher_spitze(prozess: &std::process::Child) -> u64 {
    let mut zaehler = SpeicherZaehler { cb: size_of::<SpeicherZaehler>() as u32, ..Default::default() };
    let ok = unsafe { K32GetProcessMemoryInfo(prozess.as_raw_handle(), &mut zaehler, zaehler.cb) };
    if ok != 0 { zaehler.peak_working_set_size as u64 } else { 0 }
}

// ------------------------------------------------------------- Sprachen

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Sprache {
    Oida,
    C,
    Rust,
    Java,
    JavaScript,
    Python,
}

const SPRACHEN: [Sprache; 6] =
    [Sprache::Oida, Sprache::C, Sprache::Rust, Sprache::Java, Sprache::JavaScript, Sprache::Python];

impl Sprache {
    fn name(self) -> &'static str {
        match self {
            Sprache::Oida => "OIDA",
            Sprache::C => "C",
            Sprache::Rust => "Rust",
            Sprache::Java => "Java",
            Sprache::JavaScript => "JavaScript",
            Sprache::Python => "Python",
        }
    }

    /// Wie das Programm in diese Sprache kommt (für den Bericht).
    fn art(self) -> &'static str {
        match self {
            Sprache::Oida => "übersetzt (LLVM)",
            Sprache::C => "übersetzt (LLVM, clang -O2)",
            Sprache::Rust => "übersetzt (LLVM, opt-level 3)",
            Sprache::Java => "Bytecode + JIT",
            Sprache::JavaScript => "JIT (Node)",
            Sprache::Python => "Interpreter",
        }
    }

    fn datei(self, programm: &str) -> String {
        match self {
            Sprache::Oida => format!("{programm}.oida"),
            Sprache::C => format!("{programm}.c"),
            Sprache::Rust => format!("{programm}.rs"),
            Sprache::Java => format!("{}.java", gross(programm)),
            Sprache::JavaScript => format!("{programm}.js"),
            Sprache::Python => format!("{programm}.py"),
        }
    }
}

/// Erster Buchstabe groß (Java will, dass die Datei wie die Klasse heißt).
fn gross(name: &str) -> String {
    let mut zeichen = name.chars();
    match zeichen.next() {
        Some(erstes) => erstes.to_uppercase().collect::<String>() + zeichen.as_str(),
        None => String::new(),
    }
}

/// Wo die Werkzeuge der Sprachen liegen.
struct Werkzeuge {
    oida: Option<PathBuf>,
    clang: Option<PathBuf>,
    /// Der Linker für Rust (aus llvm-mingw).
    mingw_clang: Option<PathBuf>,
    rustc: Option<PathBuf>,
    javac: Option<PathBuf>,
    java: Option<PathBuf>,
    node: Option<PathBuf>,
    python: Option<PathBuf>,
}

fn im_pfad(name: &str) -> Option<PathBuf> {
    let pfad = std::env::var_os("PATH")?;
    std::env::split_paths(&pfad).map(|ordner| ordner.join(name)).find(|datei| datei.is_file())
}

fn falls_da(datei: PathBuf) -> Option<PathBuf> {
    datei.is_file().then_some(datei)
}

fn umgebung(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|w| !w.is_empty()).map(PathBuf::from)
}

/// Java: im PATH, über JAVA_HOME oder das, was DataGrip mitbringt.
fn finde_java(name: &str) -> Option<PathBuf> {
    if let Some(datei) = im_pfad(name) {
        return Some(datei);
    }
    if let Some(datei) = umgebung("JAVA_HOME").and_then(|h| falls_da(h.join("bin").join(name))) {
        return Some(datei);
    }
    let programme = umgebung("LOCALAPPDATA")?.join("Programs");
    let mut ordner: Vec<PathBuf> = fs::read_dir(programme).ok()?.flatten().map(|e| e.path()).collect();
    ordner.sort();
    ordner.into_iter().find_map(|o| falls_da(o.join("jbr").join("bin").join(name)))
}

impl Werkzeuge {
    fn suchen(wurzel: &Path) -> Werkzeuge {
        let mingw = umgebung("LOCALAPPDATA").map(|l| l.join("Programs").join("llvm-mingw").join("bin"));
        let aus_mingw = |name: &str| mingw.as_ref().and_then(|m| falls_da(m.join(name)));
        Werkzeuge {
            oida: falls_da(wurzel.join("target").join("release").join("oida.exe")),
            clang: aus_mingw("clang.exe").or_else(|| im_pfad("clang.exe")),
            mingw_clang: aus_mingw("x86_64-w64-mingw32-clang.exe")
                .or_else(|| im_pfad("x86_64-w64-mingw32-clang.exe")),
            rustc: im_pfad("rustc.exe")
                .or_else(|| umgebung("USERPROFILE").and_then(|h| falls_da(h.join(".cargo").join("bin").join("rustc.exe")))),
            javac: finde_java("javac.exe"),
            java: finde_java("java.exe"),
            node: im_pfad("node.exe"),
            python: im_pfad("python.exe"),
        }
    }

    fn vorhanden(&self, sprache: Sprache) -> bool {
        match sprache {
            Sprache::Oida => self.oida.is_some(),
            Sprache::C => self.clang.is_some(),
            Sprache::Rust => self.rustc.is_some() && self.mingw_clang.is_some(),
            Sprache::Java => self.javac.is_some() && self.java.is_some(),
            Sprache::JavaScript => self.node.is_some(),
            Sprache::Python => self.python.is_some(),
        }
    }

    /// Die erste Zeile von `--version` (für den Bericht).
    fn version(&self, sprache: Sprache) -> String {
        let (programm, argument) = match sprache {
            Sprache::Oida => (&self.oida, "--version"),
            Sprache::C => (&self.clang, "--version"),
            Sprache::Rust => (&self.rustc, "--version"),
            Sprache::Java => (&self.java, "-version"),
            Sprache::JavaScript => (&self.node, "--version"),
            Sprache::Python => (&self.python, "--version"),
        };
        let Some(programm) = programm else { return String::new() };
        let Ok(ergebnis) = Command::new(programm).arg(argument).output() else { return String::new() };
        // java schreibt die Version nach stderr.
        let text = if ergebnis.stdout.is_empty() { ergebnis.stderr } else { ergebnis.stdout };
        let zeile = String::from_utf8_lossy(&text).lines().next().unwrap_or("").trim().to_string();
        // clang hängt die Adresse des Quellcodes an; die braucht im Bericht niemand.
        zeile.split(" (http").next().unwrap_or("").to_string()
    }
}

// --------------------------------------------------------------- Messen

struct Lauf {
    sekunden: f64,
    speicher: u64,
    ausgabe: Vec<u8>,
    erfolg: bool,
}

/// Startet einen Befehl, wartet auf das Ende und misst Zeit und Speicher.
fn laufen(befehl: &mut Command) -> Result<Lauf, String> {
    let anfang = Instant::now();
    let mut prozess = befehl
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("lässt sich nicht starten: {e}"))?;
    let mut ausgabe = Vec::new();
    prozess.stdout.take().unwrap().read_to_end(&mut ausgabe).map_err(|e| e.to_string())?;
    let status = prozess.wait().map_err(|e| e.to_string())?;
    let sekunden = anfang.elapsed().as_secs_f64();
    Ok(Lauf { sekunden, speicher: speicher_spitze(&prozess), ausgabe, erfolg: status.success() })
}

/// Ein fertig gebautes Programm: wie man es startet und was das Bauen gekostet hat.
struct Gebaut {
    programm: PathBuf,
    argumente: Vec<String>,
    bau_sekunden: Option<f64>,
    /// Größe dessen, was beim Bauen entsteht (exe oder .class-Dateien).
    groesse: Option<u64>,
}

fn bauen(sprache: Sprache, quelle: &Path, name: &str, ordner: &Path, w: &Werkzeuge) -> Result<Gebaut, String> {
    let exe = ordner.join(format!("{name}-{}.exe", sprache.name().to_lowercase()));
    let _ = fs::remove_file(&exe);
    let mut befehl = match sprache {
        Sprache::Oida => {
            let mut b = Command::new(w.oida.as_ref().unwrap());
            b.arg(quelle).arg("-o").arg(&exe);
            b
        }
        Sprache::C => {
            let mut b = Command::new(w.clang.as_ref().unwrap());
            b.arg("-O2").arg(quelle).arg("-o").arg(&exe);
            b
        }
        Sprache::Rust => {
            // Entspricht "cargo build --release".
            let mut b = Command::new(w.rustc.as_ref().unwrap());
            b.args(["--edition", "2024", "-C", "opt-level=3", "-C", "strip=debuginfo", "-C"])
                .arg(format!("linker={}", w.mingw_clang.as_ref().unwrap().display()))
                .arg(quelle)
                .arg("-o")
                .arg(&exe);
            b
        }
        Sprache::Java => {
            let klassen = ordner.join(format!("{name}-java"));
            let _ = fs::remove_dir_all(&klassen);
            let mut b = Command::new(w.javac.as_ref().unwrap());
            b.arg("-d").arg(&klassen).arg(quelle);
            let anfang = Instant::now();
            let ergebnis = b.output().map_err(|e| format!("javac lässt sich nicht starten: {e}"))?;
            if !ergebnis.status.success() {
                return Err(format!("javac: {}", String::from_utf8_lossy(&ergebnis.stderr)));
            }
            let groesse = fs::read_dir(&klassen)
                .map(|d| d.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum())
                .ok();
            return Ok(Gebaut {
                programm: w.java.clone().unwrap(),
                argumente: vec!["-cp".into(), klassen.display().to_string(), gross(name)],
                bau_sekunden: Some(anfang.elapsed().as_secs_f64()),
                groesse,
            });
        }
        Sprache::JavaScript | Sprache::Python => {
            let programm = if sprache == Sprache::JavaScript { &w.node } else { &w.python };
            return Ok(Gebaut {
                programm: programm.clone().unwrap(),
                argumente: vec![quelle.display().to_string()],
                bau_sekunden: None,
                groesse: None,
            });
        }
    };
    let anfang = Instant::now();
    let ergebnis = befehl.output().map_err(|e| format!("Compiler lässt sich nicht starten: {e}"))?;
    if !ergebnis.status.success() {
        return Err(format!(
            "lässt sich nicht bauen:\n{}{}",
            String::from_utf8_lossy(&ergebnis.stdout),
            String::from_utf8_lossy(&ergebnis.stderr)
        ));
    }
    let bau_sekunden = Some(anfang.elapsed().as_secs_f64());
    let groesse = fs::metadata(&exe).map(|m| m.len()).ok();
    Ok(Gebaut { programm: exe, argumente: Vec::new(), bau_sekunden, groesse })
}

/// Eine fertige Messung.
struct Messung {
    sprache: Sprache,
    sekunden: f64,
    runden: usize,
    speicher: u64,
    bau_sekunden: Option<f64>,
    groesse: Option<u64>,
    ausgabe: Vec<u8>,
}

fn messen(sprache: Sprache, quelle: &Path, name: &str, ordner: &Path, w: &Werkzeuge) -> Result<Messung, String> {
    let gebaut = bauen(sprache, quelle, name, ordner, w)?;
    let starte = || laufen(Command::new(&gebaut.programm).args(&gebaut.argumente).current_dir(ordner));

    let erster = starte()?;
    if !erster.erfolg {
        return Err("ist mit einem Fehler zu Ende gegangen".to_string());
    }
    // Schnelle Programme öfter laufen lassen, damit Zufall weniger ausmacht.
    let runden = match erster.sekunden {
        s if s < 1.0 => 7,
        s if s < 5.0 => 5,
        s if s < 20.0 => 3,
        _ => 1,
    };
    let mut sekunden = erster.sekunden;
    let mut speicher = erster.speicher;
    for _ in 1..runden {
        let lauf = starte()?;
        if !lauf.erfolg || lauf.ausgabe != erster.ausgabe {
            return Err("liefert nicht jedes Mal dasselbe".to_string());
        }
        sekunden = sekunden.min(lauf.sekunden);
        speicher = speicher.max(lauf.speicher);
    }
    Ok(Messung {
        sprache,
        sekunden,
        runden,
        speicher,
        bau_sekunden: gebaut.bau_sekunden,
        groesse: gebaut.groesse,
        ausgabe: ohne_wagenruecklauf(&erster.ausgabe),
    })
}

/// Manche Sprachen schreiben unter Windows \r\n, manche \n.
fn ohne_wagenruecklauf(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().copied().filter(|b| *b != b'\r').collect()
}

// -------------------------------------------------------------- Bericht

/// Zahl mit Komma statt Punkt.
fn zahl(wert: f64, stellen: usize) -> String {
    format!("{wert:.stellen$}").replace('.', ",")
}

fn zeit(sekunden: f64) -> String {
    if sekunden < 1.0 { format!("{} ms", zahl(sekunden * 1000.0, 0)) } else { format!("{} s", zahl(sekunden, 2)) }
}

fn groesse(bytes: u64) -> String {
    if bytes < 1024 * 1024 {
        format!("{} KB", zahl(bytes as f64 / 1024.0, 0))
    } else {
        format!("{} MB", zahl(bytes as f64 / 1024.0 / 1024.0, 1))
    }
}

fn faktor(wert: f64) -> String {
    if wert < 10.0 { format!("{}×", zahl(wert, 2)) } else { format!("{}×", zahl(wert, 0)) }
}

/// Das heutige Datum als JJJJ-MM-TT (ohne Zusatzbibliothek).
fn heute() -> String {
    let tage = (SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) / 86400) as i64;
    let z = tage + 719468;
    let aera = z.div_euclid(146097);
    let tag_der_aera = z.rem_euclid(146097);
    let jahr_der_aera = (tag_der_aera - tag_der_aera / 1460 + tag_der_aera / 36524 - tag_der_aera / 146096) / 365;
    let tag_im_jahr = tag_der_aera - (365 * jahr_der_aera + jahr_der_aera / 4 - jahr_der_aera / 100);
    let mp = (5 * tag_im_jahr + 2) / 153;
    let tag = tag_im_jahr - (153 * mp + 2) / 5 + 1;
    let monat = if mp < 10 { mp + 3 } else { mp - 9 };
    let jahr = jahr_der_aera + aera * 400 + i64::from(monat <= 2);
    format!("{jahr:04}-{monat:02}-{tag:02}")
}

fn powershell(befehl: &str) -> String {
    Command::new("powershell")
        .args(["-NoProfile", "-Command", befehl])
        .output()
        .map(|e| String::from_utf8_lossy(&e.stdout).trim().to_string())
        .unwrap_or_default()
}

struct Ergebnis {
    name: String,
    was: String,
    messungen: Vec<Messung>,
    /// Sprachen, bei denen etwas nicht geklappt hat, mit dem Grund.
    ausfaelle: Vec<(Sprache, String)>,
}

impl Ergebnis {
    fn von(&self, sprache: Sprache) -> Option<&Messung> {
        self.messungen.iter().find(|m| m.sprache == sprache)
    }
}

fn bericht(ergebnisse: &[Ergebnis], sprachen: &[Sprache], w: &Werkzeuge) -> String {
    let mut aus = String::new();
    let _ = writeln!(aus, "# Leistungsvergleich\n");
    let _ = writeln!(
        aus,
        "Erzeugt mit `cargo run --release -p oida-messen` am {}. Nicht von Hand ändern.\n",
        heute()
    );
    let _ = writeln!(aus, "Rechner: {}, {}\n", powershell("(Get-CimInstance Win32_Processor).Name"), {
        let ram = powershell("[math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory/1GB)");
        format!("{ram} GB Arbeitsspeicher, {}", powershell("(Get-CimInstance Win32_OperatingSystem).Caption"))
    });
    let _ = writeln!(aus, "| Sprache | Art | Version |\n|---|---|---|");
    for s in sprachen {
        let _ = writeln!(aus, "| {} | {} | {} |", s.name(), s.art(), w.version(*s));
    }

    // Überblick: Zeit je Programm und Sprache.
    let _ = writeln!(aus, "\n## Überblick: Zeit\n");
    let _ = writeln!(aus, "Kleiner ist besser. Die schnellste von mehreren Runden, samt Starten des Programms.\n");
    let _ = write!(aus, "| Programm |");
    for s in sprachen {
        let _ = write!(aus, " {} |", s.name());
    }
    let _ = write!(aus, "\n|---|");
    for _ in sprachen {
        let _ = write!(aus, "---:|");
    }
    let _ = writeln!(aus);
    for e in ergebnisse {
        let _ = write!(aus, "| {} |", e.name);
        for s in sprachen {
            let _ = write!(aus, " {} |", e.von(*s).map(|m| zeit(m.sekunden)).unwrap_or_else(|| "–".into()));
        }
        let _ = writeln!(aus);
    }

    // Wie viel langsamer oder schneller als OIDA, im Schnitt.
    let _ = writeln!(aus, "\n## Überblick: Zeit im Verhältnis zu OIDA\n");
    let _ = writeln!(
        aus,
        "1,00× heißt gleich schnell wie OIDA, 2× heißt doppelt so lang, 0,5× halb so lang. \
         Die letzte Zeile ist das geometrische Mittel über alle Programme.\n"
    );
    let _ = write!(aus, "| Programm |");
    for s in sprachen {
        let _ = write!(aus, " {} |", s.name());
    }
    let _ = write!(aus, "\n|---|");
    for _ in sprachen {
        let _ = write!(aus, "---:|");
    }
    let _ = writeln!(aus);
    let mut summen = vec![(0.0f64, 0usize); sprachen.len()];
    for e in ergebnisse {
        let Some(oida) = e.von(Sprache::Oida) else { continue };
        let _ = write!(aus, "| {} |", e.name);
        for (i, s) in sprachen.iter().enumerate() {
            match e.von(*s) {
                Some(m) => {
                    let verhaeltnis = m.sekunden / oida.sekunden;
                    summen[i].0 += verhaeltnis.ln();
                    summen[i].1 += 1;
                    let _ = write!(aus, " {} |", faktor(verhaeltnis));
                }
                None => {
                    let _ = write!(aus, " – |");
                }
            }
        }
        let _ = writeln!(aus);
    }
    let _ = write!(aus, "| **Mittel** |");
    for (summe, anzahl) in &summen {
        if *anzahl > 0 {
            let _ = write!(aus, " **{}** |", faktor((summe / *anzahl as f64).exp()));
        } else {
            let _ = write!(aus, " – |");
        }
    }
    let _ = writeln!(aus);

    // Speicher.
    let _ = writeln!(aus, "\n## Überblick: Speicher\n");
    let _ = writeln!(aus, "Der höchste Arbeitsspeicher, den das Programm gebraucht hat. Kleiner ist besser.\n");
    let _ = write!(aus, "| Programm |");
    for s in sprachen {
        let _ = write!(aus, " {} |", s.name());
    }
    let _ = write!(aus, "\n|---|");
    for _ in sprachen {
        let _ = write!(aus, "---:|");
    }
    let _ = writeln!(aus);
    for e in ergebnisse {
        let _ = write!(aus, "| {} |", e.name);
        for s in sprachen {
            let _ = write!(aus, " {} |", e.von(*s).map(|m| groesse(m.speicher)).unwrap_or_else(|| "–".into()));
        }
        let _ = writeln!(aus);
    }

    // Einzelheiten je Programm.
    let _ = writeln!(aus, "\n## Die Programme im Einzelnen\n");
    for e in ergebnisse {
        let _ = writeln!(aus, "### {}\n\n{}\n", e.name, e.was);
        let _ = writeln!(aus, "| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |");
        let _ = writeln!(aus, "|---|---:|---:|---:|---:|---:|---:|");
        let oida = e.von(Sprache::Oida).map(|m| m.sekunden);
        for m in &e.messungen {
            let _ = writeln!(
                aus,
                "| {} | {} | {} | {} | {} | {} | {} |",
                m.sprache.name(),
                zeit(m.sekunden),
                oida.map(|o| faktor(m.sekunden / o)).unwrap_or_else(|| "–".into()),
                m.runden,
                groesse(m.speicher),
                m.bau_sekunden.map(zeit).unwrap_or_else(|| "–".into()),
                m.groesse.map(groesse).unwrap_or_else(|| "–".into()),
            );
        }
        for (sprache, grund) in &e.ausfaelle {
            let _ = writeln!(aus, "\n{} fehlt: {}", sprache.name(), grund.lines().next().unwrap_or(""));
        }
        let _ = writeln!(aus);
    }
    aus
}

// ----------------------------------------------------------------- Main

fn main() -> std::process::ExitCode {
    let wurzel = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().to_path_buf();
    let programme_ordner = wurzel.join("leistung").join("programme");
    let arbeit = wurzel.join("target").join("leistung");
    fs::create_dir_all(&arbeit).expect("Arbeitsordner lässt sich nicht anlegen");

    // Argumente: Namen von Programmen, und --ohne <sprache>.
    let mut nur: Vec<String> = Vec::new();
    let mut ohne: Vec<String> = Vec::new();
    let mut argumente = std::env::args().skip(1);
    while let Some(argument) = argumente.next() {
        if argument == "--ohne" {
            ohne.push(argumente.next().unwrap_or_default().to_lowercase());
        } else {
            nur.push(argument);
        }
    }

    let w = Werkzeuge::suchen(&wurzel);
    if w.oida.is_none() {
        eprintln!("Heast! target\\release\\oida.exe gibt's ned. Zerst: cargo build --release");
        return std::process::ExitCode::from(2);
    }
    let mut sprachen = Vec::new();
    for s in SPRACHEN {
        if ohne.contains(&s.name().to_lowercase()) {
            continue;
        }
        if w.vorhanden(s) {
            sprachen.push(s);
        } else {
            eprintln!("{} ist auf diesem Rechner nicht da und wird ausgelassen.", s.name());
        }
    }

    let mut namen: Vec<String> = fs::read_dir(&programme_ordner)
        .expect("leistung/programme fehlt")
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| nur.is_empty() || nur.contains(n))
        .collect();
    // Die Reihenfolge im Bericht: erst das Kleine, dann Rechnen, dann Texte und Ausgabe.
    let reihenfolge = ["hallo", "fib", "primzahlen", "collatz", "mandelbrot", "texte", "ausgabe"];
    namen.sort_by_key(|n| (reihenfolge.iter().position(|r| r == n).unwrap_or(usize::MAX), n.clone()));

    let mut ergebnisse = Vec::new();
    let mut alles_richtig = true;
    for name in &namen {
        let ordner = programme_ordner.join(name);
        let was = fs::read_to_string(ordner.join("was.txt")).unwrap_or_default().trim().to_string();
        let erwartet = fs::read(ordner.join(format!("{name}.erwartet"))).ok().map(|b| ohne_wagenruecklauf(&b));
        eprintln!("\n{name}: {was}");
        let mut ergebnis = Ergebnis { name: name.clone(), was, messungen: Vec::new(), ausfaelle: Vec::new() };
        for sprache in &sprachen {
            let quelle = ordner.join(sprache.datei(name));
            if !quelle.is_file() {
                continue;
            }
            eprint!("  {:<11}", sprache.name());
            match messen(*sprache, &quelle, name, &arbeit, &w) {
                Ok(m) => {
                    // Richtig ist, was in der .erwartet-Datei steht; gibt es keine, das, was OIDA ausgibt.
                    let soll = erwartet.as_ref().or(ergebnis.von(Sprache::Oida).map(|o| &o.ausgabe));
                    if soll.is_some_and(|soll| *soll != m.ausgabe) {
                        eprintln!("FALSCHE AUSGABE");
                        ergebnis.ausfaelle.push((*sprache, "falsche Ausgabe".to_string()));
                        alles_richtig = false;
                        continue;
                    }
                    eprintln!("{:>10}  {:>8}  ({} Runden)", zeit(m.sekunden), groesse(m.speicher), m.runden);
                    ergebnis.messungen.push(m);
                }
                Err(grund) => {
                    eprintln!("FEHLER: {grund}");
                    ergebnis.ausfaelle.push((*sprache, grund));
                    alles_richtig = false;
                }
            }
        }
        ergebnisse.push(ergebnis);
    }

    let text = bericht(&ergebnisse, &sprachen, &w);
    if nur.is_empty() && ohne.is_empty() {
        let datei = wurzel.join("leistung").join("ERGEBNIS.md");
        fs::write(&datei, &text).expect("ERGEBNIS.md lässt sich nicht schreiben");
        eprintln!("\nGeschrieben: {}", datei.display());
    } else {
        // Nur ein Teil gemessen: den Bericht zeigen, aber die Datei nicht überschreiben.
        println!("{text}");
    }
    if alles_richtig { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::from(1) }
}
