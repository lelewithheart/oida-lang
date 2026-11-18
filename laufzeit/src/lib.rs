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
//!
//! # Listen
//!
//! Eine Liste ist ein Zeiger auf einen [`ListenKopf`]: Zähler (wie bei
//! Texten), Länge, Platz und ein Zeiger auf die Elemente. Die Elemente liegen
//! als Werte dicht hintereinander. Wie groß ein Element ist, weiß nur der
//! Compiler; er gibt die Größe bei jedem Aufruf mit. Lesen und Schreiben der
//! Elemente macht der erzeugte Code selbst.

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
    /// Wie viele Texte und Listen gerade am Heap leben.
    lebende_texte: i64,
    /// Prüfmodus (Umgebungsvariable `OIDA_TEXTE_PRUEFEN`): Texte und Listen
    /// werden nicht wirklich freigegeben, sondern als tot markiert. Wird so
    /// etwas noch verwendet, bricht das Programm ab; am Ende wird gemeldet,
    /// wie viele übrig sind. Damit testet der Compiler seine Referenzzählung.
    pruefen: bool,
    /// Stand des Zufallsgenerators. 0 heißt: noch nicht gestartet.
    zufall: u64,
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
    zufall: 0,
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
        if (*text).zaehler <= TOT {
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

/// `ZOI AUS(text)`: Schreibt die Zahl nach `ziel`. Geht es nicht, kommt der
/// Fehler als neuer Text zurück (sonst null) und in `ziel` steht 0.
///
/// # Safety
/// `text` muss ein gültiger Text sein, `ziel` Platz für eine Zahl.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_text_als_zoi(text: *const TextKopf, ziel: *mut i64) -> *mut TextKopf {
    let inhalt = unsafe { bytes(text) };
    let wert = text_zu_zoi(inhalt);
    unsafe { *ziel = wert.unwrap_or(0) };
    match wert {
        Some(_) => std::ptr::null_mut(),
        None => neuer_text(
            format!("Aus \"{}\" kaun i ka gaunze Zoi mochn.", String::from_utf8_lossy(inhalt)).as_bytes(),
        ),
    }
}

/// `KOMMAZOI AUS(text)`: wie [`oida_text_als_zoi`], für Kommazahlen.
///
/// # Safety
/// `text` muss ein gültiger Text sein, `ziel` Platz für eine Zahl.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_text_als_kommazoi(text: *const TextKopf, ziel: *mut f64) -> *mut TextKopf {
    let inhalt = unsafe { bytes(text) };
    let wert = text_zu_kommazoi(inhalt);
    unsafe { *ziel = wert.unwrap_or(0.0) };
    match wert {
        Some(_) => std::ptr::null_mut(),
        None => neuer_text(
            format!("Aus \"{}\" kaun i ka Kommazoi mochn.", String::from_utf8_lossy(inhalt)).as_bytes(),
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

/// Ein Text als `&str`. Texte in OIDA sind immer gültiges UTF-8.
///
/// # Safety
/// `text` muss ein gültiger Text sein.
unsafe fn als_str<'a>(text: *const TextKopf) -> &'a str {
    std::str::from_utf8(unsafe { bytes(text) }).expect("Texte sind immer UTF-8")
}

/// Anzahl der Zeichen (nicht der Bytes: "Grüß" hat 4).
///
/// # Safety
/// `text` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_text_laenge(text: *const TextKopf) -> i64 {
    unsafe { als_str(text) }.chars().count() as i64
}

/// Das Stück eines Textes ab dem Zeichen `von` (gezählt ab 0) mit `anzahl`
/// Zeichen. `anzahl` -1 heißt: bis zum Ende.
fn stueck(text: &str, von: i64, anzahl: i64) -> Result<&str, String> {
    let laenge = text.chars().count() as i64;
    if von < 0 || von > laenge {
        return Err(format!("Der Text hot {laenge} Zeichn, bei {von} kaun des Stickl ned aufaunga."));
    }
    let anzahl = if anzahl == -1 { laenge - von } else { anzahl };
    if anzahl < 0 {
        return Err(format!("A Stickl kaun ned {anzahl} Zeichn laung sei."));
    }
    if von + anzahl > laenge {
        return Err(format!(
            "Der Text hot {laenge} Zeichn, a Stickl ob {von} mit {anzahl} Zeichn geht drüba naus."
        ));
    }
    // Von der Zeichen-Nummer zur Stelle in den Bytes.
    let stelle = |zeichen: i64| text.char_indices().nth(zeichen as usize).map_or(text.len(), |(i, _)| i);
    Ok(&text[stelle(von)..stelle(von + anzahl)])
}

/// # Safety
/// `text` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_text_stueck(text: *const TextKopf, von: i64, anzahl: i64, zeile: i64) -> *mut TextKopf {
    match stueck(unsafe { als_str(text) }, von, anzahl) {
        Ok(teil) => neuer_text(teil.as_bytes()),
        Err(meldung) => abbruch(zeile, &meldung),
    }
}

