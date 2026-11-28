//! Nebenbei: mehrere Dinge gleichzeitig tun (für `NEBNBEI`, `WOAT AUF` und `BRAUCH nebnbei OIDA`).
//!
//! `NEBNBEI f(a, b)` lässt eine Funktion in einem eigenen Thread laufen. Die Regel, die alles
//! einfach hält: **Zwei Threads teilen sich nie einen Wert.** Was ein Thread mitbekommt, ist eine
//! eigene Kopie ([`kopiere`]); was er über einen Kanal schickt, wird beim Schicken kopiert; und
//! was er am Ende zurückgibt, gehört ab dann dem, der darauf wartet. Darum braucht kein Zähler
//! an einem Text oder einer Liste eine Sperre, und ein Programm ohne `NEBNBEI` ist so schnell
//! wie vorher.
//!
//! Ein Wert, der gerade unterwegs ist (in einem Kanal, als Mitgabe, als Ergebnis), liegt in
//! einem [`Paket`]: ein Stück Speicher, das so aussieht wie der Wert im Programm.
//!
//! # Wenn alle warten
//!
//! Wartet ein Thread ohne Ende auf einen Kanal oder auf eine Aufgabe, kann ihn nur ein anderer
//! Thread des Programms wecken. Warten alle so, geht nie mehr etwas weiter; das merkt die
//! Laufzeit ([`LEBEND`], [`SCHLAFEND`]) und hört mit einer Meldung auf, statt für immer zu hängen.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::json::{
    ART_AUFZAEHLUNG, ART_BELIEBIG, ART_JONA, ART_KETTN, ART_KOMMAZOI, ART_LISTN, ART_STRUKTUR, ART_VAZEICHNIS,
    ART_VIELLEICHT_WERT, ART_VIELLEICHT_ZEIGER, ART_ZOI, JsonTyp,
};
use super::{
    ListenKopf, TOT, TextKopf, WbKopf, abbruch, abbruch_compiler, abbruch_ohne_zeile, bytes, neuer_text, oida_liste_neu,
    oida_wb_dazu, oida_wb_neu, text_loslassen,
};

// ------------------------------------------------------------ tiefe Kopie

/// Tiefer verschachtelt geht es nicht (schützt vor einer Liste, die sich selbst enthält).
const HOECHSTE_TIEFE: usize = 2_000;

/// Eine eigene Kopie eines Textes. Feste Texte aus dem Programm werden nie gezählt und nie
/// freigegeben: Die können sich alle Threads teilen.
unsafe fn text_kopie(text: *mut TextKopf) -> *mut TextKopf {
    unsafe { if (*text).zaehler == -1 { text } else { neuer_text(bytes(text)) } }
}

/// Liegt ein Wert dieses Typs ganz an Ort und Stelle (ohne Zeiger woandershin)?
fn flach(typ: &JsonTyp) -> bool {
    matches!(typ.art, ART_ZOI | ART_KOMMAZOI | ART_JONA | ART_AUFZAEHLUNG)
}

unsafe fn listen_kopie(quelle: *const ListenKopf, element: &JsonTyp, tiefe: usize) -> *mut ListenKopf {
    unsafe {
        if (*quelle).zaehler <= TOT {
            abbruch_compiler("a LISTN wird no verwendet, obwohl sie scho freigebn is");
        }
        let (laenge, groesse) = ((*quelle).laenge as usize, element.groesse as usize);
        let neu = oida_liste_neu(element.groesse, laenge as i64);
        (*neu).laenge = laenge as i64;
        if flach(element) {
            std::ptr::copy_nonoverlapping((*quelle).daten, (*neu).daten, laenge * groesse);
        } else {
            for i in 0..laenge {
                kopiere((*quelle).daten.add(i * groesse), (*neu).daten.add(i * groesse), element, tiefe + 1);
            }
        }
        neu
    }
}

