//! Dateien, Ordner und Pfade (für `BRAUCH dateien OIDA`).
//!
//! Die Funktionen hier geben nur "geklappt" oder "nicht geklappt" zurück.
//! Warum es nicht geklappt hat, holt sich der OIDA-Teil der Bibliothek
//! danach mit [`oida_datei_fehler`] und macht einen `FEHLA` daraus.

use std::cell::RefCell;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, ErrorKind, Write};
use std::path::Path;

use super::{ListenKopf, TextKopf, bytes, liste_aus_zahlen, neuer_text};

thread_local! {
    /// Was beim letzten Versuch schiefgegangen ist.
    static LETZTER_FEHLER: RefCell<String> = const { RefCell::new(String::new()) };
}

pub(super) fn merke_fehler(fehler: &std::io::Error) {
    // Die Texte von Windows sind je nach Sprache anders; die häufigen Fälle sagt OIDA selbst.
    let meldung = match fehler.kind() {
        ErrorKind::NotFound => "De gibt's ned.".to_string(),
        ErrorKind::PermissionDenied => "Do derf i ned hi.".to_string(),
        ErrorKind::InvalidData => "Des is ka Text (ka UTF-8).".to_string(),
        ErrorKind::AlreadyExists => "De gibt's scho.".to_string(),
        ErrorKind::DirectoryNotEmpty => "Da Ordna is ned laar.".to_string(),
        ErrorKind::NotADirectory => "Des is ka Ordna.".to_string(),
        ErrorKind::IsADirectory => "Des is a Ordna, ka Datei.".to_string(),
        _ => fehler.to_string(),
    };
    merke_meldung(&meldung);
}

pub(super) fn merke_meldung(meldung: &str) {
    LETZTER_FEHLER.with(|f| *f.borrow_mut() = meldung.to_string());
}

/// Merkt sich den Fehler, wenn es einer ist, und sagt, ob es geklappt hat.
fn geklappt(ergebnis: std::io::Result<()>) -> bool {
    match ergebnis {
        Ok(()) => true,
        Err(fehler) => {
            merke_fehler(&fehler);
            false
        }
    }
}

/// Ein Pfad, wie OIDA ihn zurückgibt: unter Windows mit "/" statt "\" (Windows
/// versteht beides). Anderswo ist "\" ein Zeichen wie jedes andere und bleibt stehen.
pub(super) fn schraeg(pfad: &Path) -> String {
    schraeg_text(&pfad.to_string_lossy())
}

fn schraeg_text(pfad: &str) -> String {
    if cfg!(windows) { pfad.replace('\\', "/") } else { pfad.to_string() }
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
        .and_then(|pfad| pfad.parent().map(schraeg))
        .unwrap_or_else(|| ".".to_string());
    neuer_text(format!("{}/", ordner.trim_end_matches('/')).as_bytes())
}

// ------------------------------------------------------------------- Ordner

/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_ordner_gibts(pfad: *const TextKopf) -> bool {
    fs::metadata(unsafe { pfad_von(pfad) }).is_ok_and(|m| m.is_dir())
}

/// Was in einem Ordner liegt: die Namen (ohne den Ordner davor), nach dem
/// Alphabet, mit einem Zeilenende dazwischen. Null, wenn es nicht geht.
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_ordner_inhalt(pfad: *const TextKopf) -> *mut TextKopf {
    let gelesen = fs::read_dir(unsafe { pfad_von(pfad) }).and_then(|eintraege| {
        let mut namen = Vec::new();
        for eintrag in eintraege {
            namen.push(eintrag?.file_name().to_string_lossy().into_owned());
        }
        Ok(namen)
    });
    match gelesen {
        Ok(mut namen) => {
            // Groß und klein stehen beieinander, wie im Explorer.
            namen.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b)));
            neuer_text(namen.join("\n").as_bytes())
        }
        Err(fehler) => {
            merke_fehler(&fehler);
            std::ptr::null_mut()
        }
    }
}