/// Die Stelle (in Zeichen ab 0), an der `gesucht` zum ersten Mal vorkommt, oder -1.
fn suche(text: &str, gesucht: &str) -> i64 {
    match text.find(gesucht) {
        Some(stelle) => text[..stelle].chars().count() as i64,
        None => -1,
    }
}

/// # Safety
/// Beide Zeiger müssen gültige Texte sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_text_suche(text: *const TextKopf, gesucht: *const TextKopf) -> i64 {
    unsafe { suche(als_str(text), als_str(gesucht)) }
}

/// # Safety
/// `text` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_text_gross(text: *const TextKopf) -> *mut TextKopf {
    neuer_text(unsafe { als_str(text) }.to_uppercase().as_bytes())
}

/// # Safety
/// `text` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_text_klein(text: *const TextKopf) -> *mut TextKopf {
    neuer_text(unsafe { als_str(text) }.to_lowercase().as_bytes())
}

/// Ohne Leerraum am Anfang und am Ende.
///
/// # Safety
/// `text` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_text_gestutzt(text: *const TextKopf) -> *mut TextKopf {
    neuer_text(unsafe { als_str(text) }.trim().as_bytes())
}

/// # Safety
/// Alle drei Zeiger müssen gültige Texte sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_text_ersetzt(
    text: *const TextKopf,
    alt: *const TextKopf,
    neu: *const TextKopf,
    zeile: i64,
) -> *mut TextKopf {
    let (text, alt, neu) = unsafe { (als_str(text), als_str(alt), als_str(neu)) };
    if alt.is_empty() {
        abbruch(zeile, "Nix kaun ma ned ersetzn: Der Text, den i suachn soll, is laar.");
    }
    neuer_text(text.replace(alt, neu).as_bytes())
}

// ------------------------------------------------------------------- Listen

/// Kopf einer Liste. Die Reihenfolge muss zu `%oida.Liste` im erzeugten Code passen.
#[repr(C)]
pub struct ListenKopf {
    zaehler: i64,
    /// Wie viele Elemente drin sind.
    laenge: i64,
    /// Für wie viele Elemente Speicher da ist.
    platz: i64,
    daten: *mut u8,
}

/// Alle Elemente sind auf 8 Bytes ausgerichtet (mehr braucht kein Typ in OIDA).
const ELEMENT_AUSRICHTUNG: usize = 8;

fn daten_layout(platz: i64, groesse: i64) -> Layout {
    let bytes = (platz as usize).checked_mul(groesse as usize).expect("Liste zu groß");
    Layout::from_size_align(bytes, ELEMENT_AUSRICHTUNG).expect("Liste zu groß")
}

/// Speicher für `platz` Elemente. Braucht es keinen (leere Liste oder
/// Elemente ohne Größe), gibt es einen Zeiger, hinter dem nichts liegt.
fn neue_daten(platz: i64, groesse: i64) -> *mut u8 {
    let layout = daten_layout(platz, groesse);
    if layout.size() == 0 {
        return std::ptr::without_provenance_mut(ELEMENT_AUSRICHTUNG);
    }
    let daten = unsafe { alloc(layout) };
    if daten.is_null() {
        handle_alloc_error(layout);
    }
    daten
}

/// Prüft, dass die Liste noch lebt.
unsafe fn liste<'a>(liste: *mut ListenKopf) -> &'a mut ListenKopf {
    let liste = unsafe { &mut *liste };
    if liste.zaehler <= TOT {
        abbruch_compiler("a LISTN wird no verwendet, obwohl sie scho freigebn is");
    }
    liste
}

/// Eine neue, leere Liste mit Platz für `platz` Elemente zu je `groesse` Bytes.
#[unsafe(no_mangle)]
pub extern "C" fn oida_liste_neu(groesse: i64, platz: i64) -> *mut ListenKopf {
    zustand().lebende_texte += 1;
    Box::into_raw(Box::new(ListenKopf { zaehler: 1, laenge: 0, platz, daten: neue_daten(platz, groesse) }))
}

