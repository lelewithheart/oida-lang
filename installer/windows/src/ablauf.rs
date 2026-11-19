//! Was der Installer der Reihe nach tut, und wie er alles wieder entfernt.

use std::io::{BufRead, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fs};

use crate::system::{self, Schluessel};
use crate::{paket, sha256};

/// Alles, was installiert wird (siehe `build.rs`).
static INHALT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/inhalt.bin"));
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Die Ausgabe von llvm-mingw, mit der OIDA gebaut und getestet ist. Größe und
/// Prüfsumme stammen von der Seite der Ausgabe bei GitHub (mstorsjo/llvm-mingw).
const LLVM_AUSGABE: &str = "20261104";
const LLVM_DATEI: &str = "llvm-mingw-20261104-ucrt-x86_64.zip";
const LLVM_SHA256: &str = "552191b5754c49a5a20723276a6fc56541ea1922c4c50b7b669022333fc1b40f";
const LLVM_MB_GELADEN: u32 = 190;
const LLVM_MB_AUSGEPACKT: u32 = 720;

/// Die Liste der installierten Dateien, damit beim Entfernen nur weggeht, was von OIDA kam.
const LISTE: &str = "installiert.txt";
/// Unter diesem Namen legt sich der Installer selbst in den Ordner, zum späteren Entfernen.
const ENTFERNER: &str = "deinstalliern.exe";
const ERWEITERUNG: &str = "oida-vscode.vsix";

const HILFE: &str = "\
oida-setup - installiert OIDA, de Programmiersproch

Aufruf:
  oida-setup [Optionen]

Ohne Optionen frogt er, bevor er wos aus'm Internet hoit oder in VS Code einbaut.