/// Legt einen Ordner an, mit allen Ordnern davor, die noch fehlen. Gibt es ihn schon, ist das kein Fehler.
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_ordner_anlegen(pfad: *const TextKopf) -> bool {
    geklappt(fs::create_dir_all(unsafe { pfad_von(pfad) }))
}

/// Löscht einen Ordner: nur wenn er leer ist, oder mit `alles` samt Inhalt.
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_ordner_loeschen(pfad: *const TextKopf, alles: bool) -> bool {
    let pfad = unsafe { pfad_von(pfad) };
    geklappt(if alles { fs::remove_dir_all(&pfad) } else { fs::remove_dir(&pfad) })
}

// ------------------------------------------------------- mehr über Dateien

/// Wie groß eine Datei ist, in Bytes; -1, wenn es nicht geht.
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_datei_groesse(pfad: *const TextKopf) -> i64 {
    match fs::metadata(unsafe { pfad_von(pfad) }) {
        Ok(angaben) if angaben.is_dir() => {
            merke_meldung("Des is a Ordna, ka Datei.");
            -1
        }
        Ok(angaben) => angaben.len().min(i64::MAX as u64) as i64,
        Err(fehler) => {
            merke_fehler(&fehler);
            -1
        }
    }
}

/// Wann eine Datei (oder ein Ordner) zuletzt geändert wurde: Sekunden seit
/// dem 1. Jänner 1970 (Weltzeit); -1, wenn es nicht geht.
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_datei_geaendert(pfad: *const TextKopf) -> i64 {
    match fs::metadata(unsafe { pfad_von(pfad) }).and_then(|angaben| angaben.modified()) {
        Ok(zeit) => zeit.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64),
        Err(fehler) => {
            merke_fehler(&fehler);
            -1
        }
    }
}

/// Kopiert eine Datei. Gibt es das Ziel schon, wird es ersetzt.
///
/// # Safety
/// `von` und `nach` müssen gültige Texte sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_datei_kopieren(von: *const TextKopf, nach: *const TextKopf) -> bool {
    let (von, nach) = unsafe { (pfad_von(von), pfad_von(nach)) };
    geklappt(fs::copy(von, nach).map(|_| ()))
}

/// Verschiebt eine Datei oder einen Ordner oder gibt ihnen einen neuen Namen.
///
/// # Safety
/// `von` und `nach` müssen gültige Texte sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_datei_verschieben(von: *const TextKopf, nach: *const TextKopf) -> bool {
    let (von, nach) = unsafe { (pfad_von(von), pfad_von(nach)) };
    geklappt(fs::rename(von, nach))
}

// -------------------------------------------------------------------- Pfade

/// `pfad_zamm(ordna, nam)`: hängt zwei Stücke mit genau einem "/" zusammen.
/// Ist das zweite schon ein ganzer Pfad (`C:/...` oder `/...`), gilt es allein.
///
/// # Safety
/// `ordner` und `name` müssen gültige Texte sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_pfad_zusammen(ordner: *const TextKopf, name: *const TextKopf) -> *mut TextKopf {
    let (ordner, name) = unsafe { (pfad_von(ordner), pfad_von(name)) };
    let ganz = if ordner.is_empty() { name } else if name.is_empty() { ordner } else { schraeg(&Path::new(&ordner).join(&name)) };
    neuer_text(schraeg_text(&ganz).as_bytes())
}

/// Der letzte Teil eines Pfads: der Name der Datei oder des Ordners.
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_pfad_name(pfad: *const TextKopf) -> *mut TextKopf {
    let pfad = unsafe { pfad_von(pfad) };
    neuer_text(Path::new(&pfad).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().as_bytes())
}

/// Alles vor dem letzten Teil: der Ordner, in dem die Datei liegt (ohne "/" am Ende; leer, wenn keiner dasteht).
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_pfad_ordner(pfad: *const TextKopf) -> *mut TextKopf {
    let pfad = unsafe { pfad_von(pfad) };
    neuer_text(Path::new(&pfad).parent().map(schraeg).unwrap_or_default().as_bytes())
}