/// Schreibt nach `ziel` eine eigene Kopie des Werts bei `quelle`: mit allem, was daran hängt
/// (Texte, Listen, Verzeichnisse), sodass die Kopie und das Original nichts mehr gemeinsam haben.
/// Die Kopie gehört dem, der sie bestellt hat (alle Zähler darin stehen auf 1).
///
/// # Safety
/// `quelle` muss auf einen Wert des Typs zeigen, den `typ` beschreibt, `ziel` auf Platz dafür.
pub(crate) unsafe fn kopiere(quelle: *const u8, ziel: *mut u8, typ: &JsonTyp, tiefe: usize) {
    if tiefe > HOECHSTE_TIEFE {
        abbruch_ohne_zeile("Des is z'tiaf verschachtelt zum Kopiern (oda es zagt im Kreis auf si söwa).");
    }
    unsafe {
        let zeiger = |wo: *const u8| (wo as *const *mut u8).read_unaligned();
        let setz = |wo: *mut u8, wert: *mut u8| (wo as *mut *mut u8).write_unaligned(wert);
        match typ.art {
            ART_ZOI | ART_KOMMAZOI | ART_AUFZAEHLUNG => std::ptr::copy_nonoverlapping(quelle, ziel, 8),
            ART_JONA => ziel.write(quelle.read()),
            ART_KETTN => setz(ziel, text_kopie(zeiger(quelle).cast()).cast()),
            ART_VIELLEICHT_ZEIGER => {
                if zeiger(quelle).is_null() {
                    setz(ziel, std::ptr::null_mut());
                } else {
                    kopiere(quelle, ziel, typ.innen(), tiefe);
                }
            }
            ART_VIELLEICHT_WERT => {
                std::ptr::write_bytes(ziel, 0, typ.groesse as usize);
                if quelle.read() != 0 {
                    ziel.write(1);
                    let stelle = typ.anzahl as usize;
                    kopiere(quelle.add(stelle), ziel.add(stelle), typ.innen(), tiefe);
                }
            }
            ART_STRUKTUR => {
                std::ptr::write_bytes(ziel, 0, typ.groesse as usize);
                for feld in typ.felder() {
                    kopiere(quelle.add(feld.stelle as usize), ziel.add(feld.stelle as usize), &*feld.typ, tiefe);
                }
            }
            ART_LISTN => setz(ziel, listen_kopie(zeiger(quelle).cast(), typ.innen(), tiefe).cast()),
            ART_VAZEICHNIS => {
                let alt = &*(zeiger(quelle) as *const WbKopf);
                if alt.zaehler <= TOT {
                    abbruch_compiler("a VAZEICHNIS wird no verwendet, obwohl's scho freigebn is");
                }
                let innen = typ.innen();
                let groesse = innen.groesse as usize;
                let neu = oida_wb_neu(i64::from(alt.text));
                for i in 0..alt.laenge as usize {
                    let mut schluessel = *alt.schluessel.add(i);
                    if alt.text {
                        schluessel = text_kopie(schluessel as *mut TextKopf) as i64;
                    }
                    let stelle = oida_wb_dazu(neu, schluessel, innen.groesse) as usize;
                    if alt.text {
                        // Das Verzeichnis zählt den Text jetzt selbst mit.
                        text_loslassen(schluessel as *mut TextKopf);
                    }
                    kopiere(alt.werte.add(i * groesse), (*neu).werte.add(stelle * groesse), innen, tiefe + 1);
                }
                setz(ziel, neu.cast());
            }
            ART_BELIEBIG => {
                // Die Struktur `Json` aus der Bibliothek: Von ihren sieben Feldern steht in der
                // Beschreibung nur, wo sie liegen. Drei davon sind Zeiger: der Text, die Teile
                // (wieder `Json`) und die Namen.
                std::ptr::copy_nonoverlapping(quelle, ziel, typ.groesse as usize);
                let felder = typ.felder();
                let (von, nach) = (|n: usize| quelle.add(felder[n].stelle as usize), |n: usize| ziel.add(felder[n].stelle as usize));
                if !zeiger(von(4)).is_null() {
                    setz(nach(4), text_kopie(zeiger(von(4)).cast()).cast());
                }
                if !zeiger(von(5)).is_null() {
                    setz(nach(5), listen_kopie(zeiger(von(5)).cast(), typ, tiefe).cast());
                }
                if !zeiger(von(6)).is_null() {
                    let kettn = JsonTyp { art: ART_KETTN, groesse: 8, anzahl: 0, innen: std::ptr::null(), felder: std::ptr::null() };
                    setz(nach(6), listen_kopie(zeiger(von(6)).cast(), &kettn, tiefe).cast());
                }
            }
            _ => abbruch_compiler("so a Wert kaun ned kopiert wern"),
        }
    }
}

/// Ein Wert, der gerade keinem Thread gehört: unterwegs in einem Kanal, als Mitgabe für einen
/// neuen Thread oder als dessen Ergebnis. Der Speicher ist auf 8 Bytes ausgerichtet, wie alles in OIDA.
struct Paket(Vec<u64>);

impl Paket {
    fn neu(groesse: i64) -> Paket {
        Paket(vec![0; (groesse.max(0) as usize).div_ceil(8)])
    }

    fn zeiger(&mut self) -> *mut u8 {
        self.0.as_mut_ptr().cast()
    }

    /// Stellt den Wert an seinen Platz im Programm. Ab da gehört er dem, der ihn geholt hat.
    unsafe fn auspacken(&self, ziel: *mut u8, groesse: i64) {
        let bytes = (groesse.max(0) as usize).min(self.0.len() * 8);
        unsafe { std::ptr::copy_nonoverlapping(self.0.as_ptr().cast::<u8>(), ziel, bytes) };
    }
}

// ------------------------------------------------------------- wer wartet

/// Wie viele Threads das Programm gerade hat; das Hauptprogramm zählt mit.
static LEBEND: AtomicI64 = AtomicI64::new(1);
/// Wie viele davon ohne Ende auf einen Kanal oder eine Aufgabe warten.
static SCHLAFEND: AtomicI64 = AtomicI64::new(0);

fn alle_warten() -> ! {
    abbruch_ohne_zeile(
        "Jetzt woatn olle, und kana tuat mehr wos: Jeda Thread woat auf an Kanal oda auf a Aufgab, und es is kana mehr do, der wos schickn oda fertig wern kunnt.",
    );
}

/// Wer an einer Stelle (einem Kanal, einer Aufgabe) ohne Ende wartet. Geändert wird das nur
/// unter der Sperre dieser Stelle.
#[derive(Default)]
struct Warteraum {
    wartende: usize,
    /// Wie viele davon schon geweckt sind, aber noch nicht wieder laufen.
    geweckt: usize,
}

impl Warteraum {
    /// Weckt bis zu `wie_viele` von denen, die hier warten. Ab jetzt zählen sie wieder als wach:
    /// Das steht fest, bevor der Wecker läutet, damit niemand sie für eingeschlafen hält.
    fn wecken(&mut self, wecker: &Condvar, wie_viele: usize) {
        let anzahl = self.wartende.min(wie_viele);
        if anzahl > 0 {
            self.wartende -= anzahl;
            self.geweckt += anzahl;
            SCHLAFEND.fetch_sub(anzahl as i64, Ordering::SeqCst);
        }
        // Auch wer nur eine bestimmte Zeit wartet, soll nachschauen.
        wecker.notify_all();
    }
}

