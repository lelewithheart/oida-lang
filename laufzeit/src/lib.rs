//! Laufzeitbibliothek von OIDA.
//!
//! Wird als statische Bibliothek gebaut und in jedes OIDA-Programm gelinkt.
//! Der Compiler ruft diese Funktionen aus dem erzeugten LLVM-IR auf; die
//! Namen und Parameter müssen daher zu `uebersetzer/src/irgen.rs` passen.
//!
//! OIDA-Programme laufen in einem einzigen Thread. Darauf verlässt sich die
//! Laufzeit: Ausgabepuffer und Zähler sind nicht gegen gleichzeitige
//! Zugriffe geschützt, das spart bei jedem Aufruf eine Sperre.
//!
//! # Texte
//!
//! Ein Text ist ein Zeiger auf einen [`TextKopf`], direkt dahinter stehen die
//! Bytes (UTF-8). Im Kopf steht ein Zähler: wie viele Stellen im Programm den
//! Text gerade brauchen (Referenzzählung). Hochzählen und Herunterzählen macht
//! der erzeugte Code selbst; fällt der Zähler auf 0, ruft er
//! [`oida_text_frei`]. Feste Texte aus dem Programm haben den Zähler -1 und
//! werden nie gezählt und nie freigegeben.

use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::cell::UnsafeCell;
use std::cmp::Ordering;
use std::io::{IsTerminal, Write};

/// Kopf eines Textes. Direkt dahinter stehen `laenge` Bytes.
#[repr(C)]
pub struct TextKopf {
    zaehler: i64,
    laenge: i64,
}

const KOPF_GROESSE: usize = std::mem::size_of::<TextKopf>();

/// Zähler eines Textes, der im Prüfmodus "freigegeben" wurde.
/// Negativ, damit der erzeugte Code ihn wie einen festen Text in Ruhe lässt.
const TOT: i64 = -2;

/// Was die Laufzeit zwischen den Aufrufen im Kopf behält.
struct Zustand {
    /// Gesammelte Ausgabe, die noch nicht geschrieben ist.
    puffer: Vec<u8>,
    /// Im Terminal wird nach jeder Zeile geschrieben, in eine Datei oder
    /// Pipe erst, wenn der Puffer voll ist (das ist viel schneller).
    terminal: bool,
    /// Wie viele Texte gerade am Heap leben.
    lebende_texte: i64,
    /// Prüfmodus (Umgebungsvariable `OIDA_TEXTE_PRUEFEN`): Texte werden nicht
    /// wirklich freigegeben, sondern als tot markiert. Wird ein toter Text
    /// noch verwendet, bricht das Programm ab; am Ende wird gemeldet, wie
    /// viele Texte übrig sind. Damit testet der Compiler seine Referenzzählung.
    pruefen: bool,
}

/// Erlaubt eine veränderliche globale Variable ohne Sperre.
struct Einzeln(UnsafeCell<Zustand>);

// SAFETY: OIDA-Programme haben genau einen Thread (siehe oben).
unsafe impl Sync for Einzeln {}

static ZUSTAND: Einzeln = Einzeln(UnsafeCell::new(Zustand {
    puffer: Vec::new(),
    terminal: false,
    lebende_texte: 0,
    pruefen: false,
}));

const PUFFER_GROESSE: usize = 64 * 1024;

fn zustand() -> &'static mut Zustand {
    // SAFETY: nur ein Thread, und keine Funktion hier behält den Verweis
    // über einen Aufruf hinaus, der wieder `zustand()` holt.
    unsafe { &mut *ZUSTAND.0.get() }
}

// -------------------------------------------------------------------- Texte

/// Die Bytes eines Textes.
///
/// # Safety
/// `text` muss auf einen gültigen Text zeigen (vom Compiler oder von
/// [`neuer_text`] erzeugt).
unsafe fn bytes<'a>(text: *const TextKopf) -> &'a [u8] {
    unsafe {
        if (*text).zaehler == TOT {
            abbruch_compiler("a Text wird no verwendet, obwohl er scho freigebn is");
        }
        let laenge = (*text).laenge as usize;
        std::slice::from_raw_parts((text as *const u8).add(KOPF_GROESSE), laenge)
    }
}