/// Die Endung ohne Punkt ("txt"); leer, wenn es keine gibt.
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_pfad_endung(pfad: *const TextKopf) -> *mut TextKopf {
    let pfad = unsafe { pfad_von(pfad) };
    neuer_text(Path::new(&pfad).extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default().as_bytes())
}

/// Der Pfad ohne die Endung ("a/b.txt" wird "a/b").
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_pfad_ohne_endung(pfad: *const TextKopf) -> *mut TextKopf {
    let pfad = unsafe { pfad_von(pfad) };
    neuer_text(schraeg(&Path::new(&pfad).with_extension("")).as_bytes())
}

/// Der ganze Pfad ab dem Laufwerk, auch wenn es die Datei noch nicht gibt.
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_pfad_ganz(pfad: *const TextKopf) -> *mut TextKopf {
    let pfad = unsafe { pfad_von(pfad) };
    let ganz = std::path::absolute(&pfad).map(|p| schraeg(&p)).unwrap_or(pfad);
    neuer_text(ganz.as_bytes())
}

// ------------------------------------------- Zeile für Zeile lesen und schreiben

/// Eine Datei, die gerade gelesen wird, und ob schon etwas gelesen wurde.
struct Leser {
    datei: BufReader<fs::File>,
    am_anfang: bool,
}

thread_local! {
    /// Die offenen Dateien; die Nummer, die OIDA bekommt, ist die Stelle hier.
    static LESER: RefCell<Vec<Option<Leser>>> = const { RefCell::new(Vec::new()) };
    static SCHREIBER: RefCell<Vec<Option<BufWriter<fs::File>>>> = const { RefCell::new(Vec::new()) };
}

/// Legt etwas auf den ersten freien Platz und gibt dessen Nummer zurück.
fn ablegen<T>(plaetze: &mut Vec<Option<T>>, was: T) -> i64 {
    let stelle = match plaetze.iter().position(Option::is_none) {
        Some(stelle) => stelle,
        None => {
            plaetze.push(None);
            plaetze.len() - 1
        }
    };
    plaetze[stelle] = Some(was);
    stelle as i64
}

/// Öffnet eine Datei zum Lesen, Zeile für Zeile. Gibt ihre Nummer zurück, oder -1.
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_leser_auf(pfad: *const TextKopf) -> i64 {
    match fs::File::open(unsafe { pfad_von(pfad) }) {
        Ok(datei) => LESER.with(|l| ablegen(&mut l.borrow_mut(), Leser { datei: BufReader::new(datei), am_anfang: true })),
        Err(fehler) => {
            merke_fehler(&fehler);
            -1
        }
    }
}

