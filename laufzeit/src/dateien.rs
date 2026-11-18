//! Dateien lesen und schreiben (für `BRAUCH dateien OIDA`).
//!
//! Die Funktionen hier geben nur "geklappt" oder "nicht geklappt" zurück.
//! Warum es nicht geklappt hat, holt sich der OIDA-Teil der Bibliothek
//! danach mit [`oida_datei_fehler`] und macht einen `FEHLA` daraus.

use std::cell::RefCell;
use std::fs;
use std::io::{ErrorKind, Write};

use super::{TextKopf, bytes, neuer_text};

thread_local! {
    /// Was beim letzten Versuch schiefgegangen ist.
    static LETZTER_FEHLER: RefCell<String> = const { RefCell::new(String::new()) };
}

fn merke_fehler(fehler: &std::io::Error) {
    // Die Texte von Windows sind je nach Sprache anders; die häufigen Fälle sagt OIDA selbst.
    let meldung = match fehler.kind() {
        ErrorKind::NotFound => "De gibt's ned.".to_string(),
        ErrorKind::PermissionDenied => "Do derf i ned hi.".to_string(),
        ErrorKind::InvalidData => "Des is ka Text (ka UTF-8).".to_string(),
        _ => fehler.to_string(),
    };
    LETZTER_FEHLER.with(|f| *f.borrow_mut() = meldung);
}

/// Der Pfad als Rust-Text.
///
/// # Safety
/// `text` muss ein gültiger Text sein.
unsafe fn pfad_von(text: *const TextKopf) -> String {
    String::from_utf8_lossy(unsafe { bytes(text) }).into_owned()
}

/// Liest eine ganze Datei als Text. Gibt null zurück, wenn es nicht geht.
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_datei_lesen(pfad: *const TextKopf) -> *mut TextKopf {
    let gelesen = fs::read(unsafe { pfad_von(pfad) }).and_then(|inhalt| {
        // Ein BOM vom Windows-Editor gehört nicht zum Inhalt.
        let inhalt = inhalt.strip_prefix(b"\xEF\xBB\xBF").map(<[u8]>::to_vec).unwrap_or(inhalt);
        String::from_utf8(inhalt).map_err(|_| std::io::Error::from(ErrorKind::InvalidData))
    });
    match gelesen {
        Ok(inhalt) => neuer_text(inhalt.as_bytes()),
        Err(fehler) => {
            merke_fehler(&fehler);
            std::ptr::null_mut()
        }
    }
}

/// Schreibt einen Text in eine Datei: ersetzt ihren Inhalt oder hängt hinten an (`dazu`).
///
/// # Safety
/// `pfad` und `inhalt` müssen gültige Texte sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_datei_schreiben(pfad: *const TextKopf, inhalt: *const TextKopf, dazu: bool) -> bool {
    let (pfad, inhalt) = unsafe { (pfad_von(pfad), bytes(inhalt)) };
    let geschrieben = if dazu {
        fs::OpenOptions::new().create(true).append(true).open(&pfad).and_then(|mut datei| datei.write_all(inhalt))
    } else {
        fs::write(&pfad, inhalt)
    };
    match geschrieben {
        Ok(()) => true,
        Err(fehler) => {
            merke_fehler(&fehler);
            false
        }
    }
}

/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_datei_gibts(pfad: *const TextKopf) -> bool {
    fs::metadata(unsafe { pfad_von(pfad) }).is_ok_and(|m| m.is_file())
}

/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_datei_loeschen(pfad: *const TextKopf) -> bool {
    match fs::remove_file(unsafe { pfad_von(pfad) }) {
        Ok(()) => true,
        Err(fehler) => {
            merke_fehler(&fehler);
            false
        }
    }
}

/// Warum der letzte Versuch nicht geklappt hat, als neuer Text.
#[unsafe(no_mangle)]
pub extern "C" fn oida_datei_fehler() -> *mut TextKopf {
    LETZTER_FEHLER.with(|f| neuer_text(f.borrow().as_bytes()))
}

/// `programm_ordna()`: der Ordner, in dem das Programm selbst liegt (mit "/" am Ende),
/// egal von wo es gestartet wurde. Dort liegt, was zum Programm gehört: Bilder, Musik.
#[unsafe(no_mangle)]
pub extern "C" fn oida_programm_ordner() -> *mut TextKopf {
    let ordner = std::env::current_exe()
        .ok()
        .and_then(|pfad| pfad.parent().map(|o| o.to_string_lossy().replace('\\', "/")))
        .unwrap_or_else(|| ".".to_string());
    neuer_text(format!("{}/", ordner.trim_end_matches('/')).as_bytes())
}