fn sperre<T>(schloss: &Mutex<T>) -> MutexGuard<'_, T> {
    schloss.lock().unwrap_or_else(|vergiftet| vergiftet.into_inner())
}

/// Wartet ohne Ende, bis jemand weckt. Wäre dieser Thread der letzte, der noch wach ist,
/// hört das Programm mit einer Meldung auf.
fn schlafen<'a, T>(mut wache: MutexGuard<'a, T>, wecker: &Condvar, raum: impl Fn(&mut T) -> &mut Warteraum) -> MutexGuard<'a, T> {
    raum(&mut wache).wartende += 1;
    let schlafend = SCHLAFEND.fetch_add(1, Ordering::SeqCst) + 1;
    if schlafend >= LEBEND.load(Ordering::SeqCst) {
        alle_warten();
    }
    loop {
        wache = wecker.wait(wache).unwrap_or_else(|vergiftet| vergiftet.into_inner());
        let raum = raum(&mut wache);
        if raum.geweckt > 0 {
            raum.geweckt -= 1;
            return wache;
        }
    }
}

// ---------------------------------------------------------------- Kanäle

struct KanalStand {
    werte: VecDeque<Paket>,
    zu: bool,
    raum: Warteraum,
}

struct Kanal {
    stand: Mutex<KanalStand>,
    wecker: Condvar,
}

/// Alle Kanäle des Programms; die Nummer im Programm (`Kanal.numma`) ist die Stelle hier.
static KANAELE: Mutex<Vec<Arc<Kanal>>> = Mutex::new(Vec::new());

fn neuer_kanal() -> i64 {
    let kanal = Arc::new(Kanal {
        stand: Mutex::new(KanalStand { werte: VecDeque::new(), zu: false, raum: Warteraum::default() }),
        wecker: Condvar::new(),
    });
    let mut alle = sperre(&KANAELE);
    alle.push(kanal);
    alle.len() as i64 - 1
}

/// Der Kanal, den ein Programm in der Hand hat. Im Programm ist das eine Struktur mit einem
/// Feld, der Nummer.
unsafe fn kanal(wert: *const i64) -> Arc<Kanal> {
    let nummer = unsafe { wert.read_unaligned() };
    let gefunden = usize::try_from(nummer).ok().and_then(|n| sperre(&KANAELE).get(n).cloned());
    gefunden.unwrap_or_else(|| abbruch_ohne_zeile("Den Kanal gibt's ned. An Kanal mocht ma mit neicha_kanal."))
}

/// Wie lange jemand auf einen Wert wartet.
enum Geduld {
    Immer,
    Bis(Instant),
}

/// Holt den ältesten Wert aus dem Kanal. `None` heißt: Es kommt keiner (der Kanal ist zu und
/// leer, oder die Zeit ist um).
fn holen(kanal: &Kanal, geduld: Geduld) -> Option<Paket> {
    let mut stand = sperre(&kanal.stand);
    let mut nachgeschaut = false;
    loop {
        if let Some(paket) = stand.werte.pop_front() {
            return Some(paket);
        }
        if stand.zu {
            return None;
        }
        if let Geduld::Bis(frist) = geduld
            && Instant::now() >= frist
        {
            return None;
        }
        // Bevor sich der Thread schlafen legt, schaut er noch kurz immer wieder nach: Antwortet
        // der andere gleich, spart das einmal Schlafenlegen und Wecken, und das ist das Teuerste
        // an einem Kanal (unter Linux in einer virtuellen Maschine an die 50 Mikrosekunden).
        if !nachgeschaut && mehrere_kerne() {
            nachgeschaut = true;
            let anfang = Instant::now();
            while stand.werte.is_empty() && !stand.zu && anfang.elapsed() < NACHSCHAUEN {
                drop(stand);
                for _ in 0..16 {
                    std::hint::spin_loop();
                }
                stand = sperre(&kanal.stand);
            }
            continue;
        }
        // Was dieser Thread schon geschrieben hat, soll dastehen, während er wartet.
        super::zeilen_leeren();
        match geduld {
            Geduld::Immer => stand = schlafen(stand, &kanal.wecker, |s| &mut s.raum),
            Geduld::Bis(frist) => {
                let jetzt = Instant::now();
                if jetzt >= frist {
                    return None;
                }
                stand = kanal.wecker.wait_timeout(stand, frist - jetzt).unwrap_or_else(|vergiftet| vergiftet.into_inner()).0;
            }
        }
    }
}

/// So lange schaut ein Thread nach, ob doch gleich etwas kommt, bevor er sich schlafen legt.
const NACHSCHAUEN: Duration = Duration::from_micros(60);

/// Auf einem Rechner mit nur einem Kern bringt Nachschauen nichts: Solange dieser Thread
/// nachschaut, kann kein anderer etwas schicken.
fn mehrere_kerne() -> bool {
    static MEHRERE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *MEHRERE.get_or_init(|| std::thread::available_parallelism().is_ok_and(|n| n.get() > 1))
}

/// `neicha_kanal VON T()`: ein neuer, leerer Kanal.
///
/// # Safety
/// `ziel` muss auf Platz für eine Struktur `Kanal` zeigen.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_kanal_neu(ziel: *mut i64, _typ: *const JsonTyp) {
    unsafe { ziel.write_unaligned(neuer_kanal()) };
}