/// Macht hinten Platz für ein weiteres Element und gibt zurück, wo es
/// hinkommt. Die Liste ist danach um eins länger; hineinschreiben muss der Aufrufer.
///
/// # Safety
/// `liste` muss eine gültige Liste mit Elementen dieser Größe sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_liste_neuer_platz(liste_zeiger: *mut ListenKopf, groesse: i64) -> *mut u8 {
    let l = unsafe { liste(liste_zeiger) };
    if l.laenge == l.platz {
        // Voll: doppelt so viel Platz holen und alles umziehen.
        let neuer_platz = (l.platz * 2).max(4);
        let neu = neue_daten(neuer_platz, groesse);
        let alt = daten_layout(l.platz, groesse);
        unsafe {
            std::ptr::copy_nonoverlapping(l.daten, neu, (l.laenge * groesse) as usize);
            if alt.size() > 0 {
                dealloc(l.daten, alt);
            }
        }
        l.daten = neu;
        l.platz = neuer_platz;
    }
    let stelle = unsafe { l.daten.add((l.laenge * groesse) as usize) };
    l.laenge += 1;
    stelle
}

/// Nimmt das Element an der Stelle heraus und rückt die dahinter nach.
/// (Was an dem Element hängt, hat der Aufrufer vorher losgelassen.)
///
/// # Safety
/// `liste` muss eine gültige Liste mit Elementen dieser Größe sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_liste_raus(liste_zeiger: *mut ListenKopf, stelle: i64, groesse: i64, zeile: i64) {
    let l = unsafe { liste(liste_zeiger) };
    if stelle < 0 || stelle >= l.laenge {
        oida_fehler_stelle(stelle, l.laenge, zeile);
    }
    let dahinter = (l.laenge - stelle - 1) * groesse;
    unsafe {
        let hier = l.daten.add((stelle * groesse) as usize);
        std::ptr::copy(hier.add(groesse as usize), hier, dahinter as usize);
    }
    l.laenge -= 1;
}

/// Gibt eine Liste frei. Der erzeugte Code ruft das auf, sobald ihr Zähler auf
/// 0 gefallen ist und er die Elemente losgelassen hat.
///
/// # Safety
/// `liste` muss eine Liste sein, die niemand mehr braucht.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_liste_frei(liste_zeiger: *mut ListenKopf, groesse: i64) {
    let l = unsafe { &mut *liste_zeiger };
    if l.zaehler != 0 {
        abbruch_compiler("a LISTN wird freigebn, obwohl sie no braucht wird");
    }
    let z = zustand();
    z.lebende_texte -= 1;
    if z.pruefen {
        // Als tot markieren und leeren: Jeder spätere Zugriff fällt dann auf.
        l.zaehler = TOT;
        l.laenge = 0;
        return;
    }
    let layout = daten_layout(l.platz, groesse);
    unsafe {
        if layout.size() > 0 {
            dealloc(l.daten, layout);
        }
        drop(Box::from_raw(liste_zeiger));
    }
}

// -------------------------------------------------------------- Verzeichnis

/// Ein Verzeichnis (Schlüssel -> Wert) am Heap. Die ersten vier Felder kennt
/// auch der erzeugte Code; Länge steht an derselben Stelle wie bei einer Liste.
#[repr(C)]
pub struct WbKopf {
    zaehler: i64,
    laenge: i64,
    /// Die Schlüssel in der Reihenfolge, in der sie dazugekommen sind:
    /// ganze Zahlen, oder Zeiger auf Texte (dann zählt das Verzeichnis die Texte mit).
    schluessel: *mut i64,
    /// Die Werte dazu, dicht hintereinander. Ihre Größe gibt der Compiler mit.
    werte: *mut u8,
    platz: i64,
    text: bool,
    /// Zum schnellen Finden: Prüfsumme des Schlüssels -> Stellen mit dieser Prüfsumme.
    index: *mut std::collections::HashMap<u64, Vec<usize>>,
}

/// Prüft, dass das Verzeichnis noch lebt.
unsafe fn verzeichnis<'a>(zeiger: *mut WbKopf) -> &'a mut WbKopf {
    let v = unsafe { &mut *zeiger };
    if v.zaehler <= TOT {
        abbruch_compiler("a VAZEICHNIS wird no verwendet, obwohl's scho freigebn is");
    }
    v
}

