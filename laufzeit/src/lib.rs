//! Laufzeitbibliothek von OIDA.
//!
//! Wird als statische Bibliothek gebaut und in jedes OIDA-Programm gelinkt.
//! Der Compiler ruft diese Funktionen aus dem erzeugten LLVM-IR auf; die
//! Namen und Parameter müssen daher zu `uebersetzer/src/irgen.rs` passen.
//!
//! Ein Text ist ein Zeiger auf einen [`TextKopf`], direkt dahinter stehen die
//! Bytes (UTF-8). Der Zähler ist für die Referenzzählung vorgesehen:
//! -1 heißt "fest im Programm eingebaut, nie freigeben". In v0.1 wird noch
//! nichts freigegeben.

use std::alloc::{Layout, alloc, handle_alloc_error};
use std::cmp::Ordering;
use std::io::{BufWriter, IsTerminal, Stdout, Write};
use std::sync::{Mutex, OnceLock};

/// Kopf eines Textes. Direkt dahinter stehen `laenge` Bytes.
#[repr(C)]
pub struct TextKopf {
    zaehler: i64,
    laenge: i64,
}

const KOPF_GROESSE: usize = std::mem::size_of::<TextKopf>();

/// Die Bytes eines Textes.
///
/// # Safety
/// `text` muss auf einen gültigen Text zeigen (vom Compiler oder von
/// [`neuer_text`] erzeugt).
unsafe fn bytes<'a>(text: *const TextKopf) -> &'a [u8] {
    unsafe {
        let laenge = (*text).laenge as usize;
        std::slice::from_raw_parts((text as *const u8).add(KOPF_GROESSE), laenge)
    }
}

/// Legt einen neuen Text am Heap an.
fn neuer_text(inhalt: &[u8]) -> *mut TextKopf {
    let layout = Layout::from_size_align(KOPF_GROESSE + inhalt.len(), std::mem::align_of::<TextKopf>())
        .expect("Text zu groß");
    unsafe {
        let zeiger = alloc(layout);
        if zeiger.is_null() {
            handle_alloc_error(layout);
        }
        let kopf = zeiger as *mut TextKopf;
        (*kopf).zaehler = 1;
        (*kopf).laenge = inhalt.len() as i64;
        std::ptr::copy_nonoverlapping(inhalt.as_ptr(), zeiger.add(KOPF_GROESSE), inhalt.len());
        kopf
    }
}

// ------------------------------------------------------------------ Ausgabe

struct Ausgabe {
    puffer: BufWriter<Stdout>,
    /// Im Terminal wird nach jeder Zeile ausgegeben, in eine Datei oder
    /// Pipe erst, wenn der Puffer voll ist (das ist viel schneller).
    terminal: bool,
}

fn ausgabe() -> &'static Mutex<Ausgabe> {
    static AUSGABE: OnceLock<Mutex<Ausgabe>> = OnceLock::new();
    AUSGABE.get_or_init(|| {
        let stdout = std::io::stdout();
        let terminal = stdout.is_terminal();
        Mutex::new(Ausgabe { puffer: BufWriter::with_capacity(64 * 1024, stdout), terminal })
    })
}

fn schreib(inhalt: &[u8]) {
    if let Ok(mut a) = ausgabe().lock() {
        let _ = a.puffer.write_all(inhalt);
    }
}

fn leeren() {
    if let Ok(mut a) = ausgabe().lock() {
        let _ = a.puffer.flush();
    }
}

/// Bricht das Programm mit einer Meldung ab.
fn abbruch(zeile: i64, meldung: &str) -> ! {
    leeren();
    eprintln!("Heast! Zeile {zeile}: {meldung}");
    std::process::exit(1);
}

/// Wird am Anfang von jedem Programm aufgerufen.
#[unsafe(no_mangle)]
pub extern "C" fn oida_start() {
    let _ = ausgabe();
}

