//! Der Befehl `oida`: übersetzt eine `.oida`-Datei in eine `.exe`.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use oida_uebersetzer::treiber;
use oida_uebersetzer::woerterbuch::{Schluessel, Woerterbuch};

const HILFE: &str = "\
OIDA - de Programmiersproch

Aufruf:
  oida <datei.oida> [Optionen]

Optionen:
  -o <datei.exe>          so soll des Programm haaßn (sunst wia de .oida-Datei)
  --renn                  des Programm noch'm Übersetzn glei startn
  --ir                    des LLVM-IR ois .ll-Datei danebn speichern
  --ohne-optimierung      schnölla übersetzn, dafia a langsamers Programm
  --woerterbuch <datei>   des Wörterbuch nehma
  --woerter               de Wörter von da Sproch auzagn
  --hilfe                 des do auzagn
  --version               de Version auzagn
";

struct Optionen {
    datei: Option<PathBuf>,
    ausgabe: Option<PathBuf>,
    renn: bool,
    ir: bool,
    optimieren: bool,
    woerterbuch: Option<PathBuf>,
    woerter: bool,
}

fn optionen_lesen(argumente: &[String]) -> Result<Optionen, String> {
    let mut o = Optionen {
        datei: None,
        ausgabe: None,
        renn: false,
        ir: false,
        optimieren: true,
        woerterbuch: None,
        woerter: false,
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
            "--woerter" => o.woerter = true,
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

    let Some(datei) = optionen.datei else {
        return Err("Wöche Datei soll i übersetzn? Aufruf: oida <datei.oida>".to_string());
    };
    let name = datei.display().to_string();
    let bytes = std::fs::read(&datei).map_err(|e| format!("I kaun {name} ned lesn: {e}"))?;
    let quelltext = String::from_utf8(bytes)
        .map_err(|_| format!("{name} is ka UTF-8. Speicher de Datei im Editor ois UTF-8 o."))?;

    let ir = match treiber::zu_ir(&quelltext, &wb) {
        Ok(ir) => ir,
        Err(fehler) => {
            for f in &fehler {
                eprintln!("{}", f.anzeigen(&name, &quelltext));
            }
            eprintln!("Na, des wird nix: {} Fehler.", fehler.len());
            return Ok(ExitCode::from(1));
        }
    };

    if optionen.ir {
        let ir_datei = datei.with_extension("ll");
        std::fs::write(&ir_datei, &ir).map_err(|e| format!("I kaun {} ned schreibn: {e}", ir_datei.display()))?;
    }
    let ausgabe = optionen.ausgabe.unwrap_or_else(|| datei.with_extension("exe"));
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
    Ok(ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8))
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