/// Die Prüfsumme eines Schlüssels. Texte zählen nach ihrem Inhalt (FNV-1a).
unsafe fn wb_summe(text: bool, schluessel: i64) -> u64 {
    if !text {
        return (schluessel as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
    let mut summe: u64 = 0xcbf2_9ce4_8422_2325;
    for b in unsafe { bytes(schluessel as *const TextKopf) } {
        summe = (summe ^ *b as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
    summe
}

unsafe fn wb_gleich(text: bool, a: i64, b: i64) -> bool {
    if text { unsafe { bytes(a as *const TextKopf) == bytes(b as *const TextKopf) } } else { a == b }
}

fn schluessel_layout(platz: i64) -> Layout {
    Layout::array::<i64>(platz as usize).expect("Verzeichnis zu groß")
}

/// Ein Text wird an einer weiteren Stelle gebraucht (feste Texte werden nicht gezählt).
unsafe fn text_behalten(text: *mut TextKopf) {
    unsafe {
        if (*text).zaehler >= 0 {
            (*text).zaehler += 1;
        }
    }
}

unsafe fn text_loslassen(text: *mut TextKopf) {
    unsafe {
        if (*text).zaehler > 0 {
            (*text).zaehler -= 1;
            if (*text).zaehler == 0 {
                oida_text_frei(text);
            }
        }
    }
}

/// Ein neues, leeres Verzeichnis. `text` sagt, ob die Schlüssel Texte sind (sonst ganze Zahlen).
#[unsafe(no_mangle)]
pub extern "C" fn oida_wb_neu(text: i64) -> *mut WbKopf {
    zustand().lebende_texte += 1;
    Box::into_raw(Box::new(WbKopf {
        zaehler: 1,
        laenge: 0,
        schluessel: std::ptr::NonNull::dangling().as_ptr(),
        werte: std::ptr::without_provenance_mut(ELEMENT_AUSRICHTUNG),
        platz: 0,
        text: text != 0,
        index: Box::into_raw(Box::default()),
    }))
}

/// An welcher Stelle der Schlüssel steht, oder -1.
///
/// # Safety
/// `zeiger` muss ein gültiges Verzeichnis sein und `schluessel` zu seiner Art passen.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_wb_finde(zeiger: *mut WbKopf, schluessel: i64) -> i64 {
    unsafe {
        let v = verzeichnis(zeiger);
        let summe = wb_summe(v.text, schluessel);
        if let Some(stellen) = (*v.index).get(&summe) {
            for &stelle in stellen {
                if wb_gleich(v.text, *v.schluessel.add(stelle), schluessel) {
                    return stelle as i64;
                }
            }
        }
        -1
    }
}

/// Hängt einen neuen Schlüssel an und gibt seine Stelle zurück. Den Wert
/// schreibt der Aufrufer dorthin. (Dass es den Schlüssel noch nicht gibt,
/// hat er vorher mit [`oida_wb_finde`] geprüft.)
///
/// # Safety
/// `zeiger` muss ein gültiges Verzeichnis mit Werten dieser Größe sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_wb_dazu(zeiger: *mut WbKopf, schluessel: i64, groesse: i64) -> i64 {
    unsafe {
        let v = verzeichnis(zeiger);
        if v.laenge == v.platz {
            // Voll: doppelt so viel Platz holen und alles umziehen.
            let neuer_platz = (v.platz * 2).max(4);
            let schluessel_neu = alloc(schluessel_layout(neuer_platz)) as *mut i64;
            if schluessel_neu.is_null() {
                handle_alloc_error(schluessel_layout(neuer_platz));
            }
            let werte_neu = neue_daten(neuer_platz, groesse);
            std::ptr::copy_nonoverlapping(v.schluessel, schluessel_neu, v.laenge as usize);
            std::ptr::copy_nonoverlapping(v.werte, werte_neu, (v.laenge * groesse) as usize);
            if v.platz > 0 {
                dealloc(v.schluessel as *mut u8, schluessel_layout(v.platz));
                let alt = daten_layout(v.platz, groesse);
                if alt.size() > 0 {
                    dealloc(v.werte, alt);
                }
            }
            v.schluessel = schluessel_neu;
            v.werte = werte_neu;
            v.platz = neuer_platz;
        }
        let stelle = v.laenge as usize;
        *v.schluessel.add(stelle) = schluessel;
        if v.text {
            text_behalten(schluessel as *mut TextKopf);
        }
        (*v.index).entry(wb_summe(v.text, schluessel)).or_default().push(stelle);
        v.laenge += 1;
        stelle as i64
    }
}

/// Nimmt den Eintrag an der Stelle heraus und rückt die dahinter nach.
/// (Was am Wert hängt, hat der Aufrufer vorher losgelassen.)
///
/// # Safety
/// `zeiger` muss ein gültiges Verzeichnis mit Werten dieser Größe sein, `stelle` eine Stelle darin.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_wb_raus(zeiger: *mut WbKopf, stelle: i64, groesse: i64) {
    unsafe {
        let v = verzeichnis(zeiger);
        let (stelle, laenge) = (stelle as usize, v.laenge as usize);
        let schluessel = *v.schluessel.add(stelle);
        let summe = wb_summe(v.text, schluessel);
        let index = &mut *v.index;
        if let Some(stellen) = index.get_mut(&summe) {
            stellen.retain(|s| *s != stelle);
            if stellen.is_empty() {
                index.remove(&summe);
            }
        }
        // Alles dahinter rückt um eins nach vorne, auch im Index.
        for stellen in index.values_mut() {
            for s in stellen.iter_mut() {
                if *s > stelle {
                    *s -= 1;
                }
            }
        }
        std::ptr::copy(v.schluessel.add(stelle + 1), v.schluessel.add(stelle), laenge - stelle - 1);
        let g = groesse as usize;
        std::ptr::copy(v.werte.add((stelle + 1) * g), v.werte.add(stelle * g), (laenge - stelle - 1) * g);
        v.laenge -= 1;
        if v.text {
            text_loslassen(schluessel as *mut TextKopf);
        }
    }
}

/// Die Schlüssel als neue Liste, in der Reihenfolge, in der sie dazugekommen sind.
///
/// # Safety
/// `zeiger` muss ein gültiges Verzeichnis sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_wb_schluessel(zeiger: *mut WbKopf) -> *mut ListenKopf {
    unsafe {
        let v = verzeichnis(zeiger);
        let liste = oida_liste_neu(8, v.laenge);
        std::ptr::copy_nonoverlapping(v.schluessel, (*liste).daten as *mut i64, v.laenge as usize);
        (*liste).laenge = v.laenge;
        if v.text {
            for i in 0..v.laenge as usize {
                text_behalten(*v.schluessel.add(i) as *mut TextKopf);
            }
        }
        liste
    }
}