/// `schick_eine(kanal, wert)`: legt eine Kopie des Werts in den Kanal. Wartet nie.
///
/// # Safety
/// `wert` muss auf einen Wert des Typs zeigen, den `typ` beschreibt.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_kanal_schicken(wo: *const i64, _kanal_typ: *const JsonTyp, wert: *const u8, typ: *const JsonTyp) {
    unsafe {
        let kanal = kanal(wo);
        if sperre(&kanal.stand).zu {
            abbruch_ohne_zeile("In an Kanal, der zua is, kaun ma nix mehr schickn.");
        }
        let typ = &*typ;
        let mut paket = Paket::neu(typ.groesse);
        kopiere(wert, paket.zeiger(), typ, 0);
        let mut stand = sperre(&kanal.stand);
        stand.werte.push_back(paket);
        stand.raum.wecken(&kanal.wecker, 1);
    }
}

/// `hol_aussa(kanal)`: der älteste Wert; wartet, bis einer da ist. Aus einem Kanal, der zu und leer
/// ist, kommt nichts mehr: Das ist hier ein Fehler im Programm.
///
/// # Safety
/// `ziel` muss auf Platz für einen Wert des Typs zeigen, den `typ` beschreibt.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_kanal_holen(wo: *const i64, _kanal_typ: *const JsonTyp, ziel: *mut u8, typ: *const JsonTyp) {
    unsafe {
        match holen(&kanal(wo), Geduld::Immer) {
            Some(paket) => paket.auspacken(ziel, (*typ).groesse),
            None => abbruch_ohne_zeile(
                "Aus dem Kanal kummt nix mehr: Er is zua und laa. (Mit aussa_oda_nix kriagst in dem Foi NIX statt ana Meldung.)",
            ),
        }
    }
}

/// Schreibt nach `ziel` ein `VIELLEICHT T`: den Wert aus dem Paket, oder NIX.
unsafe fn vielleicht_schreiben(paket: Option<Paket>, ziel: *mut u8, typ: &JsonTyp) {
    unsafe {
        std::ptr::write_bytes(ziel, 0, typ.groesse as usize);
        let Some(paket) = paket else { return };
        match typ.art {
            // Ein Zeiger, bei dem NIX der Nullzeiger ist.
            ART_VIELLEICHT_ZEIGER => paket.auspacken(ziel, typ.groesse),
            // Vorne der Merker "ist da", dahinter der Wert.
            ART_VIELLEICHT_WERT => {
                ziel.write(1);
                paket.auspacken(ziel.add(typ.anzahl as usize), typ.innen().groesse);
            }
            _ => abbruch_compiler("aussa_oda_nix gibt ka VIELLEICHT zruck"),
        }
    }
}

/// `aussa_oda_nix(kanal)`: wie `hol_aussa`, aber aus einem Kanal, der zu und leer ist, kommt NIX.
///
/// # Safety
/// `ziel` muss auf Platz für ein `VIELLEICHT T` zeigen, das `typ` beschreibt.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_kanal_holen_vielleicht(wo: *const i64, _kanal_typ: *const JsonTyp, ziel: *mut u8, typ: *const JsonTyp) {
    unsafe { vielleicht_schreiben(holen(&kanal(wo), Geduld::Immer), ziel, &*typ) };
}

/// `aussa_bis(kanal, millisekundn)`: wartet höchstens so lange; ist dann nichts da, kommt NIX.
/// Mit 0 wird gar nicht gewartet.
///
/// # Safety
/// `ziel` muss auf Platz für ein `VIELLEICHT T` zeigen, das `typ` beschreibt.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_kanal_holen_bis(
    wo: *const i64,
    _kanal_typ: *const JsonTyp,
    millisekunden: i64,
    ziel: *mut u8,
    typ: *const JsonTyp,
) {
    let frist = Instant::now() + Duration::from_millis(millisekunden.clamp(0, 1_000_000_000) as u64);
    unsafe { vielleicht_schreiben(holen(&kanal(wo), Geduld::Bis(frist)), ziel, &*typ) };
}

/// `moch_zua(kanal)`: Ab jetzt kommt nichts Neues mehr hinein. Was noch drin ist, kann man holen.
///
/// # Safety
/// `wo` muss auf eine Struktur `Kanal` zeigen.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_kanal_zu(wo: *const i64, _kanal_typ: *const JsonTyp) {
    let kanal = unsafe { kanal(wo) };
    let mut stand = sperre(&kanal.stand);
    stand.zu = true;
    // Alle, die warten, sollen sehen, dass nichts mehr kommt.
    stand.raum.wecken(&kanal.wecker, usize::MAX);
}

/// # Safety
/// `wo` muss auf eine Struktur `Kanal` zeigen.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_kanal_ist_zu(wo: *const i64, _kanal_typ: *const JsonTyp) -> bool {
    sperre(&unsafe { kanal(wo) }.stand).zu
}

/// Wie viele Werte im Kanal liegen und noch nicht geholt sind.
///
/// # Safety
/// `wo` muss auf eine Struktur `Kanal` zeigen.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_kanal_anzahl(wo: *const i64, _kanal_typ: *const JsonTyp) -> i64 {
    sperre(&unsafe { kanal(wo) }.stand).werte.len() as i64
}

// --------------------------------------------------------------- Aufgaben

#[derive(Default)]
struct AufgabenStand {
    fertig: bool,
    /// Was die Funktion zurückgegeben hat; weg, sobald es jemand abgeholt hat.
    ergebnis: Option<Paket>,
    raum: Warteraum,
}

#[derive(Default)]
struct Aufgabe {
    stand: Mutex<AufgabenStand>,
    wecker: Condvar,
}