Optionen:
  --ordner <pfad>      durthin kummt OIDA (sunst: %LOCALAPPDATA%\\Programs\\oida)
  --jo                 nix frogn, überoi \"jo\" sogn
  --ohne-pfad          den Ordner ned in dein PATH eintrogn
  --ohne-llvm          llvm-mingw (clang) ned suachn und ned hoin
  --ohne-vscode        de VS-Code-Erweiterung ned installiern
  --llvm-ordner <pfad> durthin kummt llvm-mingw (sunst: %LOCALAPPDATA%\\Programs\\llvm-mingw;
                       nur durt find's OIDA vo söwa)
  --weg                OIDA wieda entferna
  --hilfe, --version
";

#[derive(Default, Debug)]
struct Wunsch {
    ordner: Option<PathBuf>,
    llvm_ordner: Option<PathBuf>,
    jo: bool,
    ohne_pfad: bool,
    ohne_llvm: bool,
    ohne_vscode: bool,
    weg: bool,
}

/// Wo alles hinkommt und wo es in der Registry steht.
struct Orte {
    ordner: PathBuf,
    llvm: PathBuf,
    /// Dort sucht der Compiler llvm-mingw von selbst.
    llvm_ueblich: PathBuf,
    /// Der Schlüssel mit dem PATH des Benutzers.
    umgebung: String,
    /// Der Eintrag unter "Installierte Apps".
    eintrag: String,
    /// Laufenden Programmen sagen, dass sich der PATH geändert hat.
    melden: bool,
}

impl Orte {
    fn neu(wunsch: &Wunsch) -> Result<Orte, String> {
        let lokal = env::var_os("LOCALAPPDATA")
            .filter(|w| !w.is_empty())
            .map(PathBuf::from)
            .ok_or("Windows sogt ma ned, wo dei Ordner fia Programme is (LOCALAPPDATA fehlt). Gib --ordner au.")?;
        // Für die Tests des Installers: Mit dieser Umgebungsvariable landen der PATH und der
        // Eintrag unter einem eigenen Schlüssel statt an den echten Stellen.
        let (umgebung, eintrag, melden) = match env::var("OIDA_SETUP_TESTSCHLUESSEL").ok().filter(|w| !w.is_empty()) {
            Some(test) => (format!("{test}\\Environment"), format!("{test}\\Uninstall\\OIDA"), false),
            None => (
                "Environment".to_string(),
                "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\OIDA".to_string(),
                true,
            ),
        };
        let llvm_ueblich = lokal.join("Programs").join("llvm-mingw");
        let ordner = match &wunsch.ordner {
            Some(ordner) => ganz(ordner),
            // Beim Entfernen ohne Angabe: dort, wohin es installiert wurde.
            None => Schluessel::lesen(&eintrag)
                .filter(|_| wunsch.weg)
                .and_then(|s| s.text("InstallLocation").ok().flatten())
                .map(|(ort, _)| PathBuf::from(ort))
                .unwrap_or_else(|| lokal.join("Programs").join("oida")),
        };
        let llvm = wunsch.llvm_ordner.as_deref().map(ganz).unwrap_or_else(|| llvm_ueblich.clone());
        Ok(Orte { ordner, llvm, llvm_ueblich, umgebung, eintrag, melden })
    }
}

/// Ein Pfad von ganz oben, auch wenn er von hier aus angegeben wurde.
fn ganz(pfad: &Path) -> PathBuf {
    std::path::absolute(pfad).unwrap_or_else(|_| pfad.to_path_buf())
}

fn wuensche(argumente: &[String]) -> Result<Wunsch, String> {
    let mut wunsch = Wunsch::default();
    let mut i = 0;
    while i < argumente.len() {
        let argument = argumente[i].as_str();
        let mut wert = |wofuer: &str| -> Result<PathBuf, String> {
            i += 1;
            argumente.get(i).map(PathBuf::from).ok_or(format!("Noch {wofuer} fehlt da Ordner."))
        };
        match argument {
            "--ordner" => wunsch.ordner = Some(wert("--ordner")?),
            "--llvm-ordner" => wunsch.llvm_ordner = Some(wert("--llvm-ordner")?),
            "--jo" => wunsch.jo = true,
            "--ohne-pfad" => wunsch.ohne_pfad = true,
            "--ohne-llvm" => wunsch.ohne_llvm = true,
            "--ohne-vscode" => wunsch.ohne_vscode = true,
            "--weg" => wunsch.weg = true,
            anderes => return Err(format!("De Option {anderes} kenn i ned. Schau mit --hilfe noch.")),
        }
        i += 1;
    }
    Ok(wunsch)
}

/// Stellt eine Frage, auf die man mit Enter (ja) oder n (nein) antwortet.
/// Mit `--jo` wird nicht gefragt. Ist niemand da, der antworten könnte, heißt das nein.
fn frag(frage: &str, wunsch: &Wunsch) -> bool {
    if wunsch.jo {
        return true;
    }
    print!("{frage} [J/n] ");
    let _ = std::io::stdout().flush();
    let mut antwort = String::new();
    match std::io::stdin().lock().read_line(&mut antwort) {
        Ok(0) | Err(_) => {
            println!("(kana do zum Frogn: na)");
            false
        }
        Ok(_) => !matches!(antwort.trim().chars().next(), Some('n' | 'N')),
    }
}

// ------------------------------------------------------------------- PATH

/// Meinen zwei Einträge im PATH denselben Ordner? Groß und klein, die Richtung
/// der Striche, ein Strich am Ende und Anführungszeichen zählen dabei nicht.
fn gleicher_ordner(a: &str, b: &str) -> bool {
    let glatt = |pfad: &str| pfad.trim().trim_matches('"').replace('/', "\\").trim_end_matches('\\').to_lowercase();
    !glatt(a).is_empty() && glatt(a) == glatt(b)
}

/// Der PATH mit dem Ordner hinten dran; `None`, wenn er schon drinsteht.
fn pfad_mit(alt: &str, ordner: &str) -> Option<String> {
    if alt.split(';').any(|eintrag| gleicher_ordner(eintrag, ordner)) {
        return None;
    }
    let alt = alt.trim_end_matches(';');
    Some(if alt.is_empty() { ordner.to_string() } else { format!("{alt};{ordner}") })
}

/// Der PATH ohne den Ordner; `None`, wenn er gar nicht drinsteht. Alles andere bleibt, wie es ist.
fn pfad_ohne(alt: &str, ordner: &str) -> Option<String> {
    let bleibt: Vec<&str> = alt.split(';').filter(|eintrag| !gleicher_ordner(eintrag, ordner)).collect();
    (bleibt.len() != alt.split(';').count()).then(|| bleibt.join(";"))
}

/// Trägt den Ordner in den PATH des Benutzers ein (oder nimmt ihn heraus).
/// Gibt zurück, ob sich etwas geändert hat.
fn pfad_aendern(orte: &Orte, hinein: bool) -> Result<bool, String> {
    let schluessel = Schluessel::anlegen(&orte.umgebung)?;
    // Lässt sich der bisherige PATH nicht lesen, wird er nicht angerührt.
    let (alt, art) = schluessel.text("Path")?.unwrap_or((String::new(), system::REG_EXPAND_SZ));
    let ordner = orte.ordner.display().to_string();
    let neu = if hinein { pfad_mit(&alt, &ordner) } else { pfad_ohne(&alt, &ordner) };
    let Some(neu) = neu else { return Ok(false) };
    if neu.len() > 30_000 {
        return Err("Dei PATH is scho so laung, dass nix mehr eine geht. Trog den Ordner söwa ei.".to_string());
    }
    schluessel.setz_text("Path", &neu, art)?;
    if orte.melden {
        system::umgebung_melden();
    }
    Ok(true)
}

// ---------------------------------------------------------------- Dateien

fn liste_lesen(ordner: &Path) -> Vec<String> {
    fs::read_to_string(ordner.join(LISTE))
        .map(|inhalt| inhalt.lines().map(str::trim).filter(|z| !z.is_empty()).map(str::to_string).collect())
        .unwrap_or_default()
}

/// Wo eine Datei aus dem Paket im Ordner landet.
fn ziel(ordner: &Path, name: &str) -> PathBuf {
    name.split('/').fold(ordner.to_path_buf(), |pfad, teil| pfad.join(teil))
}

/// Packt alles in den Ordner. Gibt die Namen der Dateien und ihre Größe in KB zurück.
fn dateien_hinlegen(orte: &Orte) -> Result<(Vec<String>, u64), String> {
    let eintraege = paket::zerlegen(&paket::auspacken(INHALT)?)?;
    if eintraege.is_empty() {
        return Err("In dem Installer is nix drin: Er is ohne Inhalt baut wordn (nimm installer\\bauen.ps1).".to_string());
    }
    let alte = liste_lesen(&orte.ordner);
    let mut namen = Vec::new();
    let mut bytes = 0u64;
    for eintrag in &eintraege {
        let datei = ziel(&orte.ordner, &eintrag.name);
        let fehler = |e: std::io::Error| match e.kind() {
            std::io::ErrorKind::PermissionDenied => {
                format!("{} kaun i ned schreibn. Rennt grod a oida.exe? Dann moch's zua und probier's nomoi.", datei.display())
            }
            _ => format!("{} kaun i ned schreibn: {e}", datei.display()),
        };
        fs::create_dir_all(datei.parent().unwrap_or(&orte.ordner)).map_err(fehler)?;
        fs::write(&datei, &eintrag.inhalt).map_err(fehler)?;
        bytes += eintrag.inhalt.len() as u64;
        namen.push(eintrag.name.clone());
    }
    // Der Installer selbst kommt dazu: Mit ihm lässt sich später alles entfernen.
    let entferner = orte.ordner.join(ENTFERNER);
    if let Ok(selbst) = env::current_exe()
        && fs::canonicalize(&selbst).ok() != fs::canonicalize(&entferner).ok()
    {
        fs::copy(&selbst, &entferner).map_err(|e| format!("{} kaun i ned schreibn: {e}", entferner.display()))?;
    }
    bytes += fs::metadata(&entferner).map(|m| m.len()).unwrap_or(0);
    namen.push(ENTFERNER.to_string());
    namen.push(LISTE.to_string());
    fs::write(orte.ordner.join(LISTE), namen.join("\r\n") + "\r\n")
        .map_err(|e| format!("{} kaun i ned schreibn: {e}", orte.ordner.join(LISTE).display()))?;
    // Was eine ältere Version hingelegt hat und jetzt nicht mehr dabei ist, kommt weg.
    for alt in alte.iter().filter(|alt| !namen.contains(alt)) {
        let _ = fs::remove_file(ziel(&orte.ordner, alt));
    }
    Ok((namen, bytes / 1024))
}

/// Der Eintrag unter "Installierte Apps" in den Einstellungen von Windows.
fn eintrag_schreiben(orte: &Orte, groesse_kb: u64) -> Result<(), String> {
    let schluessel = Schluessel::anlegen(&orte.eintrag)?;
    let entferner = orte.ordner.join(ENTFERNER);
    schluessel.setz_text("DisplayName", "OIDA", system::REG_SZ)?;
    schluessel.setz_text("DisplayVersion", VERSION, system::REG_SZ)?;
    schluessel.setz_text("Publisher", "lelewithheart", system::REG_SZ)?;
    schluessel.setz_text("InstallLocation", &orte.ordner.display().to_string(), system::REG_SZ)?;
    schluessel.setz_text("UninstallString", &format!("\"{}\" --weg", entferner.display()), system::REG_SZ)?;
    schluessel.setz_zahl("EstimatedSize", groesse_kb.min(u32::MAX as u64) as u32)?;
    schluessel.setz_zahl("NoModify", 1)?;
    schluessel.setz_zahl("NoRepair", 1)
}

// ------------------------------------------------------------------ clang

fn im_pfad(name: &str) -> Option<PathBuf> {
    let pfad = env::var_os("PATH")?;
    env::split_paths(&pfad).map(|ordner| ordner.join(name)).find(|datei| datei.is_file())
}

/// Ein Programm, das Windows mitbringt (curl, tar): zuerst das aus System32, sonst eines aus dem PATH.
fn von_windows(name: &str) -> Option<PathBuf> {
    let eigenes = env::var_os("SystemRoot").map(|wurzel| PathBuf::from(wurzel).join("System32").join(name));
    eigenes.filter(|datei| datei.is_file()).or_else(|| im_pfad(name))
}

enum Clang {
    /// Eines, mit dem OIDA etwas anfangen kann (aus llvm-mingw oder ausdrücklich angegeben).
    Da(PathBuf),
    /// Nur ein gewöhnliches `clang` im PATH: Das kann meistens keine Programme für llvm-mingw bauen.
    NurAnderes(PathBuf),
    Fehlt,
}

/// Sucht `clang` so, wie es auch der Compiler tut (siehe `finde_clang` im Compiler).
fn finde_clang(orte: &Orte) -> Clang {
    if let Some(pfad) = env::var_os("OIDA_CLANG").map(PathBuf::from).filter(|p| p.is_file()) {
        return Clang::Da(pfad);
    }
    if let Some(pfad) = im_pfad("x86_64-w64-mingw32-clang.exe") {
        return Clang::Da(pfad);
    }
    for ordner in [&orte.llvm, &orte.llvm_ueblich] {
        let pfad = ordner.join("bin").join("clang.exe");
        if pfad.is_file() {
            return Clang::Da(pfad);
        }
    }
    match im_pfad("clang.exe") {
        Some(pfad) => Clang::NurAnderes(pfad),
        None => Clang::Fehlt,
    }
}

fn ordner_leer(ordner: &Path) -> bool {
    fs::read_dir(ordner).map(|mut inhalt| inhalt.next().is_none()).unwrap_or(true)
}

/// Lädt llvm-mingw von GitHub, prüft die Datei und packt sie in den Ordner aus.
fn llvm_holen(orte: &Orte) -> Result<(), String> {
    let selber = format!(
        "Du kaunst's a söwa hoin: https://github.com/mstorsjo/llvm-mingw/releases, de Datei {LLVM_DATEI}, \
         und so nach {} auspockn, dass durt da Ordner bin liegt.",
        orte.llvm_ueblich.display()
    );
    if orte.llvm.exists() && !ordner_leer(&orte.llvm) {
        return Err(format!(
            "In {} liegt scho wos, oba ka clang. Des greif i ned au. Ram den Ordner weg und start mi nomoi.",
            orte.llvm.display()
        ));
    }
    let (Some(curl), Some(tar)) = (von_windows("curl.exe"), von_windows("tar.exe")) else {
        return Err(format!("Zum Hoin brauch i curl und tar vo Windows, de find i oba ned. {selber}"));
    };
    // Für die Tests des Installers lässt sich eine andere Quelle angeben (samt ihrer Prüfsumme).
    let quelle = env::var("OIDA_SETUP_LLVM_QUELLE").ok().filter(|w| !w.is_empty()).unwrap_or_else(|| {
        format!("https://github.com/mstorsjo/llvm-mingw/releases/download/{LLVM_AUSGABE}/{LLVM_DATEI}")
    });
    let summe = env::var("OIDA_SETUP_LLVM_SHA256").ok().filter(|w| !w.is_empty()).unwrap_or_else(|| LLVM_SHA256.to_string());

    let arbeit = env::temp_dir().join(format!("oida-setup-{}", std::process::id()));
    let teil = orte.llvm.with_file_name(format!(
        "{}.teil",
        orte.llvm.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "llvm-mingw".to_string())
    ));
    let ergebnis = (|| -> Result<(), String> {
        fs::create_dir_all(&arbeit).map_err(|e| format!("{} kaun i ned aulegn: {e}", arbeit.display()))?;
        let zip = arbeit.join("llvm-mingw.zip");
        println!("      I hoi {quelle}");
        let geladen = Command::new(&curl)
            .args(["--location", "--fail", "--retry", "2", "--progress-bar", "--output"])
            .arg(&zip)
            .arg(&quelle)
            .status()
            .map_err(|e| format!("curl losst si ned startn: {e}"))?;
        if !geladen.success() {
            return Err(format!("Des Hoin hot ned klappt (ka Internet?). {selber}"));
        }
        println!("      I schau noch, ob's de richtige Datei is ...");
        let datei = fs::File::open(&zip).map_err(|e| format!("{} kaun i ned lesn: {e}", zip.display()))?;
        let ist = sha256::von(datei).map_err(|e| format!("{} kaun i ned lesn: {e}", zip.display()))?;
        if !ist.eq_ignore_ascii_case(&summe) {
            return Err(format!(
                "De Datei is ned de, de i erwort (de Prüfsumme passt ned). I nimm's ned her. {selber}"
            ));
        }
        println!("      I pock's aus (des dauert a bissl) ...");
        let _ = fs::remove_dir_all(&teil);
        fs::create_dir_all(&teil).map_err(|e| format!("{} kaun i ned aulegn: {e}", teil.display()))?;
        // In der Zip-Datei steckt alles in einem Ordner mit dem Namen der Ausgabe; der fällt weg.
        let ausgepackt = Command::new(&tar)
            .arg("-xf")
            .arg(&zip)
            .arg("-C")
            .arg(&teil)
            .arg("--strip-components=1")
            .status()
            .map_err(|e| format!("tar losst si ned startn: {e}"))?;
        if !ausgepackt.success() || !teil.join("bin").join("clang.exe").is_file() {
            return Err(format!("Des Auspockn hot ned klappt. {selber}"));
        }
        if orte.llvm.exists() {
            fs::remove_dir(&orte.llvm).map_err(|e| format!("{} is im Weg: {e}", orte.llvm.display()))?;
        }
        fs::rename(&teil, &orte.llvm).map_err(|e| format!("{} kaun i ned aulegn: {e}", orte.llvm.display()))
    })();
    let _ = fs::remove_dir_all(&arbeit);
    if ergebnis.is_err() {
        let _ = fs::remove_dir_all(&teil);
    }
    ergebnis
}