/// Gibt ein Verzeichnis frei. Der erzeugte Code ruft das auf, sobald der
/// Zähler auf 0 gefallen ist und er die Werte losgelassen hat.
///
/// # Safety
/// `zeiger` muss ein Verzeichnis mit Werten dieser Größe sein, das niemand mehr braucht.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_wb_frei(zeiger: *mut WbKopf, groesse: i64) {
    unsafe {
        let v = &mut *zeiger;
        if v.zaehler != 0 {
            abbruch_compiler("a VAZEICHNIS wird freigebn, obwohl's no braucht wird");
        }
        if v.text {
            for i in 0..v.laenge as usize {
                text_loslassen(*v.schluessel.add(i) as *mut TextKopf);
            }
        }
        let z = zustand();
        z.lebende_texte -= 1;
        if z.pruefen {
            // Als tot markieren und leeren: Jeder spätere Zugriff fällt dann auf.
            v.zaehler = TOT;
            v.laenge = 0;
            return;
        }
        if v.platz > 0 {
            dealloc(v.schluessel as *mut u8, schluessel_layout(v.platz));
            let layout = daten_layout(v.platz, groesse);
            if layout.size() > 0 {
                dealloc(v.werte, layout);
            }
        }
        drop(Box::from_raw(v.index));
        drop(Box::from_raw(zeiger));
    }
}

// ------------------------------------------------------------------ Objekte

/// So beginnt jedes Objekt einer Klasse: Zähler, dann der Zeiger auf die
/// Angaben zur Klasse. Dahinter liegen die Felder; die kennt nur der Compiler.
#[repr(C)]
struct ObjektKopf {
    zaehler: i64,
    klasse: *const u8,
}

fn objekt_layout(groesse: i64) -> Layout {
    Layout::from_size_align(groesse as usize, std::mem::align_of::<ObjektKopf>()).expect("Objekt zu groß")
}

/// Legt Speicher für ein neues Objekt mit `groesse` Bytes an (Zähler 1). Die
/// Angaben zur Klasse und die Felder trägt der erzeugte Code ein.
#[unsafe(no_mangle)]
pub extern "C" fn oida_objekt_neu(groesse: i64) -> *mut u8 {
    let layout = objekt_layout(groesse);
    unsafe {
        let zeiger = alloc(layout);
        if zeiger.is_null() {
            handle_alloc_error(layout);
        }
        (*(zeiger as *mut ObjektKopf)).zaehler = 1;
        zustand().lebende_texte += 1;
        zeiger
    }
}

