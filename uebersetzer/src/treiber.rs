//! Treiber: führt die Phasen hintereinander aus und macht mit `clang`
//! aus dem LLVM-IR ein fertiges Programm (unter Windows eine `.exe`).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ast::{Import, Programm};
use crate::fehler::{Fehler, Position};
use crate::woerterbuch::Woerterbuch;
use crate::{irgen, lexer, parser, typen};

/// Die Laufzeitbibliothek, beim Bauen des Compilers fertig übersetzt
/// (siehe `build.rs`). Sie wird in jedes OIDA-Programm gelinkt.
static LAUFZEIT: &[u8] = include_bytes!(env!("OIDA_LAUFZEIT_BIBLIOTHEK"));
const LAUFZEIT_KENNUNG: &str = env!("OIDA_LAUFZEIT_KENNUNG");

/// Das braucht die Laufzeitbibliothek (Rust-Standardbibliothek) vom System.
/// Stammt aus `cargo rustc -- --print native-static-libs`.
#[cfg(windows)]
const SYSTEM_BIBLIOTHEKEN: &[&str] =
    &["-lkernel32", "-lntdll", "-luserenv", "-lws2_32", "-ldbghelp", "-lsynchronization"];
#[cfg(not(windows))]
const SYSTEM_BIBLIOTHEKEN: &[&str] = &["-lgcc_s", "-lutil", "-lrt", "-lpthread", "-lm", "-ldl", "-lc"];

/// Teile der Bibliothek, die direkt mit Windows reden und die es anderswo noch nicht gibt.
const NUR_WINDOWS: [&str; 2] = ["fensta", "ton"];

/// Die mitgelieferte Bibliothek: in OIDA geschrieben und in den Compiler
/// eingebaut. `BRAUCH listen OIDA` holt den Teil mit diesem Namen dazu.
pub const BIBLIOTHEK: [(&str, &str); 8] = [
    ("listen", include_str!("../../bibliothek/listen.oida")),
    ("texte", include_str!("../../bibliothek/texte.oida")),
    ("mathe", include_str!("../../bibliothek/mathe.oida")),
    ("dateien", include_str!("../../bibliothek/dateien.oida")),
    ("programm", include_str!("../../bibliothek/programm.oida")),
    ("json", include_str!("../../bibliothek/json.oida")),
    ("fensta", include_str!("../../bibliothek/fensta.oida")),
    ("ton", include_str!("../../bibliothek/ton.oida")),
];

/// Funktionen, die in der Laufzeit stecken (VON DRAUSSN), gibt es nur in
/// der mitgelieferten Bibliothek: Zu einem eigenen Namen gäbe es dort nichts.
fn ohne_draussen(programm: &Programm, wb: &Woerterbuch) -> Result<(), Vec<Fehler>> {
    let methoden = programm.klassen.iter().flat_map(|k| k.methoden.iter());
    match programm.funktionen.iter().chain(methoden).find(|f| f.aussen.is_some()) {
        Some(f) => Err(vec![Fehler::neu(
            f.pos,
            format!(
                "{} geht nur in da Bibliothek, de mit OIDA mitkummt.",
                wb.wort(crate::woerterbuch::Schluessel::Extern)
            ),
        )]),
        None => Ok(()),
    }
}

/// Eine Datei, aus der das Programm besteht. Ihre Stelle in der Liste ist
/// die Nummer in [`Position::datei`]; die erste ist die Datei, die übersetzt wird.
pub struct Quelle {
    /// Wie die Datei in Meldungen heißt.
    pub name: String,
    pub text: String,
    /// Wo in der ersten Datei das BRAUCH steht, über das diese Datei (direkt
    /// oder über andere) dazugekommen ist. Dort zeigt der Editor ihre Fehler an.
    pub von: Position,
}

/// Woran man eine Datei wiedererkennt, damit sie nur einmal geladen wird.
fn kennung(pfad: &Path) -> String {
    fs::canonicalize(pfad).unwrap_or_else(|_| pfad.to_path_buf()).to_string_lossy().to_lowercase()
}

/// Liest das Programm samt allem, was es mit BRAUCH dazuholt, und macht einen
/// Syntaxbaum daraus. `pfad` ist die Datei, aus der `quelltext` stammt (wenn
/// es eine gibt): Von ihrem Ordner aus werden die anderen Dateien gesucht.
pub fn lade(pfad: Option<&Path>, quelltext: &str, wb: &Woerterbuch) -> (Vec<Quelle>, Result<Programm, Vec<Fehler>>) {
    let name = pfad.map_or_else(|| "<eingabe>".to_string(), |p| p.display().to_string());
    let mut quellen = vec![Quelle { name, text: quelltext.to_string(), von: Position::default() }];
    let ergebnis = lade_alles(pfad, quelltext, wb, &mut quellen);
    (quellen, ergebnis)
}