/// Sorgt dafür, dass `clang` da ist. `Ok(true)`: ist da. `Ok(false)`: fehlt, und das war so gewollt.
fn clang_besorgen(orte: &Orte, wunsch: &Wunsch) -> Result<bool, String> {
    match finde_clang(orte) {
        Clang::Da(pfad) => {
            println!("      is scho do: {}", pfad.display());
            return Ok(true);
        }
        Clang::NurAnderes(pfad) => {
            println!("      Im PATH is a clang ({}), oba ned des aus llvm-mingw.", pfad.display());
            println!("      Mit dem kaun OIDA meistens kane Programme baun.");
        }
        Clang::Fehlt => println!("      OIDA braucht clang, um aus deim Programm a .exe z'mochn. Des fehlt no."),
    }
    let frage = format!(
        "      Soll i llvm-mingw hoin? Des san rund {LLVM_MB_GELADEN} MB vo github.com/mstorsjo/llvm-mingw\n      \
         und auspockt rund {LLVM_MB_AUSGEPACKT} MB in {}.",
        orte.llvm.display()
    );
    if !frag(&frage, wunsch) {
        println!("      Guat, dann ned. Ohne clang kaun OIDA oba no nix übersetzn.");
        return Ok(false);
    }
    llvm_holen(orte)?;
    println!("      llvm-mingw liegt jetzt in {}", orte.llvm.display());
    if orte.llvm != orte.llvm_ueblich {
        println!(
            "      Ochtung: Durt suacht OIDA ned vo söwa. Setz OIDA_CLANG auf {}",
            orte.llvm.join("bin").join("clang.exe").display()
        );
    }
    Ok(true)
}