/// Dorthin zeigt im Prüfmodus alles, was ein freigegebenes Objekt über seine
/// Klasse wusste (freigeben, ausgeben, Sprungtabelle).
extern "C" fn objekt_tot() {
    abbruch_compiler("a Objekt wird no verwendet, obwohl's scho freigebn is");
}

static TOTE_KLASSE: [extern "C" fn(); 256] = [objekt_tot; 256];

/// Gibt ein Objekt frei. Der erzeugte Code ruft das auf, sobald der Zähler auf
/// 0 gefallen ist und er losgelassen hat, was in den Feldern steckt.
///
/// # Safety
/// `objekt` muss ein Objekt dieser Größe sein, das niemand mehr braucht.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_objekt_frei(objekt: *mut u8, groesse: i64) {
    unsafe {
        let kopf = objekt as *mut ObjektKopf;
        if (*kopf).zaehler != 0 {
            abbruch_compiler("a Objekt wird freigebn, obwohl's no braucht wird");
        }
        let z = zustand();
        z.lebende_texte -= 1;
        if z.pruefen {
            // Als tot markieren und die Felder überschreiben: Wer das Objekt
            // noch verwendet, fällt auf.
            (*kopf).zaehler = TOT;
            (*kopf).klasse = TOTE_KLASSE.as_ptr() as *const u8;
            let kopf_groesse = std::mem::size_of::<ObjektKopf>();
            std::ptr::write_bytes(objekt.add(kopf_groesse), 0xDD, groesse as usize - kopf_groesse);
        } else {
            dealloc(objekt, objekt_layout(groesse));
        }
    }
}

// ------------------------------------------------------------------- Zufall

/// Macht aus einem beliebigen Startwert einen guten, der nie 0 ist (SplitMix64).
fn samen_mischen(samen: u64) -> u64 {
    let mut z = samen.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)).max(1)
}