fn layout(laenge: usize) -> Layout {
    Layout::from_size_align(KOPF_GROESSE + laenge, std::mem::align_of::<TextKopf>()).expect("Text zu groß")
}

/// Legt Platz für einen neuen Text mit `laenge` Bytes an (Zähler 1) und gibt
/// den Text und den Anfang seiner Bytes zurück. Die Bytes muss der Aufrufer füllen.
fn neuer_platz(laenge: usize) -> (*mut TextKopf, *mut u8) {
    let layout = layout(laenge);
    unsafe {
        let zeiger = alloc(layout);
        if zeiger.is_null() {
            handle_alloc_error(layout);
        }
        let kopf = zeiger as *mut TextKopf;
        (*kopf).zaehler = 1;
        (*kopf).laenge = laenge as i64;
        zustand().lebende_texte += 1;
        (kopf, zeiger.add(KOPF_GROESSE))
    }
}

/// Legt einen neuen Text am Heap an.
fn neuer_text(inhalt: &[u8]) -> *mut TextKopf {
    let (text, ziel) = neuer_platz(inhalt.len());
    unsafe { std::ptr::copy_nonoverlapping(inhalt.as_ptr(), ziel, inhalt.len()) };
    text
}

/// Gibt einen Text frei. Der erzeugte Code ruft das auf, sobald der Zähler
/// eines Textes auf 0 gefallen ist.
///
/// # Safety
/// `text` muss ein Text vom Heap sein, den niemand mehr braucht.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_text_frei(text: *mut TextKopf) {
    unsafe {
        if (*text).zaehler != 0 {
            abbruch_compiler("a Text wird freigebn, obwohl er no braucht wird");
        }
        let z = zustand();
        z.lebende_texte -= 1;
        if z.pruefen {
            (*text).zaehler = TOT;
        } else {
            dealloc(text as *mut u8, layout((*text).laenge as usize));
        }
    }
}

/// Schreibt eine ganze Zahl als Ziffern ans Ende von `puffer` und gibt den
/// beschriebenen Teil zurück. 20 Bytes reichen für jede 64-Bit-Zahl.
fn ziffern(wert: i64, puffer: &mut [u8; 20]) -> &[u8] {
    let mut rest = wert.unsigned_abs();
    let mut i = puffer.len();
    loop {
        i -= 1;
        puffer[i] = b'0' + (rest % 10) as u8;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    if wert < 0 {
        i -= 1;
        puffer[i] = b'-';
    }
    &puffer[i..]
}

/// Eine Kommazahl sieht immer wie eine aus: 3.0 und nicht 3.
fn kommazoi_als_text(wert: f64) -> String {
    format!("{wert:?}")
}

/// # Safety
/// Beide Zeiger müssen gültige Texte sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_text_verbinden(a: *const TextKopf, b: *const TextKopf) -> *mut TextKopf {
    let (a, b) = unsafe { (bytes(a), bytes(b)) };
    let (text, ziel) = neuer_platz(a.len() + b.len());
    unsafe {
        std::ptr::copy_nonoverlapping(a.as_ptr(), ziel, a.len());
        std::ptr::copy_nonoverlapping(b.as_ptr(), ziel.add(a.len()), b.len());
    }
    text
}

/// Hängt mehrere Texte in einem Schritt zusammen (für `a + b + c + ...`).
///
/// # Safety
/// `teile` muss auf `anzahl` gültige Texte zeigen.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_text_verbinden_viele(teile: *const *const TextKopf, anzahl: i64) -> *mut TextKopf {
    let teile = unsafe { std::slice::from_raw_parts(teile, anzahl as usize) };
    let laenge: usize = teile.iter().map(|t| unsafe { bytes(*t) }.len()).sum();
    let (text, mut ziel) = neuer_platz(laenge);
    for teil in teile {
        let inhalt = unsafe { bytes(*teil) };
        unsafe {
            std::ptr::copy_nonoverlapping(inhalt.as_ptr(), ziel, inhalt.len());
            ziel = ziel.add(inhalt.len());
        }
    }
    text
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
    neuer_text(ziffern(wert, &mut [0; 20]))
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