// ---------------------------------------------------------------- VS Code

fn finde_vscode() -> Option<PathBuf> {
    let unter = |name: &str| env::var_os(name).map(|o| PathBuf::from(o).join("Microsoft VS Code").join("bin").join("code.cmd"));
    let lokal = env::var_os("LOCALAPPDATA")
        .map(|o| PathBuf::from(o).join("Programs").join("Microsoft VS Code").join("bin").join("code.cmd"));
    im_pfad("code.cmd").or_else(|| [lokal, unter("ProgramFiles")].into_iter().flatten().find(|datei| datei.is_file()))
}

fn erweiterung_einbauen(orte: &Orte, wunsch: &Wunsch) -> Result<(), String> {
    let vsix = orte.ordner.join(ERWEITERUNG);
    if !vsix.is_file() {
        println!("      (in dem Installer is ka Erweiterung dabei)");
        return Ok(());
    }
    let Some(code) = finde_vscode() else {
        println!("      VS Code find i ned. Wennst as späta host:  code --install-extension \"{}\"", vsix.display());
        return Ok(());
    };
    if !frag("      VS Code is do. Soll i de OIDA-Erweiterung einbaun (Foabn, Fehler beim Tippn, F5)?", wunsch) {
        println!("      Guat, dann ned. Späta geht's mit:  code --install-extension \"{}\"", vsix.display());
        return Ok(());
    }
    let ergebnis = Command::new(&code)
        .arg("--install-extension")
        .arg(&vsix)
        .arg("--force")
        .output()
        .map_err(|e| format!("VS Code losst si ned startn: {e}"))?;
    if ergebnis.status.success() {
        println!("      eingebaut");
        Ok(())
    } else {
        Err(format!("VS Code hot de Erweiterung ned gnumma:\n{}", String::from_utf8_lossy(&ergebnis.stderr).trim()))
    }
}