/// Alle Aufgaben, die laufen oder deren Ergebnis noch niemand abgeholt hat. Die Nummer im
/// Programm (`Aufgab.numma`) ist die Stelle hier; ein Platz wird nie wieder vergeben.
static AUFGABEN: Mutex<Vec<Option<Arc<Aufgabe>>>> = Mutex::new(Vec::new());

/// So viel Stapel bekommt jeder Thread: genug für Funktionen, die sich tief selbst aufrufen.
/// (Belegt wird davon nur, was wirklich gebraucht wird.)
const STAPEL: usize = 16 * 1024 * 1024;

/// `NEBNBEI f(a, b)`: startet einen Thread und gibt die Nummer der Aufgabe zurück.
///
/// `code` ist ein Stück vom Compiler, das die Werte aus `mitgabe` nimmt, die Funktion aufruft
/// und ihr Ergebnis nach `ergebnis` legt. `mitgabe` zeigt auf die Werte, wie der Aufrufer sie
/// hat (alle hintereinander, wie eine Struktur); `typ` beschreibt sie. Der Thread bekommt davon
/// eine eigene Kopie.
///
/// # Safety
/// `mitgabe` muss auf Werte zeigen, wie `typ` sie beschreibt, und `code` muss dazu passen.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_nebnbei_start(
    code: unsafe extern "C" fn(*mut u8, *mut u8),
    mitgabe: *const u8,
    typ: *const JsonTyp,
    ergebnis_groesse: i64,
) -> i64 {
    let typ = unsafe { &*typ };
    let mut werte = Paket::neu(typ.groesse);
    unsafe { kopiere(mitgabe, werte.zeiger(), typ, 0) };

    let aufgabe = Arc::new(Aufgabe::default());
    let nummer = {
        let mut alle = sperre(&AUFGABEN);
        alle.push(Some(aufgabe.clone()));
        alle.len() as i64 - 1
    };
    // Was bisher geschrieben wurde, steht vor allem, was der neue Thread schreibt.
    super::zeilen_leeren();
    // Der neue Thread würfelt für sich; mit festem SOMEN kommt trotzdem immer dasselbe heraus.
    let samen = super::zufallszahl();
    super::es_gibt_mehrere();
    LEBEND.fetch_add(1, Ordering::SeqCst);
    let gestartet = std::thread::Builder::new().stack_size(STAPEL).spawn(move || {
        super::thread_beginnt(samen);
        let mut werte = werte;
        let mut ergebnis = Paket::neu(ergebnis_groesse);
        // SAFETY: Der Compiler hat `code` für genau diese Werte gebaut. Er nimmt sie aus dem
        // Paket heraus; danach ist das Paket nur noch leerer Speicher.
        unsafe { code(werte.zeiger(), ergebnis.zeiger()) };
        drop(werte);
        super::thread_endet();
        {
            let mut stand = sperre(&aufgabe.stand);
            stand.ergebnis = Some(ergebnis);
            stand.fertig = true;
            stand.raum.wecken(&aufgabe.wecker, usize::MAX);
        }
        // Erst jetzt ist dieser Thread weg: Wer auf ihn gewartet hat, zählt schon wieder als wach.
        let uebrig = LEBEND.fetch_sub(1, Ordering::SeqCst) - 1;
        if uebrig > 0 && SCHLAFEND.load(Ordering::SeqCst) >= uebrig {
            alle_warten();
        }
    });
    if let Err(fehler) = gestartet {
        abbruch_ohne_zeile(&format!("Do geht ka Thread mehr auf: {fehler}"));
    }
    nummer
}

fn aufgabe(nummer: i64) -> Option<Arc<Aufgabe>> {
    let alle = sperre(&AUFGABEN);
    usize::try_from(nummer).ok().and_then(|n| alle.get(n)).and_then(Clone::clone)
}

/// `WOAT AUF aufgab`: wartet, bis die Funktion fertig ist, und stellt ihr Ergebnis nach `ziel`.
/// Das Ergebnis gehört ab dann dem, der gewartet hat; ein zweites Mal gibt es das nicht.
///
/// # Safety
/// `ziel` muss auf Platz für `groesse` Bytes zeigen: so groß, wie das Ergebnis der Funktion ist.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_nebnbei_warten(nummer: i64, ziel: *mut u8, groesse: i64, zeile: i64) {
    const SCHON: &str = "Auf de Aufgab hot scho wer gwoat. Wos a Aufgab ausgrechnt hot, kriagt ma nur a anzigs Moi.";
    let Some(aufgabe) = aufgabe(nummer) else { abbruch(zeile, SCHON) };
    let mut stand = sperre(&aufgabe.stand);
    while !stand.fertig {
        super::zeilen_leeren();
        stand = schlafen(stand, &aufgabe.wecker, |s| &mut s.raum);
    }
    let Some(ergebnis) = stand.ergebnis.take() else { abbruch(zeile, SCHON) };
    drop(stand);
    unsafe { ergebnis.auspacken(ziel, groesse) };
    if let Some(platz) = usize::try_from(nummer).ok().and_then(|n| sperre(&AUFGABEN).get_mut(n).map(Option::take)) {
        drop(platz);
    }
}

/// `is_fertig(aufgab)`: Ist die Funktion schon fertig? Wartet nicht.
///
/// # Safety
/// `wo` muss auf eine Struktur `Aufgab` zeigen.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_nebnbei_fertig(wo: *const i64, _typ: *const JsonTyp) -> bool {
    // Eine Aufgabe, auf die schon jemand gewartet hat, ist auch fertig.
    aufgabe(unsafe { wo.read_unaligned() }).is_none_or(|aufgabe| sperre(&aufgabe.stand).fertig)
}