// ------------------------------------------------------------------ Ausgabe

/// Schreibt den Puffer wirklich hinaus.
fn leeren() {
    let z = zustand();
    if z.puffer.is_empty() {
        return;
    }
    let mut stdout = std::io::stdout().lock();
    // Ist die Ausgabe zu (zum Beispiel "| head"), gibt es nichts mehr zu tun.
    let _ = stdout.write_all(&z.puffer);
    let _ = stdout.flush();
    z.puffer.clear();
}

fn schreib(inhalt: &[u8]) {
    if zustand().puffer.len() + inhalt.len() > PUFFER_GROESSE {
        leeren();
    }
    zustand().puffer.extend_from_slice(inhalt);
}

/// Wird am Anfang von jedem Programm aufgerufen.
#[unsafe(no_mangle)]
pub extern "C" fn oida_start() {
    let z = zustand();
    z.puffer.reserve(PUFFER_GROESSE);
    z.terminal = std::io::stdout().is_terminal();
    z.pruefen = std::env::var_os("OIDA_TEXTE_PRUEFEN").is_some_and(|w| !w.is_empty());
}

/// Wird am Ende von jedem Programm aufgerufen.
#[unsafe(no_mangle)]
pub extern "C" fn oida_ende() {
    leeren();
    if zustand().pruefen {
        eprintln!("OIDA: {} Texte übrig", zustand().lebende_texte);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_schreib_zoi(wert: i64) {
    schreib(ziffern(wert, &mut [0; 20]));
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
    schreib(b"\n");
    if zustand().terminal {
        leeren();
    }
}

// ------------------------------------------------------------------- Fehler

/// Bricht das Programm mit einer Meldung ab.
fn abbruch(zeile: i64, meldung: &str) -> ! {
    leeren();
    eprintln!("Heast! Zeile {zeile}: {meldung}");
    std::process::exit(1);
}

/// Bricht ab, weil der Compiler falschen Code erzeugt hat (sollte nie vorkommen).
fn abbruch_compiler(meldung: &str) -> ! {
    leeren();
    eprintln!("Heast! Des is a Fehler im OIDA-Compiler, ned in deim Programm: {meldung}.");
    std::process::exit(70);
}

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
        assert_eq!(unsafe { (*t).zaehler }, 1);
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
    fn viele_verbinden() {
        let teile = [neuer_text(b"a") as *const TextKopf, neuer_text(b""), neuer_text(b"bc"), neuer_text(b"d")];
        assert_eq!(inhalt(unsafe { oida_text_verbinden_viele(teile.as_ptr(), 4) }), "abcd");
        assert_eq!(inhalt(unsafe { oida_text_verbinden_viele(teile.as_ptr(), 0) }), "");
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
        assert_eq!(inhalt(oida_zoi_zu_text(0)), "0");
        assert_eq!(inhalt(oida_zoi_zu_text(i64::MAX)), "9223372036854775807");
        assert_eq!(inhalt(oida_zoi_zu_text(i64::MIN)), "-9223372036854775808");
        assert_eq!(inhalt(oida_kommazoi_zu_text(3.0)), "3.0");
        assert_eq!(inhalt(oida_kommazoi_zu_text(2.5)), "2.5");
    }

    #[test]
    fn ziffern_wie_die_standardbibliothek() {
        for wert in [0, 1, -1, 9, 10, 99, 100, 12345, -98765, i64::MAX, i64::MIN, i64::MIN + 1] {
            assert_eq!(ziffern(wert, &mut [0; 20]), wert.to_string().as_bytes());
        }
    }

    #[test]
    fn text_zu_zahlen() {
        assert_eq!(text_zu_zoi(b" 17 "), Some(17));
        assert_eq!(text_zu_zoi(b"siebzehn"), None);
        assert_eq!(text_zu_kommazoi(b"1.5"), Some(1.5));
        assert_eq!(text_zu_kommazoi(b""), None);
    }
}