// ------------------------------------------------------------ die Abläufe

/// Installieren. Gibt zurück, ob am Ende alles da ist, was OIDA braucht.
fn installieren(orte: &Orte, wunsch: &Wunsch) -> Result<bool, String> {
    println!("OIDA {VERSION} - Installiern");
    println!();
    println!("[1/4] Dateien nach {}", orte.ordner.display());
    let (namen, groesse_kb) = dateien_hinlegen(orte)?;
    println!("      {} Dateien, {:.1} MB", namen.len(), groesse_kb as f64 / 1024.0);
    eintrag_schreiben(orte, groesse_kb)?;

    println!("[2/4] PATH (damit ma überoi afoch \"oida\" tippn kaun)");
    if wunsch.ohne_pfad {
        println!("      ausglossn (--ohne-pfad)");
    } else if pfad_aendern(orte, true)? {
        println!("      eintrogn. Güt in jedm Terminal, des ab jetzt aufgeht.");
    } else {
        println!("      steht scho drin");
    }

    println!("[3/4] clang (llvm-mingw)");
    // Schiefgegangen ist etwas anderes als "wollte ich nicht": Nur das Erste ist am Ende ein Fehler.
    let (mut clang_da, mut alles_da) = (true, true);
    if wunsch.ohne_llvm {
        println!("      ausglossn (--ohne-llvm)");
    } else {
        match clang_besorgen(orte, wunsch) {
            Ok(da) => clang_da = da,
            Err(meldung) => {
                println!("      Heast! {meldung}");
                (clang_da, alles_da) = (false, false);
            }
        }
    }

    println!("[4/4] VS Code");
    if wunsch.ohne_vscode {
        println!("      ausglossn (--ohne-vscode)");
    } else if let Err(meldung) = erweiterung_einbauen(orte, wunsch) {
        println!("      Heast! {meldung}");
    }

    println!();
    if clang_da {
        println!("Passt! OIDA is installiert.");
    } else {
        println!("OIDA is installiert, oba clang fehlt no (schau obn): Ohne des kaun's nix übersetzn.");
    }
    println!("Moch a neichs Terminal auf und probier:");
    println!("    oida --version");
    println!("    oida \"{}\" --renn", orte.ordner.join("beispiele").join("hallo.oida").display());
    println!("Wia's weidageht, steht in {}", orte.ordner.join("LIESMICH.txt").display());
    Ok(alles_da)
}