/// `kerne()`: wie viele Dinge der Rechner wirklich gleichzeitig tun kann.
#[unsafe(no_mangle)]
pub extern "C" fn oida_nebnbei_kerne() -> i64 {
    std::thread::available_parallelism().map_or(1, |n| n.get() as i64)
}

/// `schlof(millisekundn)`: Der Thread tut so lange nichts.
#[unsafe(no_mangle)]
pub extern "C" fn oida_nebnbei_schlafen(millisekunden: i64) {
    super::zeilen_leeren();
    std::thread::sleep(Duration::from_millis(millisekunden.clamp(0, 1_000_000_000) as u64));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::JsonFeld;
    use crate::{oida_liste_frei, oida_text_frei, oida_wb_finde, oida_wb_frei};

    fn typ(art: i64, groesse: i64, anzahl: i64, innen: *const JsonTyp, felder: Vec<JsonFeld>) -> &'static JsonTyp {
        let felder: &'static [JsonFeld] = Box::leak(felder.into_boxed_slice());
        Box::leak(Box::new(JsonTyp { art, groesse, anzahl, innen, felder: felder.as_ptr() }))
    }

    fn einfach(art: i64) -> &'static JsonTyp {
        typ(art, if art == ART_JONA { 1 } else { 8 }, 0, std::ptr::null(), Vec::new())
    }

    fn feld(stelle: usize, typ: &'static JsonTyp) -> JsonFeld {
        JsonFeld { name: std::ptr::null(), stelle: stelle as i64, typ }
    }

    fn inhalt(text: *const TextKopf) -> String {
        String::from_utf8(unsafe { bytes(text) }.to_vec()).unwrap()
    }

    /// So liegt im Programm eine Struktur mit einem Text, einer Liste von Texten, einem
    /// `VIELLEICHT ZOI` und einem Verzeichnis von Texten auf Listen von Zahlen.
    #[repr(C)]
    struct Spieler {
        name: *mut TextKopf,
        sachen: *mut ListenKopf,
        da: u8,
        punkte: i64,
        runden: *mut WbKopf,
    }

    fn spieler_typ() -> &'static JsonTyp {
        let kettn = einfach(ART_KETTN);
        let zoi = einfach(ART_ZOI);
        let texte = typ(ART_LISTN, 8, 0, kettn, Vec::new());
        let vielleicht = typ(ART_VIELLEICHT_WERT, 16, 8, zoi, Vec::new());
        let zahlen = typ(ART_LISTN, 8, 0, zoi, Vec::new());
        let runden = typ(ART_VAZEICHNIS, 8, 0, zahlen, Vec::new());
        typ(
            ART_STRUKTUR,
            std::mem::size_of::<Spieler>() as i64,
            4,
            std::ptr::null(),
            vec![feld(0, kettn), feld(8, texte), feld(16, vielleicht), feld(32, runden)],
        )
    }

    unsafe fn neuer_spieler() -> Spieler {
        unsafe {
            let sachen = oida_liste_neu(8, 2);
            ((*sachen).daten as *mut *mut TextKopf).write(neuer_text(b"Schwert"));
            ((*sachen).daten as *mut *mut TextKopf).add(1).write(neuer_text("Schüd".as_bytes()));
            (*sachen).laenge = 2;
            let runden = oida_wb_neu(1);
            let schluessel = neuer_text(b"heit");
            let stelle = oida_wb_dazu(runden, schluessel as i64, 8) as usize;
            text_loslassen(schluessel);
            let zahlen = oida_liste_neu(8, 3);
            std::ptr::copy_nonoverlapping([7i64, 8, 9].as_ptr(), (*zahlen).daten as *mut i64, 3);
            (*zahlen).laenge = 3;
            ((*runden).werte as *mut *mut ListenKopf).add(stelle).write(zahlen);
            Spieler { name: neuer_text(b"Hansi"), sachen, da: 1, punkte: 1200, runden }
        }
    }

    #[test]
    fn die_kopie_hat_mit_dem_original_nichts_gemeinsam() {
        unsafe {
            let original = neuer_spieler();
            let mut kopie = std::mem::MaybeUninit::<Spieler>::uninit();
            kopiere((&original as *const Spieler).cast(), kopie.as_mut_ptr().cast(), spieler_typ(), 0);
            let kopie = kopie.assume_init();

            // Derselbe Inhalt ...
            assert_eq!(inhalt(kopie.name), "Hansi");
            assert_eq!((kopie.da, kopie.punkte), (1, 1200));
            assert_eq!((*kopie.sachen).laenge, 2);
            assert_eq!(inhalt(((*kopie.sachen).daten as *const *const TextKopf).add(1).read()), "Schüd");
            assert_eq!((*kopie.runden).laenge, 1);
            let schluessel = neuer_text(b"heit");
            let stelle = oida_wb_finde(kopie.runden, schluessel as i64);
            assert_eq!(stelle, 0);
            let zahlen = ((*kopie.runden).werte as *const *const ListenKopf).read();
            assert_eq!(std::slice::from_raw_parts((*zahlen).daten as *const i64, 3), [7, 8, 9]);

            // ... aber nichts davon ist dasselbe Stück Speicher, und jedes gehört nur der Kopie.
            assert_ne!(kopie.name, original.name);
            assert_ne!(kopie.sachen, original.sachen);
            assert_ne!(kopie.runden, original.runden);
            let (alt, neu) = ((*original.sachen).daten as *const *const TextKopf, (*kopie.sachen).daten as *const *const TextKopf);
            assert_ne!(alt.read(), neu.read());
            assert_ne!(((*original.runden).werte as *const *const ListenKopf).read(), zahlen);
            assert_ne!(*(*original.runden).schluessel, *(*kopie.runden).schluessel);
            assert_eq!(((*kopie.name).zaehler, (*kopie.sachen).zaehler, (*kopie.runden).zaehler, (*zahlen).zaehler), (1, 1, 1, 1));
            assert_eq!((*(*(*kopie.runden).schluessel as *const TextKopf)).zaehler, 1);

            // Das Original ändern oder freigeben stört die Kopie nicht.
            (*original.name).zaehler = 0;
            oida_text_frei(original.name);
            ((*original.runden).werte as *mut *mut ListenKopf).read().as_mut().unwrap().laenge = 0;
            assert_eq!(inhalt(kopie.name), "Hansi");
            assert_eq!((*zahlen).laenge, 3);
        }
    }

    #[test]
    fn feste_texte_und_nix_werden_nicht_kopiert() {
        unsafe {
            // Ein fester Text aus dem Programm hat den Zähler -1: Den teilen sich alle.
            let fest = neuer_text(b"fix");
            (*fest).zaehler = -1;
            let mut ziel: *mut TextKopf = std::ptr::null_mut();
            kopiere((&fest as *const *mut TextKopf).cast(), (&mut ziel as *mut *mut TextKopf).cast(), einfach(ART_KETTN), 0);
            assert_eq!(ziel, fest);

            // NIX bleibt NIX, bei beiden Arten von VIELLEICHT.
            let vielleicht_text = typ(ART_VIELLEICHT_ZEIGER, 8, 0, einfach(ART_KETTN), Vec::new());
            let nix: *mut TextKopf = std::ptr::null_mut();
            let mut ziel = fest;
            kopiere((&nix as *const *mut TextKopf).cast(), (&mut ziel as *mut *mut TextKopf).cast(), vielleicht_text, 0);
            assert!(ziel.is_null());
            let da = neuer_text(b"do");
            kopiere((&da as *const *mut TextKopf).cast(), (&mut ziel as *mut *mut TextKopf).cast(), vielleicht_text, 0);
            assert!(ziel != da && inhalt(ziel) == "do");

            let vielleicht_zoi = typ(ART_VIELLEICHT_WERT, 16, 8, einfach(ART_ZOI), Vec::new());
            let (quelle, mut ziel) = ([0u64, 99], [7u64, 7]);
            kopiere(quelle.as_ptr().cast(), ziel.as_mut_ptr().cast(), vielleicht_zoi, 0);
            assert_eq!(ziel, [0, 0]);
            kopiere([1u64, 99].as_ptr().cast(), ziel.as_mut_ptr().cast(), vielleicht_zoi, 0);
            assert_eq!(ziel, [1, 99]);
        }
    }

    #[test]
    fn verzeichnis_mit_zahlen_als_schluessel() {
        unsafe {
            let alt = oida_wb_neu(0);
            for (schluessel, wert) in [(40i64, 4.5f64), (-3, 0.25)] {
                let stelle = oida_wb_dazu(alt, schluessel, 8) as usize;
                ((*alt).werte as *mut f64).add(stelle).write(wert);
            }
            let mut neu: *mut WbKopf = std::ptr::null_mut();
            let beschreibung = typ(ART_VAZEICHNIS, 8, 0, einfach(ART_KOMMAZOI), Vec::new());
            kopiere((&alt as *const *mut WbKopf).cast(), (&mut neu as *mut *mut WbKopf).cast(), beschreibung, 0);
            assert!(neu != alt && (*neu).laenge == 2 && !(*neu).text);
            let stelle = oida_wb_finde(neu, -3) as usize;
            assert_eq!(((*neu).werte as *const f64).add(stelle).read(), 0.25);
            (*alt).zaehler = 0;
            oida_wb_frei(alt, 8);
            assert_eq!(oida_wb_finde(neu, 40), 0);
        }
    }

    unsafe fn kanal_in(nummer: &i64) -> *const i64 {
        nummer
    }

    #[test]
    fn kanal_gibt_in_der_reihenfolge_heraus_und_kopiert_beim_schicken() {
        unsafe {
            let zoi = einfach(ART_ZOI);
            let texte = typ(ART_LISTN, 8, 0, einfach(ART_KETTN), Vec::new());
            let nummer = neuer_kanal();
            let k = kanal_in(&nummer);

            let liste = oida_liste_neu(8, 1);
            ((*liste).daten as *mut *mut TextKopf).write(neuer_text(b"ans"));
            (*liste).laenge = 1;
            oida_kanal_schicken(k, zoi, (&liste as *const *mut ListenKopf).cast(), texte);
            // Nach dem Schicken gehört dem Absender sein Wert weiter; er kann ihn ändern und freigeben.
            (*liste).laenge = 0;
            (*liste).zaehler = 0;
            oida_liste_frei(liste, 8);
            assert_eq!(oida_kanal_anzahl(k, zoi), 1);

            let mut geholt: *mut ListenKopf = std::ptr::null_mut();
            oida_kanal_holen(k, zoi, (&mut geholt as *mut *mut ListenKopf).cast(), texte);
            assert!(geholt != liste && (*geholt).laenge == 1 && (*geholt).zaehler == 1);
            assert_eq!(inhalt(((*geholt).daten as *const *const TextKopf).read()), "ans");

            // Zahlen: der Reihe nach, und danach ist der Kanal leer.
            for zahl in [3i64, 1, 2] {
                oida_kanal_schicken(k, zoi, (&zahl as *const i64).cast(), zoi);
            }
            let vielleicht = typ(ART_VIELLEICHT_WERT, 16, 8, zoi, Vec::new());
            let mut aus = [9u64, 9];
            let mut reihe = Vec::new();
            for _ in 0..4 {
                oida_kanal_holen_bis(k, zoi, 0, aus.as_mut_ptr().cast(), vielleicht);
                reihe.push(aus);
            }
            assert_eq!(reihe, [[1, 3], [1, 1], [1, 2], [0, 0]]);

            // Zu: Was drin ist, kommt noch heraus, dann NIX (auch ohne Warten).
            oida_kanal_schicken(k, zoi, (&5i64 as *const i64).cast(), zoi);
            assert!(!oida_kanal_ist_zu(k, zoi));
            oida_kanal_zu(k, zoi);
            assert!(oida_kanal_ist_zu(k, zoi));
            oida_kanal_holen_vielleicht(k, zoi, aus.as_mut_ptr().cast(), vielleicht);
            assert_eq!(aus, [1, 5]);
            oida_kanal_holen_vielleicht(k, zoi, aus.as_mut_ptr().cast(), vielleicht);
            assert_eq!(aus, [0, 0]);

            // Ein VIELLEICHT um einen Zeiger: der Zeiger selbst, oder null.
            let vielleicht_text = typ(ART_VIELLEICHT_ZEIGER, 8, 0, einfach(ART_KETTN), Vec::new());
            let nummer = neuer_kanal();
            let k = kanal_in(&nummer);
            let text = neuer_text(b"servas");
            oida_kanal_schicken(k, zoi, (&text as *const *mut TextKopf).cast(), einfach(ART_KETTN));
            let mut geholt: *mut TextKopf = std::ptr::null_mut();
            oida_kanal_holen_bis(k, zoi, 0, (&mut geholt as *mut *mut TextKopf).cast(), vielleicht_text);
            assert!(geholt != text && inhalt(geholt) == "servas");
            oida_kanal_holen_bis(k, zoi, 0, (&mut geholt as *mut *mut TextKopf).cast(), vielleicht_text);
            assert!(geholt.is_null());
        }
    }

    #[test]
    fn warten_mit_frist_hoert_von_selbst_auf_und_merkt_was_inzwischen_kommt() {
        unsafe {
            let zoi = einfach(ART_ZOI);
            let vielleicht = typ(ART_VIELLEICHT_WERT, 16, 8, zoi, Vec::new());
            let nummer = neuer_kanal();
            let mut aus = [9u64, 9];
            let anfang = Instant::now();
            oida_kanal_holen_bis(kanal_in(&nummer), zoi, 60, aus.as_mut_ptr().cast(), vielleicht);
            assert_eq!(aus, [0, 0]);
            assert!(anfang.elapsed() >= Duration::from_millis(55), "{:?}", anfang.elapsed());

            // Ein anderer Thread schickt nach kurzer Zeit: Der Wartende bekommt es gleich.
            let schicker = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(30));
                oida_kanal_schicken(kanal_in(&nummer), einfach(ART_ZOI), (&41i64 as *const i64).cast(), einfach(ART_ZOI));
            });
            let anfang = Instant::now();
            oida_kanal_holen_bis(kanal_in(&nummer), zoi, 20_000, aus.as_mut_ptr().cast(), vielleicht);
            assert_eq!(aus, [1, 41]);
            assert!(anfang.elapsed() < Duration::from_secs(10));
            schicker.join().unwrap();
        }
    }

    /// Was der Compiler für `NEBNBEI summe(liste, dazu)` bauen würde: nimmt eine Liste von
    /// Zahlen und eine Zahl aus der Mitgabe, zählt alles zusammen, gibt die Liste frei.
    unsafe extern "C" fn summe(mitgabe: *mut u8, ergebnis: *mut u8) {
        unsafe {
            let liste = (mitgabe as *const *mut ListenKopf).read();
            let dazu = (mitgabe.add(8) as *const i64).read();
            let zahlen = std::slice::from_raw_parts((*liste).daten as *const i64, (*liste).laenge as usize);
            (ergebnis as *mut i64).write(zahlen.iter().sum::<i64>() + dazu);
            (*liste).zaehler = 0;
            oida_liste_frei(liste, 8);
        }
    }

    #[test]
    fn eine_aufgabe_rechnet_mit_ihrer_kopie_und_gibt_das_ergebnis_einmal_her() {
        unsafe {
            let zoi = einfach(ART_ZOI);
            let zahlen = typ(ART_LISTN, 8, 0, zoi, Vec::new());
            let mitgabe_typ = typ(ART_STRUKTUR, 16, 2, std::ptr::null(), vec![feld(0, zahlen), feld(8, zoi)]);
            let liste = oida_liste_neu(8, 3);
            std::ptr::copy_nonoverlapping([10i64, 20, 30].as_ptr(), (*liste).daten as *mut i64, 3);
            (*liste).laenge = 3;
            let mitgabe: (*mut ListenKopf, i64) = (liste, 5);

            let nummer = oida_nebnbei_start(summe, (&mitgabe as *const (*mut ListenKopf, i64)).cast(), mitgabe_typ, 8);
            // Dem Aufrufer gehört seine Liste weiter.
            assert_eq!(((*liste).zaehler, (*liste).laenge), (1, 3));
            let mut ergebnis = 0i64;
            oida_nebnbei_warten(nummer, (&mut ergebnis as *mut i64).cast(), 8, 1);
            assert_eq!(ergebnis, 65);
            assert!(oida_nebnbei_fertig(&nummer, zoi));
            // Der Platz ist danach leer: Ein zweites Warten würde das Programm mit einer Meldung beenden.
            assert!(aufgabe(nummer).is_none());
        }
    }
}