fn lade_alles(
    pfad: Option<&Path>,
    quelltext: &str,
    wb: &Woerterbuch,
    quellen: &mut Vec<Quelle>,
) -> Result<Programm, Vec<Fehler>> {
    let tokens = lexer::zerlegen(quelltext, wb).map_err(|f| vec![f])?;
    let mut programm = parser::parsen(tokens, wb).map_err(|f| vec![f])?;
    ohne_draussen(&programm, wb)?;
    if programm.importe.is_empty() {
        return Ok(programm);
    }

    let mut geladen: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(pfad) = pfad {
        geladen.insert(kennung(pfad));
    }
    let ordner = pfad.and_then(Path::parent).map(Path::to_path_buf).unwrap_or_default();
    // Was noch zu holen ist: das BRAUCH, der Ordner der Datei, in der es steht,
    // und die Stelle in der ersten Datei, über die es hereinkommt.
    let mut offen: std::collections::VecDeque<(Import, PathBuf, Position)> =
        programm.importe.iter().map(|i| (i.clone(), ordner.clone(), i.pos)).collect();
    // Die Bibliothek ist mit den Wörtern geschrieben, die beim Bauen des
    // Compilers galten, auch wenn hier ein eigenes Wörterbuch verwendet wird.
    let eingebaut = Woerterbuch::eingebaut();

    while let Some((import, ordner, von)) = offen.pop_front() {
        let melde = |meldung: String| vec![Fehler::neu(import.pos, meldung)];
        let (wieder, name, text, woerter, naechster_ordner) = if import.bibliothek {
            let Some((teil, text)) = BIBLIOTHEK.iter().find(|(teil, _)| *teil == import.ziel) else {
                let namen: Vec<&str> = BIBLIOTHEK.iter().map(|(teil, _)| *teil).collect();
                return Err(melde(format!(
                    "A Bibliothek '{}' gibt's ned. Es gibt: {}. (A eigene Datei ghört in Anführungszeichn.)",
                    import.ziel,
                    namen.join(", ")
                )));
            };
            if !cfg!(windows) && NUR_WINDOWS.contains(teil) {
                return Err(melde(format!(
                    "De Bibliothek '{teil}' gibt's derweil nur unter Windows: Fensta und Ton san fia Linux no ned baut."
                )));
            }
            (format!("oida:{teil}"), format!("bibliothek/{teil}.oida"), text.to_string(), &eingebaut, PathBuf::new())
        } else {
            let mut datei = ordner.join(&import.ziel);
            if datei.extension().is_none() {
                datei.set_extension("oida");
            }
            // In Meldungen überall derselbe Schrägstrich, egal wie der Pfad zusammengesetzt wurde.
            let anzeige = datei.display().to_string().replace('\\', "/");
            let bytes = fs::read(&datei).map_err(|e| {
                melde(match e.kind() {
                    std::io::ErrorKind::NotFound => format!("De Datei {anzeige} find i ned."),
                    _ => format!("De Datei {anzeige} kaun i ned lesn: {e}"),
                })
            })?;
            let text = String::from_utf8(bytes)
                .map_err(|_| melde(format!("{anzeige} is ka UTF-8. Speicher de Datei im Editor ois UTF-8 o.")))?;
            let daneben = datei.parent().map(Path::to_path_buf).unwrap_or_default();
            (kennung(&datei), anzeige, text, wb, daneben)
        };
        if !geladen.insert(wieder) {
            continue; // schon da: Jede Datei zählt nur einmal, auch im Kreis.
        }
        let nummer = quellen.len() as u16;
        quellen.push(Quelle { name, text: text.clone(), von });

        let hier = |mut f: Fehler| {
            f.pos.datei = nummer;
            vec![f]
        };
        let mut tokens = lexer::zerlegen(&text, woerter).map_err(hier)?;
        for token in &mut tokens {
            token.pos.datei = nummer;
            token.ende.datei = nummer;
        }
        let teil = parser::parsen(tokens, woerter).map_err(hier)?;
        if !import.bibliothek {
            ohne_draussen(&teil, wb)?;
        }
        if let Some(erste) = teil.haupt.first() {
            use crate::woerterbuch::Schluessel as S;
            return Err(vec![Fehler::neu(
                erste.pos,
                format!(
                    "In ana Datei, de mit {} dazuaghoit wird, derfn nur {}, {}, {} und {} stehn. \
                     Wos glei laufn soll, ghört in de Datei, de übersetzt wird.",
                    wb.wort(S::Import),
                    wb.wort(S::Function),
                    wb.wort(S::Class),
                    wb.wort(S::Struct),
                    wb.wort(S::Const)
                ),
            )]);
        }
        for weiter in teil.importe {
            offen.push_back((weiter, naechster_ordner.clone(), von));
        }
        programm.konstanten.extend(teil.konstanten);
        programm.strukturen.extend(teil.strukturen);
        programm.klassen.extend(teil.klassen);
        programm.funktionen.extend(teil.funktionen);
    }
    Ok(programm)
}