/// Entfernen: genau das, was der Installer hingelegt und eingetragen hat.
fn entfernen(orte: &Orte, wunsch: &Wunsch) -> Result<(), String> {
    let namen = liste_lesen(&orte.ordner);
    if namen.is_empty() {
        return Err(format!("In {} is ka OIDA installiert ({LISTE} fehlt).", orte.ordner.display()));
    }
    println!("OIDA entferna aus {}", orte.ordner.display());
    if !frag("Soll OIDA wirklich weg?", wunsch) {
        println!("Guat, es bleibt ollas, wia's is.");
        return Ok(());
    }
    let selbst = env::current_exe().ok().and_then(|pfad| fs::canonicalize(pfad).ok());
    let mut bleibt_selbst = None;
    let mut ordner: Vec<PathBuf> = Vec::new();
    for name in &namen {
        // Ein Name aus der Liste, der aus dem Ordner hinausführt, wird nicht angefasst.
        if name.contains("..") || name.contains(':') || name.starts_with(['/', '\\']) {
            continue;
        }
        let datei = ziel(&orte.ordner, name);
        if selbst.is_some() && fs::canonicalize(&datei).ok() == selbst {
            // Ein laufendes Programm kann sich unter Windows nicht selbst löschen: Das kommt am Schluss.
            bleibt_selbst = Some(datei);
            continue;
        }
        match fs::remove_file(&datei) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => println!("  {} bleibt lieng: {e}", datei.display()),
        }
        let mut oben = datei.parent();
        while let Some(o) = oben.filter(|o| *o != orte.ordner && o.starts_with(&orte.ordner)) {
            if !ordner.contains(&o.to_path_buf()) {
                ordner.push(o.to_path_buf());
            }
            oben = o.parent();
        }
    }
    // Leere Ordner von innen nach außen; was noch etwas enthält (eigene Dateien), bleibt.
    ordner.sort_by_key(|o| std::cmp::Reverse(o.components().count()));
    for o in &ordner {
        let _ = fs::remove_dir(o);
    }
    if pfad_aendern(orte, false)? {
        println!("  aus'm PATH gnumma");
    }
    system::schluessel_loeschen(&orte.eintrag);
    // Liegt noch etwas im Ordner, das nicht von OIDA ist (außer diesem Programm selbst)?
    let uebrig = fs::read_dir(&orte.ordner).map(|inhalt| inhalt.count()).unwrap_or(0);
    let fremdes = uebrig > usize::from(bleibt_selbst.is_some());
    match bleibt_selbst {
        Some(selbst) => {
            // Kurz warten, bis dieses Programm zu Ende ist, dann die eigene Datei und den Ordner löschen.
            const OHNE_FENSTER: u32 = 0x0800_0000;
            let _ = Command::new("cmd")
                .raw_arg(format!(
                    "/c (ping -n 3 127.0.0.1 & del /q \"{}\" & rmdir \"{}\") >nul 2>&1",
                    selbst.display(),
                    orte.ordner.display()
                ))
                .creation_flags(OHNE_FENSTER)
                .spawn();
        }
        None => {
            let _ = fs::remove_dir(&orte.ordner);
        }
    }
    println!("OIDA is weg.");
    if fremdes {
        println!("Im Ordner liegt no wos, des ned vo mia is; des hob i lieng lossn.");
    }
    if orte.llvm_ueblich.join("bin").join("clang.exe").is_file() {
        println!("llvm-mingw hob i ned augriffn (des brauchn vielleicht aundare a): {}", orte.llvm_ueblich.display());
    }
    println!("De VS-Code-Erweiterung geht weg mit:  code --uninstall-extension lelewithheart.oida");
    Ok(())
}