/// Die nächste Zufallszahl (xorshift64*). Ohne SOMEN startet die Folge bei
/// jedem Lauf anders.
fn zufallszahl() -> u64 {
    let z = zustand();
    if z.zufall == 0 {
        let zeit = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        z.zufall = samen_mischen(zeit ^ (u64::from(std::process::id()) << 32));
    }
    let mut x = z.zufall;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    z.zufall = x;
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

/// Eine Kommazahl von 0 bis unter 1.
#[unsafe(no_mangle)]
pub extern "C" fn oida_zufall() -> f64 {
    (zufallszahl() >> 11) as f64 / (1u64 << 53) as f64
}

/// Eine ganze Zahl von `von` bis einschließlich `bis`.
#[unsafe(no_mangle)]
pub extern "C" fn oida_zufall_zoi(von: i64, bis: i64, zeile: i64) -> i64 {
    if von > bis {
        abbruch(zeile, &format!("Zufoi von {von} bis {bis} geht ned: De erste Zoi is größa ois de zweite."));
    }
    let breite = (i128::from(bis) - i128::from(von) + 1) as u128;
    (i128::from(von) + (u128::from(zufallszahl()) % breite) as i128) as i64
}

/// Legt den Startwert fest: Danach kommt immer dieselbe Folge.
#[unsafe(no_mangle)]
pub extern "C" fn oida_zufall_samen(samen: i64) {
    zustand().zufall = samen_mischen(samen as u64);
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
    // Mit OIDA_SOMEN=<zahl> kommt bei jedem Lauf derselbe Zufall (für Tests).
    if let Some(samen) = std::env::var("OIDA_SOMEN").ok().and_then(|w| w.trim().parse::<i64>().ok()) {
        z.zufall = samen_mischen(samen as u64);
    }
}

/// Wird am Ende von jedem Programm aufgerufen.
#[unsafe(no_mangle)]
pub extern "C" fn oida_ende() {
    leeren();
    if zustand().pruefen {
        eprintln!("OIDA: {} Texte, Listen und Objekte übrig", zustand().lebende_texte);
    }
}

/// Liest eine Zeile von der Eingabe (ohne den Zeilenumbruch). `frage` wird
/// vorher ausgegeben, wenn sie nicht null ist. Am Ende der Eingabe kommt ein
/// leerer Text.
///
/// # Safety
/// `frage` muss null oder ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_lies(frage: *const TextKopf) -> *mut TextKopf {
    if !frage.is_null() {
        schreib(unsafe { bytes(frage) });
    }
    // Was bisher geschrieben wurde, muss am Bildschirm stehen, bevor gewartet wird.
    leeren();
    let mut zeile = String::new();
    let _ = std::io::stdin().read_line(&mut zeile);
    neuer_text(zeile.trim_end_matches(['\n', '\r']).as_bytes())
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

/// Ein Text in Anführungszeichen: So stehen Texte in Listen und Strukturen da.
///
/// # Safety
/// `text` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_schreib_text_zitiert(text: *const TextKopf) {
    schreib(b"\"");
    schreib(unsafe { bytes(text) });
    schreib(b"\"");
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

/// Zugriff auf eine Stelle, die es in der Liste nicht gibt.
#[unsafe(no_mangle)]
pub extern "C" fn oida_fehler_stelle(stelle: i64, laenge: i64, zeile: i64) -> ! {
    let meldung = match laenge {
        0 => format!("De LISTN is laar, a Stö {stelle} gibt's ned."),
        1 => format!("De LISTN hot nur a Element (de Stö 0), a Stö {stelle} gibt's ned."),
        n => format!("De LISTN hot {n} Elemente (Stön 0 bis {}), a Stö {stelle} gibt's ned.", n - 1),
    };
    abbruch(zeile, &meldung);
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_fehler_schritt_null(zeile: i64) -> ! {
    abbruch(zeile, "A Zöhschleifn mit Schritt 0 kummt nie weida.");
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
    fn stueck_zaehlt_zeichen_nicht_bytes() {
        assert_eq!(stueck("Grüß di", 0, 4), Ok("Grüß"));
        assert_eq!(stueck("Grüß di", 2, -1), Ok("üß di"));
        assert_eq!(stueck("Grüß di", 7, -1), Ok(""));
        assert_eq!(stueck("abc", 1, 0), Ok(""));
        assert!(stueck("abc", 4, -1).unwrap_err().contains("hot 3 Zeichn"));
        assert!(stueck("abc", -1, 1).is_err());
        assert!(stueck("abc", 1, 3).unwrap_err().contains("geht drüba naus"));
        assert!(stueck("abc", 1, -2).is_err());
    }

    #[test]
    fn suche_zaehlt_zeichen() {
        assert_eq!(suche("Grüß di", "di"), 5);
        assert_eq!(suche("Grüß di", "G"), 0);
        assert_eq!(suche("Grüß di", "x"), -1);
        assert_eq!(suche("abc", ""), 0);
    }

    #[test]
    fn zufall_mit_samen_ist_immer_gleich() {
        oida_zufall_samen(42);
        let erste: Vec<i64> = (0..5).map(|_| oida_zufall_zoi(1, 6, 0)).collect();
        oida_zufall_samen(42);
        let zweite: Vec<i64> = (0..5).map(|_| oida_zufall_zoi(1, 6, 0)).collect();
        assert_eq!(erste, zweite);
        assert!(erste.iter().all(|w| (1..=6).contains(w)));
        for _ in 0..1000 {
            let k = oida_zufall();
            assert!((0.0..1.0).contains(&k));
        }
        // Auch der ganze Bereich der 64-Bit-Zahlen geht.
        let _ = oida_zufall_zoi(i64::MIN, i64::MAX, 0);
        assert_eq!(oida_zufall_zoi(7, 7, 0), 7);
    }

    #[test]
    fn liste_waechst_und_schrumpft() {
        // Eine Liste von i64 (8 Bytes je Element), so wie der erzeugte Code sie benutzt.
        let l = oida_liste_neu(8, 0);
        for wert in 0..100i64 {
            unsafe { *(oida_liste_neuer_platz(l, 8) as *mut i64) = wert * 10 };
        }
        let lesen = |stelle: usize| unsafe { *((*l).daten as *const i64).add(stelle) };
        assert_eq!(unsafe { (*l).laenge }, 100);
        assert_eq!((lesen(0), lesen(57), lesen(99)), (0, 570, 990));
        unsafe { oida_liste_raus(l, 0, 8, 0) };
        unsafe { oida_liste_raus(l, 98, 8, 0) };
        unsafe { oida_liste_raus(l, 10, 8, 0) };
        assert_eq!(unsafe { (*l).laenge }, 97);
        assert_eq!((lesen(0), lesen(9), lesen(10), lesen(96)), (10, 100, 120, 980));
        unsafe {
            (*l).zaehler = 0;
            oida_liste_frei(l, 8);
        }
    }

    #[test]
    fn liste_ohne_groesse() {
        // Eine Struktur ohne Felder braucht keinen Speicher je Element.
        let l = oida_liste_neu(0, 3);
        for _ in 0..10 {
            unsafe { oida_liste_neuer_platz(l, 0) };
        }
        unsafe { oida_liste_raus(l, 3, 0, 0) };
        assert_eq!(unsafe { (*l).laenge }, 9);
        unsafe {
            (*l).zaehler = 0;
            oida_liste_frei(l, 0);
        }
    }

    #[test]
    fn verzeichnis_mit_zahlen() {
        let v = oida_wb_neu(0);
        unsafe {
            // 100 Einträge: Schlüssel i * 7, Wert i * i (8 Bytes).
            for i in 0..100i64 {
                assert_eq!(oida_wb_finde(v, i * 7), -1);
                let stelle = oida_wb_dazu(v, i * 7, 8);
                *((*v).werte as *mut i64).add(stelle as usize) = i * i;
            }
            assert_eq!((*v).laenge, 100);
            let wert = |schluessel: i64| {
                let stelle = oida_wb_finde(v, schluessel);
                (stelle >= 0).then(|| *((*v).werte as *mut i64).add(stelle as usize))
            };
            assert_eq!((wert(0), wert(7 * 9), wert(7 * 99), wert(5)), (Some(0), Some(81), Some(9801), None));
            // Einen in der Mitte heraus: Die dahinter rücken nach, und jeder wird noch gefunden.
            oida_wb_raus(v, oida_wb_finde(v, 7 * 50), 8);
            assert_eq!((*v).laenge, 99);
            assert_eq!((wert(7 * 50), wert(7 * 49), wert(7 * 51), wert(7 * 99)), (None, Some(2401), Some(2601), Some(9801)));
            // Die Schlüssel kommen in der Reihenfolge, in der sie dazugekommen sind.
            let liste = oida_wb_schluessel(v);
            let schluessel = std::slice::from_raw_parts((*liste).daten as *const i64, (*liste).laenge as usize);
            assert_eq!((schluessel.len(), schluessel[0], schluessel[50], schluessel[98]), (99, 0, 7 * 51, 7 * 99));
            (*liste).zaehler = 0;
            oida_liste_frei(liste, 8);
            (*v).zaehler = 0;
            oida_wb_frei(v, 8);
        }
    }

    #[test]
    fn verzeichnis_mit_texten() {
        let v = oida_wb_neu(1);
        let (hansi, sepp, hansi2) = (neuer_text(b"Hansi"), neuer_text(b"Sepp"), neuer_text(b"Hansi"));
        unsafe {
            let a = oida_wb_dazu(v, hansi as i64, 0);
            let b = oida_wb_dazu(v, sepp as i64, 0);
            assert_eq!((a, b), (0, 1));
            // Das Verzeichnis braucht seine Schlüssel selbst.
            assert_eq!(((*hansi).zaehler, (*sepp).zaehler), (2, 2));
            // Gefunden wird nach dem Inhalt, nicht nach dem Zeiger.
            assert_eq!(oida_wb_finde(v, hansi2 as i64), 0);
            oida_wb_raus(v, 0, 0);
            assert_eq!(((*hansi).zaehler, oida_wb_finde(v, hansi2 as i64), oida_wb_finde(v, sepp as i64)), (1, -1, 0));
            (*v).zaehler = 0;
            oida_wb_frei(v, 0);
            assert_eq!((*sepp).zaehler, 1);
            for text in [hansi, sepp, hansi2] {
                (*text).zaehler = 0;
                oida_text_frei(text);
            }
        }
    }

    #[test]
    fn objekt_anlegen_und_freigeben() {
        let objekt = oida_objekt_neu(40);
        let kopf = objekt as *mut ObjektKopf;
        unsafe {
            assert_eq!((*kopf).zaehler, 1);
            // Der Platz hinter dem Kopf gehört den Feldern.
            std::ptr::write_bytes(objekt.add(std::mem::size_of::<ObjektKopf>()), 7, 24);
            (*kopf).zaehler = 0;
            oida_objekt_frei(objekt, 40);
        }
        // Auch ein Objekt ohne Felder braucht Platz für den Kopf.
        let leer = oida_objekt_neu(std::mem::size_of::<ObjektKopf>() as i64);
        unsafe {
            (*(leer as *mut ObjektKopf)).zaehler = 0;
            oida_objekt_frei(leer, std::mem::size_of::<ObjektKopf>() as i64);
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