/// Wird am Ende von jedem Programm aufgerufen.
#[unsafe(no_mangle)]
pub extern "C" fn oida_ende() {
    leeren();
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_schreib_zoi(wert: i64) {
    schreib(wert.to_string().as_bytes());
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_schreib_kommazoi(wert: f64) {
    schreib(kommazoi_als_text(wert).as_bytes());
}

/// # Safety
/// `text` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_schreib_text(text: *const TextKopf) {
    schreib(unsafe { bytes(text) });
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_schreib_leer() {
    schreib(b" ");
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_schreib_zeilenende() {
    if let Ok(mut a) = ausgabe().lock() {
        let _ = a.puffer.write_all(b"\n");
        if a.terminal {
            let _ = a.puffer.flush();
        }
    }
}

// -------------------------------------------------------------------- Texte

/// Eine Kommazahl sieht immer wie eine aus: 3.0 und nicht 3.
fn kommazoi_als_text(wert: f64) -> String {
    format!("{wert:?}")
}

/// # Safety
/// Beide Zeiger müssen gültige Texte sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_text_verbinden(a: *const TextKopf, b: *const TextKopf) -> *mut TextKopf {
    let (a, b) = unsafe { (bytes(a), bytes(b)) };
    let mut beide = Vec::with_capacity(a.len() + b.len());
    beide.extend_from_slice(a);
    beide.extend_from_slice(b);
    neuer_text(&beide)
}

/// Vergleicht zwei Texte: -1 (a kommt vorher), 0 (gleich) oder 1.
///
/// # Safety
/// Beide Zeiger müssen gültige Texte sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_text_vergleich(a: *const TextKopf, b: *const TextKopf) -> i32 {
    let (a, b) = unsafe { (bytes(a), bytes(b)) };
    match a.cmp(b) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_zoi_zu_text(wert: i64) -> *mut TextKopf {
    neuer_text(wert.to_string().as_bytes())
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_kommazoi_zu_text(wert: f64) -> *mut TextKopf {
    neuer_text(kommazoi_als_text(wert).as_bytes())
}

fn text_zu_zoi(inhalt: &[u8]) -> Option<i64> {
    std::str::from_utf8(inhalt).ok()?.trim().parse().ok()
}

fn text_zu_kommazoi(inhalt: &[u8]) -> Option<f64> {
    std::str::from_utf8(inhalt).ok()?.trim().parse().ok()
}

/// # Safety
/// `text` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_text_zu_zoi(text: *const TextKopf, zeile: i64) -> i64 {
    let inhalt = unsafe { bytes(text) };
    match text_zu_zoi(inhalt) {
        Some(wert) => wert,
        None => abbruch(
            zeile,
            &format!("Aus \"{}\" kaun i ka gaunze Zoi mochn.", String::from_utf8_lossy(inhalt)),
        ),
    }
}

/// # Safety
/// `text` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_text_zu_kommazoi(text: *const TextKopf, zeile: i64) -> f64 {
    let inhalt = unsafe { bytes(text) };
    match text_zu_kommazoi(inhalt) {
        Some(wert) => wert,
        None => abbruch(
            zeile,
            &format!("Aus \"{}\" kaun i ka Kommazoi mochn.", String::from_utf8_lossy(inhalt)),
        ),
    }
}

// ------------------------------------------------------------------- Fehler

#[unsafe(no_mangle)]
pub extern "C" fn oida_fehler_durch_null(zeile: i64) -> ! {
    abbruch(zeile, "Durch nix kaun ma ned teilen.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inhalt(text: *const TextKopf) -> String {
        String::from_utf8(unsafe { bytes(text) }.to_vec()).unwrap()
    }

    #[test]
    fn neuer_text_merkt_sich_den_inhalt() {
        let t = neuer_text("Grüß di".as_bytes());
        assert_eq!(inhalt(t), "Grüß di");
        assert_eq!(unsafe { (*t).laenge }, "Grüß di".len() as i64);
    }

    #[test]
    fn leerer_text() {
        assert_eq!(inhalt(neuer_text(b"")), "");
    }

    #[test]
    fn verbinden() {
        let a = neuer_text(b"Servas ");
        let b = neuer_text(b"Oida");
        assert_eq!(inhalt(unsafe { oida_text_verbinden(a, b) }), "Servas Oida");
    }

    #[test]
    fn vergleich() {
        let a = neuer_text(b"Apfel");
        let b = neuer_text(b"Birn");
        unsafe {
            assert_eq!(oida_text_vergleich(a, b), -1);
            assert_eq!(oida_text_vergleich(b, a), 1);
            assert_eq!(oida_text_vergleich(a, a), 0);
        }
    }

    #[test]
    fn zahlen_zu_text() {
        assert_eq!(inhalt(oida_zoi_zu_text(-42)), "-42");
        assert_eq!(inhalt(oida_kommazoi_zu_text(3.0)), "3.0");
        assert_eq!(inhalt(oida_kommazoi_zu_text(2.5)), "2.5");
    }

    #[test]
    fn text_zu_zahlen() {
        assert_eq!(text_zu_zoi(b" 17 "), Some(17));
        assert_eq!(text_zu_zoi(b"siebzehn"), None);
        assert_eq!(text_zu_kommazoi(b"1.5"), Some(1.5));
        assert_eq!(text_zu_kommazoi(b""), None);
    }
}
