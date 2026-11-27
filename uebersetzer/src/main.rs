//! Der Befehl `oida`: übersetzt eine `.oida`-Datei in ein fertiges Programm
//! (unter Windows eine `.exe`).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use oida_uebersetzer::{editor, treiber};
use oida_uebersetzer::woerterbuch::{Schluessel, Woerterbuch};

const HILFE: &str = "\
OIDA - de Programmiersproch

Aufruf:
  oida <datei.oida> [Optionen]

Optionen:
  -o <datei>              so soll des Programm haaßn (sunst wia de .oida-Datei,
                          unta Windows mit .exe hintn)
  --renn                  des Programm noch'm Übersetzn glei startn
  --ir                    des LLVM-IR ois .ll-Datei danebn speichern
  --ohne-optimierung      schnölla übersetzn, dafia a langsamers Programm
  --mit-zoinprüfung       beim Rechnen prüfn, ob a gaunze Zoi z'groß wird (mehr ois 64 Bit),
                          und daun mit ana Mödung aufhean; kost beim Rechnen Zeit
  --woerterbuch <datei>   des Wörterbuch nehma
  --woerter               de Wörter von da Sproch auzagn
  --hilfe                 des do auzagn
  --version               de Version auzagn

Fia Editoren (so redt de VS-Code-Erweiterung mit'm Compiler):
  --editor [datei]        Programm von da Eingabe lesn, Fehler und Wortoatn ois JSON ausgebn
                          (de Datei sogt nur, wo des Programm liegt, fia BRAUCH)
  --editor-dateien <ordner>
                          Grammatik, Einrückregln und Schnipsel fia VS Code erzeugn
";

struct Optionen {
    datei: Option<PathBuf>,
    ausgabe: Option<PathBuf>,
    renn: bool,
    ir: bool,
    optimieren: bool,
    zoin_pruefen: bool,
    woerterbuch: Option<PathBuf>,
    woerter: bool,
    editor: bool,
    editor_dateien: Option<PathBuf>,
}

fn optionen_lesen(argumente: &[String]) -> Result<Optionen, String> {
    let mut o = Optionen {
        datei: None,
        ausgabe: None,
        renn: false,
        ir: false,
        optimieren: true,
        zoin_pruefen: false,
        woerterbuch: None,
        woerter: false,
        editor: false,
        editor_dateien: None,
    };
    let mut i = 0;
    while i < argumente.len() {
        let argument = argumente[i].as_str();
        let mut wert = |wofuer: &str| -> Result<PathBuf, String> {
            i += 1;
            argumente.get(i).map(PathBuf::from).ok_or(format!("Noch {wofuer} fehlt de Datei."))
        };
        match argument {
            "-o" => o.ausgabe = Some(wert("-o")?),
            "--woerterbuch" => o.woerterbuch = Some(wert("--woerterbuch")?),
            "--renn" => o.renn = true,
            "--ir" => o.ir = true,
            "--ohne-optimierung" => o.optimieren = false,
            "--mit-zoinprüfung" | "--mit-zoinpruefung" => o.zoin_pruefen = true,
            "--woerter" => o.woerter = true,
            "--editor" => o.editor = true,
            "--editor-dateien" => o.editor_dateien = Some(wert("--editor-dateien")?),
            _ if argument.starts_with('-') => {
                return Err(format!("De Option {argument} kenn i ned. Schau mit --hilfe noch."));
            }
            _ if o.datei.is_some() => {
                return Err("Ane noch da aundan: i übersetz nur a Datei auf amoi.".to_string());
            }
            _ => o.datei = Some(PathBuf::from(argument)),
        }
        i += 1;
    }
    Ok(o)
}

fn woerter_anzeigen(wb: &Woerterbuch) {
    println!("Wörterbuch: {}", wb.herkunft);
    println!();
    for (schluessel, fest) in Schluessel::ALLE {
        println!("  {:<10} {}", fest, wb.varianten(schluessel).join(", "));
    }
}

fn hauptprogramm() -> Result<ExitCode, String> {
    let argumente: Vec<String> = std::env::args().skip(1).collect();
    if argumente.is_empty() || argumente.iter().any(|a| a == "--hilfe" || a == "-h") {
        print!("{HILFE}");
        return Ok(ExitCode::SUCCESS);
    }
    if argumente.iter().any(|a| a == "--version") {
        println!("oida {}", env!("CARGO_PKG_VERSION"));
        return Ok(ExitCode::SUCCESS);
    }
    let optionen = optionen_lesen(&argumente)?;
    let wb = treiber::lade_woerterbuch(optionen.woerterbuch.as_deref())?;
    if optionen.woerter {
        woerter_anzeigen(&wb);
        return Ok(ExitCode::SUCCESS);
    }
    if optionen.editor {
        let mut bytes = Vec::new();
        std::io::stdin().read_to_end(&mut bytes).map_err(|e| format!("I kaun de Eingabe ned lesn: {e}"))?;
        let quelltext = String::from_utf8(bytes).map_err(|_| "De Eingabe is ka UTF-8.".to_string())?;
        // Steht eine Datei dabei, ist das die, aus der die Eingabe stammt:
        // Von dort aus werden die Dateien gesucht, die sie mit BRAUCH dazuholt.
        println!("{}", editor::untersuche_datei(&quelltext, &wb, optionen.datei.as_deref()));
        return Ok(ExitCode::SUCCESS);
    }
    if let Some(ordner) = optionen.editor_dateien {
        for (name, inhalt) in editor::dateien(&wb) {
            let datei = ordner.join(name);
            let fehler = |e: std::io::Error| format!("I kaun {} ned schreibn: {e}", datei.display());
            std::fs::create_dir_all(datei.parent().unwrap_or(&ordner)).map_err(fehler)?;
            std::fs::write(&datei, inhalt).map_err(fehler)?;
            println!("{}", datei.display());
        }
        return Ok(ExitCode::SUCCESS);
    }

    let Some(datei) = optionen.datei else {
        return Err("Wöche Datei soll i übersetzn? Aufruf: oida <datei.oida>".to_string());
    };
    let name = datei.display().to_string();
    let bytes = std::fs::read(&datei).map_err(|e| format!("I kaun {name} ned lesn: {e}"))?;
    let quelltext = String::from_utf8(bytes)
        .map_err(|_| format!("{name} is ka UTF-8. Speicher de Datei im Editor ois UTF-8 o."))?;

    let (quellen, ergebnis) = treiber::uebersetze(Some(&datei), &quelltext, &wb);
    let ir = match ergebnis {
        Ok(ir) => ir,
        Err(fehler) => {
            for f in &fehler {
                // Der Fehler kann in einer Datei stecken, die mit BRAUCH dazugekommen ist.
                let quelle = &quellen[f.pos.datei as usize];
                eprintln!("{}", f.anzeigen(&quelle.name, &quelle.text));
            }
            eprintln!("Na, des wird nix: {} Fehler.", fehler.len());
            return Ok(ExitCode::from(1));
        }
    };
    let ir = if optionen.zoin_pruefen { ir } else { oida_uebersetzer::irgen::ohne_ueberlaufpruefung(&ir) };

    if optionen.ir {
        let ir_datei = datei.with_extension("ll");
        std::fs::write(&ir_datei, &ir).map_err(|e| format!("I kaun {} ned schreibn: {e}", ir_datei.display()))?;
    }
    // Ohne -o heißt das Programm wie die Datei: unter Windows mit .exe, sonst ohne Endung.
    let ausgabe = optionen.ausgabe.unwrap_or_else(|| datei.with_extension(std::env::consts::EXE_EXTENSION));
    if ausgabe == datei {
        return Err(format!("Des Programm tät genauso haaßn wia de Datei {name}. Gib mit -o an aundan Nom au."));
    }
    treiber::baue_exe(&ir, &ausgabe, optionen.optimieren)?;

    if optionen.renn {
        return starten(&ausgabe);
    }
    println!("Passt! {} is fertig.", ausgabe.display());
    Ok(ExitCode::SUCCESS)
}

fn starten(programm: &Path) -> Result<ExitCode, String> {
    // Ohne Ordner davor würde Windows im PATH suchen statt hier.
    let pfad = if programm.components().count() == 1 { Path::new(".").join(programm) } else { programm.to_path_buf() };
    let status =
        Command::new(&pfad).status().map_err(|e| format!("I kaun {} ned startn: {e}", pfad.display()))?;
    match status.code() {
        Some(code) if (0..=255).contains(&code) => Ok(ExitCode::from(code as u8)),
        // Windows meldet einen Absturz (etwa einen Zugriff auf falschen Speicher) mit einer riesigen Zahl.
        Some(code) => Err(format!("Des Programm is ogstürzt (Windows sogt: 0x{:08X}).", code as u32)),
        // Linux beendet es mit einem Signal (11: Zugriff auf falschen Speicher).
        None => Err(match signal(&status) {
            Some(nummer) => format!("Des Programm is ogstürzt (Signal {nummer})."),
            None => "Des Programm is ogstürzt.".to_string(),
        }),
    }
}

#[cfg(unix)]
fn signal(status: &std::process::ExitStatus) -> Option<i32> {
    std::os::unix::process::ExitStatusExt::signal(status)
}

#[cfg(not(unix))]
fn signal(_: &std::process::ExitStatus) -> Option<i32> {
    None
}

fn main() -> ExitCode {
    match hauptprogramm() {
        Ok(code) => code,
        Err(meldung) => {
            eprintln!("Heast! {meldung}");
            ExitCode::from(2)
        }
    }
}