/// Lexer, Parser, Typprüfung und IR-Erzeugung hintereinander, samt allem,
/// was das Programm mit BRAUCH dazuholt. Gibt auch die Dateien zurück, aus
/// denen es besteht (für die Meldungen).
pub fn uebersetze(pfad: Option<&Path>, quelltext: &str, wb: &Woerterbuch) -> (Vec<Quelle>, Result<String, Vec<Fehler>>) {
    let (quellen, programm) = lade(pfad, quelltext, wb);
    let ir = programm.and_then(|mut programm| {
        typen::pruefen(&mut programm, wb)?;
        Ok(irgen::erzeugen(&programm, wb))
    });
    (quellen, ir)
}

/// Wie [`uebersetze`], für ein Programm, das in keiner Datei steht.
pub fn zu_ir(quelltext: &str, wb: &Woerterbuch) -> Result<String, Vec<Fehler>> {
    uebersetze(None, quelltext, wb).1
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

/// Sucht `clang`. Unter Windows das aus llvm-mingw:
/// 1. Umgebungsvariable `OIDA_CLANG`,
/// 2. `x86_64-w64-mingw32-clang` im PATH,
/// 3. `%LOCALAPPDATA%\Programs\llvm-mingw\bin\clang.exe` (dorthin legt es der Installer),
/// 4. `clang` im PATH.
#[cfg(windows)]
pub fn finde_clang() -> Result<PathBuf, String> {
    if let Some(pfad) = clang_aus_umgebung() {
        return pfad;
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

/// Sucht `clang`. Unter Linux das vom System:
/// 1. Umgebungsvariable `OIDA_CLANG`,
/// 2. `clang` im PATH,
/// 3. das neueste `clang-<nummer>` im PATH (manche Systeme haben nur so eines).
#[cfg(not(windows))]
pub fn finde_clang() -> Result<PathBuf, String> {
    if let Some(pfad) = clang_aus_umgebung() {
        return pfad;
    }
    if let Some(pfad) = im_pfad("clang") {
        return Ok(pfad);
    }
    let mut nummeriert: Vec<(u32, PathBuf)> = Vec::new();
    for ordner in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        for eintrag in fs::read_dir(ordner).into_iter().flatten().flatten() {
            let name = eintrag.file_name().to_string_lossy().into_owned();
            if let Some(nummer) = name.strip_prefix("clang-").and_then(|n| n.parse::<u32>().ok()) {
                nummeriert.push((nummer, eintrag.path()));
            }
        }
    }
    nummeriert.sort();
    if let Some((_, pfad)) = nummeriert.pop() {
        return Ok(pfad);
    }
    Err("I find ka clang. Installier's mit da Paketverwoitung, zum Beispü: sudo apt install clang \
         (Ubuntu, Debian), sudo dnf install clang (Fedora), sudo pacman -S clang (Arch). \
         Oda setz OIDA_CLANG auf des Programm."
        .to_string())
}

/// `OIDA_CLANG` geht vor allem anderen; zeigt es ins Leere, ist das ein Fehler.
fn clang_aus_umgebung() -> Option<Result<PathBuf, String>> {
    let pfad = PathBuf::from(env::var_os("OIDA_CLANG").filter(|w| !w.is_empty())?);
    Some(if pfad.is_file() { Ok(pfad) } else { Err(format!("OIDA_CLANG zagt auf {}, oba do is nix.", pfad.display())) })
}

fn im_pfad(name: &str) -> Option<PathBuf> {
    let pfad = env::var_os("PATH")?;
    env::split_paths(&pfad).map(|ordner| ordner.join(name)).find(|datei| datei.is_file())
}

/// Wo der Compiler seine eigenen Dateien ablegt (die Laufzeitbibliothek, das IR
/// für clang). Unter Windows hat jeder seinen eigenen Temp-Ordner. Unter Linux
/// teilen sich alle `/tmp`; dort nimmt jeder Benutzer seinen eigenen Ordner,
/// damit keiner dem anderen im Weg steht.
fn arbeits_ablage() -> PathBuf {
    if cfg!(windows) {
        return env::temp_dir().join("oida");
    }
    let gesetzt = |name: &str| env::var_os(name).filter(|w| !w.is_empty()).map(PathBuf::from);
    match gesetzt("XDG_CACHE_HOME").or_else(|| gesetzt("HOME").map(|daheim| daheim.join(".cache"))) {
        Some(ablage) => ablage.join("oida"),
        None => env::temp_dir().join(format!("oida-{}", env::var("USER").unwrap_or_default())),
    }
}

/// Legt die eingebaute Laufzeitbibliothek als Datei ab (einmal pro Version)
/// und gibt zurück, wo sie liegt.
fn laufzeit_datei() -> Result<PathBuf, String> {
    let ordner = arbeits_ablage();
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
    // Sonst scheitert erst der Linker, und das sähe aus wie ein Fehler im Compiler.
    if let Some(ordner) = ausgabe.parent().filter(|o| !o.as_os_str().is_empty() && !o.is_dir()) {
        return Err(format!("Den Ordner {} gibt's ned: Durt kaun i des Programm ned hinlegn.", ordner.display()));
    }
    let clang = finde_clang()?;
    let laufzeit = laufzeit_datei()?;
    let ir_datei = arbeits_ablage().join(format!("oida-{}.ll", eindeutig()));
    fs::write(&ir_datei, ir).map_err(|e| format!("I kaun {} ned schreibn: {e}", ir_datei.display()))?;

    let ergebnis = Command::new(&clang)
        .arg(format!("--target={}", irgen::ZIEL))
        .arg(if optimieren { "-O2" } else { "-O0" })
        .arg(&ir_datei)
        .arg(&laufzeit)
        .arg("-o")
        .arg(ausgabe)
        // Unter Windows alles in die .exe hinein, damit sie ohne zusätzliche DLLs läuft.
        // Unter Linux bleibt die C-Bibliothek des Systems draußen, wie bei jedem anderen Programm.
        .args(if cfg!(windows) { &["-static"][..] } else { &[][..] })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn die_bibliothek_laesst_sich_ganz_uebersetzen() {
        // Jeder Teil für sich und alle zusammen: keine Fehler, keine doppelten Namen.
        let wb = Woerterbuch::eingebaut();
        let mut alle = String::new();
        for (teil, _) in BIBLIOTHEK {
            if !cfg!(windows) && NUR_WINDOWS.contains(&teil) {
                // Fenster und Ton gibt es nur unter Windows; anderswo sagt das eine Meldung.
                let fehler = zu_ir(&format!("BRAUCH {teil} OIDA\n"), &wb).expect_err("geht hier noch nicht");
                assert!(fehler[0].meldung.contains("gibt's derweil nur unter Windows"), "{fehler:?}");
                continue;
            }
            let quelltext = format!("BRAUCH {teil} OIDA\n");
            zu_ir(&quelltext, &wb).unwrap_or_else(|f| panic!("{teil}: {f:?}"));
            alle.push_str(&quelltext);
        }
        zu_ir(&alle, &wb).unwrap_or_else(|f| panic!("alle zusammen: {f:?}"));
    }

    #[test]
    fn die_bibliothek_geht_auch_mit_eigenem_woerterbuch() {
        // Wer WENN zu FOIS macht, kann die Bibliothek trotzdem verwenden: Sie
        // wird mit den Wörtern gelesen, mit denen sie geschrieben ist.
        let eigenes = crate::woerterbuch::EINGEBAUT.replace("IF        = WENN, WAUN", "IF        = FOIS");
        let wb = Woerterbuch::laden(&eigenes, "test").unwrap();
        let ir = zu_ir("BRAUCH listen OIDA\nFOIS summe([1, 2]) > 2 DAUN\n SCHREIB(1) OIDA\nPASST SCHO", &wb);
        assert!(ir.is_ok(), "{:?}", ir.err());
        assert!(zu_ir("WENN WOAHR DAUN\nPASST SCHO", &wb).is_err());
    }

    #[test]
    fn jede_datei_zaehlt_nur_einmal() {
        let wb = Woerterbuch::eingebaut();
        let (quellen, programm) = lade(None, "BRAUCH listen OIDA\nBRAUCH texte OIDA\nBRAUCH listen OIDA", &wb);
        assert!(programm.is_ok());
        let namen: Vec<&str> = quellen.iter().map(|q| q.name.as_str()).collect();
        assert_eq!(namen, vec!["<eingabe>", "bibliothek/listen.oida", "bibliothek/texte.oida"]);
        // Die Stellen aus der Bibliothek tragen die Nummer ihrer Datei.
        let programm = programm.unwrap();
        assert!(programm.funktionen.iter().any(|f| f.name == "summe" && f.pos.datei == 1));
        assert!(programm.funktionen.iter().any(|f| f.name == "verbind" && f.pos.datei == 2));
    }
}
