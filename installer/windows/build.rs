//! Packt alles, was der Installer installieren soll, in eine Datei, die dann
//! mit `include_bytes!` im Installer steckt.
//!
//! Was hineinkommt, liegt im Ordner aus der Umgebungsvariable
//! `OIDA_SETUP_INHALT` (den stellt `installer/bauen.ps1` zusammen). Ist sie
//! nicht gesetzt (etwa bei `cargo test`), ist das Paket leer.

use std::path::{Path, PathBuf};
use std::{env, fs};

#[path = "src/paket.rs"]
#[allow(dead_code)]
mod paket;

fn sammle(ordner: &Path, wurzel: &Path, eintraege: &mut Vec<paket::Eintrag>) {
    let mut pfade: Vec<PathBuf> = fs::read_dir(ordner)
        .unwrap_or_else(|e| panic!("{}: {e}", ordner.display()))
        .map(|eintrag| eintrag.unwrap().path())
        .collect();
    pfade.sort();
    for pfad in pfade {
        println!("cargo:rerun-if-changed={}", pfad.display());
        if pfad.is_dir() {
            sammle(&pfad, wurzel, eintraege);
        } else {
            let name = pfad.strip_prefix(wurzel).unwrap().to_string_lossy().replace('\\', "/");
            let inhalt = fs::read(&pfad).unwrap_or_else(|e| panic!("{}: {e}", pfad.display()));
            eintraege.push(paket::Eintrag { name, inhalt });
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/paket.rs");
    println!("cargo:rerun-if-env-changed=OIDA_SETUP_INHALT");
    let mut eintraege = Vec::new();
    if let Some(inhalt) = env::var_os("OIDA_SETUP_INHALT").filter(|w| !w.is_empty()) {
        let wurzel = PathBuf::from(inhalt);
        sammle(&wurzel, &wurzel, &mut eintraege);
    }
    let ziel = PathBuf::from(env::var("OUT_DIR").unwrap()).join("inhalt.bin");
    fs::write(&ziel, paket::packen(&paket::zusammen(&eintraege))).unwrap();
}