pub fn haupt() -> ExitCode {
    let argumente: Vec<String> = env::args().skip(1).collect();
    if argumente.iter().any(|a| a == "--hilfe" || a == "-h" || a == "/?") {
        print!("{HILFE}");
        return ExitCode::SUCCESS;
    }
    if argumente.iter().any(|a| a == "--version") {
        println!("oida-setup {VERSION}");
        return ExitCode::SUCCESS;
    }
    let ergebnis = wuensche(&argumente).and_then(|wunsch| {
        let orte = Orte::neu(&wunsch)?;
        let code = if wunsch.weg {
            entfernen(&orte, &wunsch).map(|()| 0)
        } else {
            // 3: installiert, aber es fehlt noch etwas, das OIDA braucht.
            installieren(&orte, &wunsch).map(|alles_da| if alles_da { 0 } else { 3 })
        };
        Ok((code, wunsch.jo))
    });
    let (code, ohne_fragen) = match ergebnis {
        Ok((Ok(code), jo)) => (code, jo),
        Ok((Err(meldung), jo)) => {
            eprintln!("Heast! {meldung}");
            (1, jo)
        }
        Err(meldung) => {
            eprintln!("Heast! {meldung}");
            (2, true)
        }
    };
    // Nach einem Doppelklick ginge das Fenster sofort wieder zu.
    if !ohne_fragen && system::eigenes_fenster() {
        println!();
        print!("Druck Enter zum Zuamochn. ");
        let _ = std::io::stdout().flush();
        let _ = std::io::stdin().lock().read_line(&mut String::new());
    }
    ExitCode::from(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordner_im_pfad_erkennen() {
        assert!(gleicher_ordner("C:\\Users\\x\\oida", "c:\\users\\X\\OIDA\\"));
        assert!(gleicher_ordner(" \"C:/Users/x/oida/\" ", "C:\\Users\\x\\oida"));
        assert!(!gleicher_ordner("C:\\Users\\x\\oida2", "C:\\Users\\x\\oida"));
        assert!(!gleicher_ordner("", ""));
    }

    #[test]
    fn pfad_bekommt_den_ordner_nur_einmal() {
        let ordner = "C:\\Users\\x\\AppData\\Local\\Programs\\oida";
        assert_eq!(pfad_mit("", ordner).unwrap(), ordner);
        assert_eq!(pfad_mit("C:\\a;%USERPROFILE%\\b", ordner).unwrap(), format!("C:\\a;%USERPROFILE%\\b;{ordner}"));
        // Ein Strichpunkt am Ende wird nicht verdoppelt.
        assert_eq!(pfad_mit("C:\\a;", ordner).unwrap(), format!("C:\\a;{ordner}"));
        // Steht er schon drin (auch anders geschrieben), ändert sich nichts.
        assert_eq!(pfad_mit(&format!("C:\\a;{}\\;C:\\b", ordner.to_uppercase()), ordner), None);
    }

    #[test]
    fn pfad_verliert_nur_den_ordner() {
        let ordner = "C:\\oida";
        assert_eq!(pfad_ohne("C:\\a;C:\\oida;C:\\b", ordner).unwrap(), "C:\\a;C:\\b");
        assert_eq!(pfad_ohne("C:\\OIDA\\", ordner).unwrap(), "");
        assert_eq!(pfad_ohne("C:\\a;C:\\oida2;%X%", ordner), None);
        // Was sonst drinsteht, bleibt genau so, auch Leeres und Variablen.
        assert_eq!(pfad_ohne("C:\\a;;%X%\\y;c:/oida", ordner).unwrap(), "C:\\a;;%X%\\y");
    }

    #[test]
    fn pfad_in_der_registry_hinein_und_wieder_heraus() {
        // Unter einem eigenen Schlüssel: Der echte PATH wird nicht angefasst.
        let test = format!("Software\\OIDA-Setup-Test-{}-pfad", std::process::id());
        let orte = Orte {
            ordner: PathBuf::from("C:\\Probe\\oida"),
            llvm: PathBuf::new(),
            llvm_ueblich: PathBuf::new(),
            umgebung: format!("{test}\\Environment"),
            eintrag: format!("{test}\\Uninstall\\OIDA"),
            melden: false,
        };
        let lies = || Schluessel::lesen(&orte.umgebung).and_then(|s| s.text("Path").unwrap());
        // Noch kein PATH da: Er wird angelegt, als Text mit Variablen.
        assert_eq!(pfad_aendern(&orte, true), Ok(true));
        assert_eq!(lies(), Some(("C:\\Probe\\oida".to_string(), system::REG_EXPAND_SZ)));
        assert_eq!(pfad_aendern(&orte, true), Ok(false));
        // Ein vorhandener PATH behält seine Art und seinen Inhalt.
        Schluessel::anlegen(&orte.umgebung).unwrap().setz_text("Path", "C:\\a;%USERPROFILE%\\b", system::REG_SZ).unwrap();
        assert_eq!(pfad_aendern(&orte, true), Ok(true));
        assert_eq!(lies(), Some(("C:\\a;%USERPROFILE%\\b;C:\\Probe\\oida".to_string(), system::REG_SZ)));
        assert_eq!(pfad_aendern(&orte, false), Ok(true));
        assert_eq!(lies(), Some(("C:\\a;%USERPROFILE%\\b".to_string(), system::REG_SZ)));
        assert_eq!(pfad_aendern(&orte, false), Ok(false));
        // Ist der PATH kein Text, wird er nicht überschrieben.
        Schluessel::anlegen(&orte.umgebung).unwrap().setz_zahl("Path", 5).unwrap();
        assert!(pfad_aendern(&orte, true).is_err());
        // Der Eintrag unter "Apps".
        eintrag_schreiben(&orte, 1234).unwrap();
        let eintrag = Schluessel::lesen(&orte.eintrag).unwrap();
        assert_eq!(eintrag.text("DisplayName").unwrap().unwrap().0, "OIDA");
        assert_eq!(eintrag.text("UninstallString").unwrap().unwrap().0, "\"C:\\Probe\\oida\\deinstalliern.exe\" --weg");
        drop(eintrag);
        system::schluessel_loeschen(&test);
        assert!(Schluessel::lesen(&test).is_none());
    }

    #[test]
    fn optionen() {
        let w = wuensche(&["--ordner".to_string(), "x".to_string(), "--jo".to_string(), "--ohne-llvm".to_string()]).unwrap();
        assert!(w.jo && w.ohne_llvm && !w.weg && w.ordner == Some(PathBuf::from("x")));
        assert!(wuensche(&["--ordner".to_string()]).is_err());
        assert!(wuensche(&["--wos".to_string()]).unwrap_err().contains("--wos"));
    }
}