/// Die nächste Zeile, ohne ihr Zeilenende; null, wenn die Datei aus ist (oder zu).
#[unsafe(no_mangle)]
pub extern "C" fn oida_leser_zeile(nummer: i64) -> *mut TextKopf {
    LESER.with(|l| {
        let mut alle = l.borrow_mut();
        let Some(Some(leser)) = usize::try_from(nummer).ok().and_then(|n| alle.get_mut(n)) else {
            return std::ptr::null_mut();
        };
        let mut zeile = Vec::new();
        match leser.datei.read_until(b'\n', &mut zeile) {
            Ok(0) | Err(_) => std::ptr::null_mut(),
            Ok(_) => {
                if zeile.last() == Some(&b'\n') {
                    zeile.pop();
                    if zeile.last() == Some(&b'\r') {
                        zeile.pop();
                    }
                }
                // Ein BOM vom Windows-Editor gehört nicht zum Inhalt.
                let ab = if leser.am_anfang && zeile.starts_with(b"\xEF\xBB\xBF") { 3 } else { 0 };
                leser.am_anfang = false;
                // Was kein UTF-8 ist, wird zu Ersatzzeichen: Eine kaputte Zeile soll nicht die ganze Datei kosten.
                neuer_text(String::from_utf8_lossy(&zeile[ab..]).as_bytes())
            }
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_leser_zu(nummer: i64) {
    LESER.with(|l| {
        if let Some(platz) = usize::try_from(nummer).ok().and_then(|n| l.borrow_mut().get_mut(n).map(Option::take)) {
            drop(platz);
        }
    });
}

/// Öffnet eine Datei zum Schreiben, Stück für Stück: leer, oder mit `dazu` hinten anhängend.
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_schreiber_auf(pfad: *const TextKopf, dazu: bool) -> i64 {
    let mut wie = fs::OpenOptions::new();
    if dazu { wie.create(true).append(true) } else { wie.create(true).write(true).truncate(true) };
    match wie.open(unsafe { pfad_von(pfad) }) {
        Ok(datei) => SCHREIBER.with(|s| ablegen(&mut s.borrow_mut(), BufWriter::new(datei))),
        Err(fehler) => {
            merke_fehler(&fehler);
            -1
        }
    }
}

/// # Safety
/// `inhalt` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_schreiber_schreiben(nummer: i64, inhalt: *const TextKopf) -> bool {
    SCHREIBER.with(|s| {
        let mut alle = s.borrow_mut();
        match usize::try_from(nummer).ok().and_then(|n| alle.get_mut(n)) {
            Some(Some(datei)) => geklappt(datei.write_all(unsafe { bytes(inhalt) })),
            _ => {
                merke_meldung("De Datei is scho zua.");
                false
            }
        }
    })
}

/// Schreibt den Rest hinaus und macht die Datei zu.
#[unsafe(no_mangle)]
pub extern "C" fn oida_schreiber_zu(nummer: i64) -> bool {
    let datei = SCHREIBER.with(|s| usize::try_from(nummer).ok().and_then(|n| s.borrow_mut().get_mut(n).and_then(Option::take)));
    match datei {
        Some(mut datei) => geklappt(datei.flush()),
        None => true,
    }
}

/// Am Ende vom Programm: Was noch offen ist, wird fertig geschrieben und zugemacht.
pub(super) fn alles_zu() {
    SCHREIBER.with(|s| {
        for mut datei in s.borrow_mut().drain(..).flatten() {
            let _ = datei.flush();
        }
    });
    LESER.with(|l| l.borrow_mut().clear());
}

// -------------------------------------------------------------------- Bytes

/// Der Inhalt einer Datei als Zahlen von 0 bis 255 (für alles, was kein Text ist). Null, wenn es nicht geht.
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_bytes_lesen(pfad: *const TextKopf) -> *mut ListenKopf {
    match fs::read(unsafe { pfad_von(pfad) }) {
        Ok(inhalt) => {
            let zahlen: Vec<i64> = inhalt.iter().map(|b| *b as i64).collect();
            liste_aus_zahlen(&zahlen)
        }
        Err(fehler) => {
            merke_fehler(&fehler);
            std::ptr::null_mut()
        }
    }
}

/// Schreibt Zahlen von 0 bis 255 als Bytes in eine Datei.
///
/// # Safety
/// `pfad` muss ein gültiger Text sein, `zahlen` eine gültige Liste von ganzen Zahlen.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_bytes_schreiben(pfad: *const TextKopf, zahlen: *const ListenKopf) -> bool {
    let zahlen = unsafe {
        let l = &*zahlen;
        std::slice::from_raw_parts(l.daten as *const i64, l.laenge as usize)
    };
    if let Some(stelle) = zahlen.iter().position(|z| !(0..=255).contains(z)) {
        merke_meldung(&format!("An da Stö {stelle} steht {}, a Byte geht oba nur vo 0 bis 255.", zahlen[stelle]));
        return false;
    }
    let inhalt: Vec<u8> = zahlen.iter().map(|z| *z as u8).collect();
    geklappt(fs::write(unsafe { pfad_von(pfad) }, inhalt))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(inhalt: &str) -> *mut TextKopf {
        neuer_text(inhalt.as_bytes())
    }

    fn inhalt(text: *mut TextKopf) -> String {
        String::from_utf8(unsafe { bytes(text) }.to_vec()).unwrap()
    }

    fn fehler() -> String {
        inhalt(oida_datei_fehler())
    }

    /// Ein frischer, leerer Ordner nur für diesen Test.
    fn arbeitsordner(name: &str) -> String {
        let ordner = std::env::temp_dir().join(format!("oida_laufzeit_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&ordner);
        fs::create_dir_all(&ordner).unwrap();
        schraeg(&ordner)
    }

    #[test]
    fn pfade_zerlegen_und_zusammensetzen() {
        unsafe {
            assert_eq!(inhalt(oida_pfad_zusammen(text("a/b"), text("c.txt"))), "a/b/c.txt");
            assert_eq!(inhalt(oida_pfad_zusammen(text("a/b/"), text("c.txt"))), "a/b/c.txt");
            assert_eq!(inhalt(oida_pfad_zusammen(text(""), text("c.txt"))), "c.txt");
            assert_eq!(inhalt(oida_pfad_zusammen(text("a"), text(""))), "a");
            let ganzer = if cfg!(windows) { "C:/x/y.txt" } else { "/x/y.txt" };
            assert_eq!(inhalt(oida_pfad_zusammen(text("a/b"), text(ganzer))), ganzer);
            assert_eq!(inhalt(oida_pfad_name(text("a/b/c.tar.gz"))), "c.tar.gz");
            assert_eq!(inhalt(oida_pfad_name(text("a/b/"))), "b");
            assert_eq!(inhalt(oida_pfad_ordner(text("a/b/c.txt"))), "a/b");
            assert_eq!(inhalt(oida_pfad_ordner(text("c.txt"))), "");
            assert_eq!(inhalt(oida_pfad_endung(text("a/b/c.tar.gz"))), "gz");
            assert_eq!(inhalt(oida_pfad_endung(text("a.b/c"))), "");
            assert_eq!(inhalt(oida_pfad_endung(text(".gitignore"))), "");
            assert_eq!(inhalt(oida_pfad_ohne_endung(text("a/b.txt"))), "a/b");
            let ganz = inhalt(oida_pfad_ganz(text("x/y.txt")));
            assert!(ganz.ends_with("/x/y.txt") && Path::new(&ganz).is_absolute(), "{ganz}");
            // Unter Windows gilt auch "\" als Trenner; zurück kommt immer "/".
            #[cfg(windows)]
            {
                assert_eq!(inhalt(oida_pfad_zusammen(text("a\\b"), text("c.txt"))), "a/b/c.txt");
                assert_eq!(inhalt(oida_pfad_name(text("a\\b\\"))), "b");
                assert_eq!(inhalt(oida_pfad_ohne_endung(text("a\\b.txt"))), "a/b");
                assert!(ganz.chars().nth(1) == Some(':') && !ganz.contains('\\'), "{ganz}");
            }
        }
    }

    #[test]
    fn ordner_anlegen_auflisten_loeschen() {
        let ordner = arbeitsordner("ordner");
        unsafe {
            assert!(oida_ordner_gibts(text(&ordner)));
            assert_eq!(inhalt(oida_ordner_inhalt(text(&ordner))), "");
            assert!(oida_ordner_anlegen(text(&format!("{ordner}/tiaf/drin"))));
            assert!(oida_ordner_anlegen(text(&format!("{ordner}/tiaf/drin"))), "zweimal anlegen ist kein Fehler");
            assert!(oida_datei_schreiben(text(&format!("{ordner}/b.txt")), text("zwa"), false));
            assert!(oida_datei_schreiben(text(&format!("{ordner}/A.txt")), text("ans"), false));
            assert_eq!(inhalt(oida_ordner_inhalt(text(&ordner))), "A.txt\nb.txt\ntiaf");
            assert!(!oida_ordner_gibts(text(&format!("{ordner}/b.txt"))), "eine Datei ist kein Ordner");
            assert!(!oida_datei_gibts(text(&format!("{ordner}/tiaf"))), "ein Ordner ist keine Datei");

            // Ein Ordner mit Inhalt geht nur mit `alles` weg.
            assert!(!oida_ordner_loeschen(text(&format!("{ordner}/tiaf")), false));
            assert_eq!(fehler(), "Da Ordna is ned laar.");
            assert!(oida_ordner_loeschen(text(&format!("{ordner}/tiaf/drin")), false));
            assert!(oida_ordner_loeschen(text(&ordner), true));
            assert!(!oida_ordner_gibts(text(&ordner)));
            assert!(oida_ordner_inhalt(text(&ordner)).is_null());
            assert_eq!(fehler(), "De gibt's ned.");
        }
    }

    #[test]
    fn groesse_zeit_kopieren_verschieben() {
        let ordner = arbeitsordner("infos");
        let (a, b, c) = (format!("{ordner}/a.txt"), format!("{ordner}/b.txt"), format!("{ordner}/c.txt"));
        unsafe {
            assert!(oida_datei_schreiben(text(&a), text("Grüß di"), false));
            assert_eq!(oida_datei_groesse(text(&a)), 9, "ü und ß brauchen je zwei Bytes");
            let jetzt = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
            assert!((oida_datei_geaendert(text(&a)) - jetzt).abs() < 60);
            assert_eq!(oida_datei_groesse(text(&b)), -1);
            assert_eq!(fehler(), "De gibt's ned.");
            assert_eq!(oida_datei_groesse(text(&ordner)), -1);
            assert_eq!(fehler(), "Des is a Ordna, ka Datei.");
            assert_eq!(oida_datei_geaendert(text(&b)), -1);

            assert!(oida_datei_kopieren(text(&a), text(&b)));
            assert_eq!(inhalt(oida_datei_lesen(text(&b))), "Grüß di");
            assert!(oida_datei_verschieben(text(&b), text(&c)));
            assert!(!oida_datei_gibts(text(&b)) && oida_datei_gibts(text(&c)) && oida_datei_gibts(text(&a)));
            assert!(!oida_datei_kopieren(text(&b), text(&c)));
            assert_eq!(fehler(), "De gibt's ned.");
            assert!(oida_ordner_loeschen(text(&ordner), true));
        }
    }

    #[test]
    fn zeile_fuer_zeile() {
        let ordner = arbeitsordner("zeilen");
        let pfad = format!("{ordner}/zeilen.txt");
        unsafe {
            let s = oida_schreiber_auf(text(&pfad), false);
            assert!(s >= 0);
            assert!(oida_schreiber_schreiben(s, text("ans\r\nzwa\n")));
            assert!(oida_schreiber_schreiben(s, text("\ndrei ohne Ende")));
            assert!(oida_schreiber_zu(s));
            assert!(!oida_schreiber_schreiben(s, text("zu spät")));
            assert_eq!(fehler(), "De Datei is scho zua.");
            assert!(oida_schreiber_zu(s), "zweimal zumachen ist kein Fehler");
            // Anhängen
            let s = oida_schreiber_auf(text(&pfad), true);
            assert!(oida_schreiber_schreiben(s, text("\nvia")));
            assert!(oida_schreiber_zu(s));

            let l = oida_leser_auf(text(&pfad));
            assert!(l >= 0);
            let mut zeilen = Vec::new();
            loop {
                let zeile = oida_leser_zeile(l);
                if zeile.is_null() {
                    break;
                }
                zeilen.push(inhalt(zeile));
            }
            assert_eq!(zeilen, ["ans", "zwa", "", "drei ohne Ende", "via"]);
            assert!(oida_leser_zeile(l).is_null(), "nach dem Ende kommt nichts mehr");
            oida_leser_zu(l);
            assert!(oida_leser_zeile(l).is_null());
            oida_leser_zu(l);
            oida_leser_zu(-5);

            // Der freie Platz wird wieder vergeben; zwei Dateien zugleich haben verschiedene Nummern.
            let (x, y) = (oida_leser_auf(text(&pfad)), oida_leser_auf(text(&pfad)));
            assert!(x != y && x >= 0 && y >= 0);
            assert_eq!(inhalt(oida_leser_zeile(x)), "ans");
            assert_eq!(inhalt(oida_leser_zeile(y)), "ans");
            assert_eq!(inhalt(oida_leser_zeile(x)), "zwa");
            oida_leser_zu(x);
            oida_leser_zu(y);

            // Ein BOM am Anfang gehört nicht zur ersten Zeile.
            fs::write(&pfad, b"\xEF\xBB\xBFerste\n\xEF\xBB\xBFzweite").unwrap();
            let l = oida_leser_auf(text(&pfad));
            assert_eq!(inhalt(oida_leser_zeile(l)), "erste");
            assert_eq!(inhalt(oida_leser_zeile(l)), "\u{feff}zweite");
            oida_leser_zu(l);

            assert_eq!(oida_leser_auf(text(&format!("{ordner}/fehlt.txt"))), -1);
            assert_eq!(fehler(), "De gibt's ned.");
            assert_eq!(oida_schreiber_auf(text(&format!("{ordner}/kan/ordna/x.txt")), false), -1);
            assert!(oida_ordner_loeschen(text(&ordner), true));
        }
    }

    #[test]
    fn offene_dateien_werden_am_ende_fertig_geschrieben() {
        let ordner = arbeitsordner("ende");
        let pfad = format!("{ordner}/rest.txt");
        unsafe {
            let s = oida_schreiber_auf(text(&pfad), false);
            assert!(oida_schreiber_schreiben(s, text("no ned draussn")));
            assert_eq!(fs::read(&pfad).unwrap(), b"", "liegt noch im Zwischenspeicher");
            alles_zu();
            assert_eq!(fs::read(&pfad).unwrap(), b"no ned draussn");
            assert!(oida_ordner_loeschen(text(&ordner), true));
        }
    }

    #[test]
    fn bytes_lesen_und_schreiben() {
        let ordner = arbeitsordner("bytes");
        let pfad = format!("{ordner}/roh.bin");
        unsafe {
            let zahlen = liste_aus_zahlen(&[0, 1, 127, 128, 255, 10, 13]);
            assert!(oida_bytes_schreiben(text(&pfad), zahlen));
            assert_eq!(fs::read(&pfad).unwrap(), [0, 1, 127, 128, 255, 10, 13]);
            let gelesen = oida_bytes_lesen(text(&pfad));
            let l = &*gelesen;
            assert_eq!(std::slice::from_raw_parts(l.daten as *const i64, l.laenge as usize), [0, 1, 127, 128, 255, 10, 13]);
            // Als Text lässt sich das nicht lesen, als Bytes schon.
            assert!(oida_datei_lesen(text(&pfad)).is_null());

            assert!(!oida_bytes_schreiben(text(&pfad), liste_aus_zahlen(&[65, 256, 66])));
            assert_eq!(fehler(), "An da Stö 1 steht 256, a Byte geht oba nur vo 0 bis 255.");
            assert!(!oida_bytes_schreiben(text(&pfad), liste_aus_zahlen(&[-1])));
            assert_eq!(fs::read(&pfad).unwrap().len(), 7, "die Datei bleibt, wie sie war");
            assert!(oida_bytes_lesen(text(&format!("{ordner}/fehlt.bin"))).is_null());
            let leer = oida_bytes_lesen(text(&pfad));
            assert_eq!((*leer).laenge, 7);
            assert!(oida_bytes_schreiben(text(&pfad), liste_aus_zahlen(&[])));
            assert_eq!((*oida_bytes_lesen(text(&pfad))).laenge, 0);
            assert!(oida_ordner_loeschen(text(&ordner), true));
        }
    }
}
