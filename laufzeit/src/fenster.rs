//! Fenster, Zeichnen und Eingabe (für `BRAUCH fensta OIDA`), direkt über Windows.
//!
//! Gezeichnet wird mit Direct2D, Text mit DirectWrite. Beides sind
//! COM-Schnittstellen: Ein Objekt ist ein Zeiger auf einen Zeiger auf eine
//! Tabelle von Funktionen, und jede Funktion bekommt das Objekt als ersten
//! Wert. Die Stellen in den Tabellen stehen unten als Konstanten; sie
//! stammen aus `d2d1.h` und `dwrite.h` und ändern sich nie.
//!
//! Die Bibliotheken von Windows (user32, gdi32, d2d1, dwrite) werden erst
//! geladen, wenn ein Programm wirklich ein Fenster aufmacht. Ein Programm
//! ohne Fenster startet so schnell wie vorher.
//!
//! # Zum Testen: Bilder statt Fenster
//!
//! Mit der Umgebungsvariable `OIDA_FENSTER_BILDER=<ordner>` geht kein Fenster
//! auf. Gezeichnet wird in ein Bild im Speicher, und jedes fertige Bild
//! landet als `bild_<nummer>.bmp` im Ordner. Die Zeit läuft dann in festen
//! Schritten von 1/60 Sekunde, damit immer dasselbe herauskommt.
//! `OIDA_FENSTER_ENDE=<n>` schließt das Fenster nach n Bildern von selbst.
//! `OIDA_FENSTER_BILD_JEDES=<n>` speichert nur jedes n-te Bild.
//! `OIDA_FENSTER_PRUEFEN=1` meldet am Ende, wie lang das Programm für ein Bild
//! gebraucht hat (Rechnen und Zeichnen, ohne das Warten auf den Bildschirm).
//! `OIDA_FENSTER_TASTEN=RECHTS@10-50,LAA@20` drückt Tasten nach Drehbuch:
//! RECHTS in den Bildern 10 bis 50, die Leertaste nur im Bild 20. So lässt
//! sich ein Spiel im Test spielen, ohne dass jemand an der Tastatur sitzt.
//! Auch die Maus geht: `MAUS(480;350)@30` stellt sie hin, `KLICK@31` drückt
//! links (`KLICK2` rechts), `RADL_OBE@40-42` und `RADL_AUFFE@50` drehen am Rad.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Steht am Anfang jeder Funktion, die etwas zeichnet: Soll das Gezeichnete
/// leuchten, ruft sie sich einmal selbst auf und zeichnet dabei in ein
/// eigenes Bild, aus dem der Schein wird (siehe `pinsel.rs`). Danach geht
/// es gewöhnlich weiter.
macro_rules! leuchtend {
    ($f:ident, $selbst:expr) => {
        let _uhr = Stoppuhr::neu(stringify!($selbst));
        if unsafe { pinsel::leuchten_anfang($f) } {
            $selbst;
            unsafe { pinsel::leuchten_ende(zeichnen()) };
        }
        let $f = zeichnen();
    };
}

mod bilder;
mod formen;
mod pinsel;

/// Für `OIDA_FENSTER_PRUEFEN`: Bilder, und wie lang das Programm für sie
/// gearbeitet hat (Summe und längstes, in Nanosekunden), dazu die Zeit fürs Anzeigen.
pub(crate) static GEZAEHLTE_BILDER: AtomicU64 = AtomicU64::new(0);
pub(crate) static ARBEIT_SUMME: AtomicU64 = AtomicU64::new(0);
pub(crate) static ARBEIT_LAENGSTE: AtomicU64 = AtomicU64::new(0);
pub(crate) static ARBEIT_UEBER: AtomicU64 = AtomicU64::new(0);
pub(crate) static ANZEIGEN_SUMME: AtomicU64 = AtomicU64::new(0);

/// Für `OIDA_FENSTER_PRUEFEN=2`: wie oft jede Zeichenfunktion lief und wie lang
/// (in Nanosekunden). 0: noch nicht nachgeschaut, 1: aus, 2: an.
static UHREN_AN: AtomicU64 = AtomicU64::new(0);
static UHREN: std::sync::Mutex<Vec<(&'static str, u64, u64)>> = std::sync::Mutex::new(Vec::new());

/// Misst, wie lang eine Zeichenfunktion braucht (nur wenn danach gefragt ist).
struct Stoppuhr(&'static str, Option<Instant>);

impl Stoppuhr {
    fn neu(aufruf: &'static str) -> Stoppuhr {
        let mut an = UHREN_AN.load(Ordering::Relaxed);
        if an == 0 {
            an = if std::env::var_os("OIDA_FENSTER_PRUEFEN").is_some_and(|w| w == "2") { 2 } else { 1 };
            UHREN_AN.store(an, Ordering::Relaxed);
        }
        Stoppuhr(aufruf, if an == 2 { Some(Instant::now()) } else { None })
    }
}

impl Drop for Stoppuhr {
    fn drop(&mut self) {
        let Some(anfang) = self.1 else { return };
        let nanos = anfang.elapsed().as_nanos() as u64;
        // Vom Aufruf, wie er im Quelltext steht, bleibt der Name der Funktion.
        let name = self.0.trim_start_matches("unsafe").trim_start_matches([' ', '{']);
        let name = name.split('(').next().unwrap_or(name).trim();
        let mut uhren = UHREN.lock().unwrap();
        match uhren.iter_mut().find(|u| u.0 == name) {
            Some(uhr) => {
                uhr.1 += 1;
                uhr.2 += nanos;
            }
            None => uhren.push((name, 1, nanos)),
        }
    }
}

/// Der Bericht zu den Stoppuhren: die Funktionen nach ihrer Zeit, pro Bild gerechnet.
pub(crate) fn uhren_bericht(bilder: u64) -> String {
    let mut uhren = UHREN.lock().unwrap().clone();
    uhren.sort_by(|a, b| b.2.cmp(&a.2));
    let bilder = bilder.max(1) as f64;
    uhren
        .iter()
        .map(|(name, mal, nanos)| format!("  {:<28} {:>8.1} moi im Büd, {:>7.3} ms", name, *mal as f64 / bilder, *nanos as f64 / 1e6 / bilder))
        .collect::<Vec<_>>()
        .join("\n")
}

use super::{ListenKopf, TextKopf, bytes};

type Zeiger = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct Guid(u32, u16, u16, [u8; 8]);

const IID_D2D_FABRIK: Guid = Guid(0x06152247, 0x6f50, 0x465a, [0x92, 0x45, 0x11, 0x8b, 0xfd, 0x3b, 0x60, 0x07]);
const IID_DWRITE_FABRIK: Guid = Guid(0xb859ee5a, 0xd838, 0x4b5b, [0xa2, 0xe8, 0x1a, 0xdc, 0x7d, 0x93, 0xdb, 0x48]);

// Stellen in den Funktionstabellen (ab 0; 0 bis 2 sind bei jedem COM-Objekt
// QueryInterface, AddRef und Release).
const FREIGEBEN: usize = 2;
// ID2D1Factory
const FABRIK_PFAD: usize = 10; // CreatePathGeometry
const FABRIK_STRICH: usize = 11; // CreateStrokeStyle
const FABRIK_FENSTER_ZIEL: usize = 14; // CreateHwndRenderTarget
const FABRIK_DC_ZIEL: usize = 16; // CreateDCRenderTarget
// ID2D1RenderTarget
const ZIEL_PINSEL: usize = 8; // CreateSolidColorBrush
const ZIEL_NEBENZIEL: usize = 12; // CreateCompatibleRenderTarget
const ZIEL_LINIE: usize = 15; // DrawLine
const ZIEL_RECHTECK_RAND: usize = 16; // DrawRectangle
const ZIEL_RECHTECK: usize = 17; // FillRectangle
const ZIEL_RUND_RAND: usize = 18; // DrawRoundedRectangle
const ZIEL_RUND: usize = 19; // FillRoundedRectangle
const ZIEL_ELLIPSE_RAND: usize = 20; // DrawEllipse
const ZIEL_ELLIPSE: usize = 21; // FillEllipse
const ZIEL_FORM_RAND: usize = 22; // DrawGeometry
const ZIEL_FORM: usize = 23; // FillGeometry
const ZIEL_BILD: usize = 26; // DrawBitmap
const ZIEL_TEXT: usize = 27; // DrawText
const ZIEL_SATZ: usize = 28; // DrawTextLayout
const ZIEL_LAGE: usize = 30; // SetTransform
const ZIEL_LEEREN: usize = 47; // Clear
const ZIEL_ANFANG: usize = 48; // BeginDraw
const ZIEL_ENDE: usize = 49; // EndDraw
const NEBENZIEL_BILD: usize = 57; // ID2D1BitmapRenderTarget::GetBitmap
// ID2D1HwndRenderTarget / ID2D1DCRenderTarget
const FENSTER_ZIEL_GROESSE: usize = 58; // Resize
const DC_ZIEL_BINDEN: usize = 57; // BindDC
// ID2D1SolidColorBrush
const PINSEL_FARBE: usize = 8; // SetColor
// ID2D1PathGeometry, ID2D1GeometrySink
const PFAD_OEFFNEN: usize = 17; // Open
const SENKE_ANFANG: usize = 5; // BeginFigure
const SENKE_LINIEN: usize = 6; // AddLines
const SENKE_ENDE: usize = 8; // EndFigure
const SENKE_ZU: usize = 9; // Close
// IDWriteFactory, IDWriteTextFormat
const SCHRIFT_FORMAT: usize = 15; // CreateTextFormat
const FORMAT_QUER: usize = 3; // SetTextAlignment
const FORMAT_HOCH: usize = 4; // SetParagraphAlignment
const FORMAT_UMBRUCH: usize = 5; // SetWordWrapping

/// Die Funktion an einer Stelle in der Tabelle eines COM-Objekts.
///
/// # Safety
/// `objekt` muss ein gültiges COM-Objekt sein, und `F` muss zur Funktion an dieser Stelle passen.
unsafe fn methode<F: Copy>(objekt: Zeiger, stelle: usize) -> F {
    unsafe {
        let tabelle = *(objekt as *const *const Zeiger);
        std::mem::transmute_copy::<Zeiger, F>(&*tabelle.add(stelle))
    }
}

/// Gibt ein COM-Objekt frei.
unsafe fn freigeben(objekt: Zeiger) {
    if !objekt.is_null() {
        unsafe {
            let f: unsafe extern "system" fn(Zeiger) -> u32 = methode(objekt, FREIGEBEN);
            f(objekt);
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Debug)]
struct Punkt {
    x: f32,
    y: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Rechteck {
    links: f32,
    oben: f32,
    rechts: f32,
    unten: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Farbe {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

/// Wie Punkte verschoben, gedreht und vergrößert werden (eine 3x2-Matrix wie in Direct2D).
#[repr(C)]
#[derive(Clone, Copy)]
struct Lage {
    m11: f32,
    m12: f32,
    m21: f32,
    m22: f32,
    dx: f32,
    dy: f32,
}

impl Lage {
    const GERADE: Lage = Lage { m11: 1.0, m12: 0.0, m21: 0.0, m22: 1.0, dx: 0.0, dy: 0.0 };

    /// Erst `self`, dann `danach`.
    fn dann(self, danach: Lage) -> Lage {
        Lage {
            m11: self.m11 * danach.m11 + self.m12 * danach.m21,
            m12: self.m11 * danach.m12 + self.m12 * danach.m22,
            m21: self.m21 * danach.m11 + self.m22 * danach.m21,
            m22: self.m21 * danach.m12 + self.m22 * danach.m22,
            dx: self.dx * danach.m11 + self.dy * danach.m21 + danach.dx,
            dy: self.dx * danach.m12 + self.dy * danach.m22 + danach.dy,
        }
    }
}

#[repr(C)]
struct ZielAngaben {
    art: u32,
    format: u32,
    alpha: u32,
    dpi_x: f32,
    dpi_y: f32,
    verwendung: u32,
    mindest: u32,
}

#[repr(C)]
struct FensterZielAngaben {
    fenster: Zeiger,
    breite: u32,
    hoehe: u32,
    anzeigen: u32,
}

#[repr(C)]
struct WinRechteck {
    links: i32,
    oben: i32,
    rechts: i32,
    unten: i32,
}

#[repr(C)]
struct FensterKlasse {
    groesse: u32,
    stil: u32,
    funktion: unsafe extern "system" fn(Zeiger, u32, usize, isize) -> isize,
    extra_klasse: i32,
    extra_fenster: i32,
    programm: Zeiger,
    bild: Zeiger,
    zeiger: Zeiger,
    hintergrund: Zeiger,
    menue: *const u16,
    name: *const u16,
    bild_klein: Zeiger,
}

/// Eine Nachricht von Windows an das Fenster (etwas größer angelegt als nötig).
#[repr(C)]
struct Nachricht {
    fenster: Zeiger,
    art: u32,
    w: usize,
    l: isize,
    rest: [u64; 4],
}

#[repr(C)]
struct BildKopf {
    groesse: u32,
    breite: i32,
    hoehe: i32,
    ebenen: u16,
    bits: u16,
    packung: u32,
    bild_groesse: u32,
    rest: [u32; 4],
}

unsafe extern "system" {
    fn LoadLibraryW(name: *const u16) -> Zeiger;
    fn GetProcAddress(bibliothek: Zeiger, name: *const u8) -> Zeiger;
    fn GetModuleHandleW(name: *const u16) -> Zeiger;
}

/// Ein Text für Windows: UTF-16 mit einer 0 am Ende.
fn breit(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Sucht eine Funktion in einer Bibliothek von Windows.
///
/// # Safety
/// `F` muss zur Funktion passen.
unsafe fn hole<F: Copy>(bibliothek: &str, name: &str) -> F {
    unsafe {
        let b = LoadLibraryW(breit(bibliothek).as_ptr());
        let mut name_c = name.as_bytes().to_vec();
        name_c.push(0);
        let funktion = if b.is_null() { std::ptr::null_mut() } else { GetProcAddress(b, name_c.as_ptr()) };
        if funktion.is_null() {
            stopp(&format!("Windows hot {name} in {bibliothek} ned. Do kaun i ka Fensta aufmochn."));
        }
        std::mem::transmute_copy::<Zeiger, F>(&funktion)
    }
}

/// Bricht das Programm mit einer Meldung ab.
fn stopp(meldung: &str) -> ! {
    super::leeren();
    eprintln!("Heast! {meldung}");
    std::process::exit(1);
}

type FensterProc = unsafe extern "system" fn(Zeiger, u32, usize, isize) -> isize;

/// Was vom Fenster zwischen den Aufrufen bleibt.
struct Fenster {
    /// Das Fenster von Windows (null, wenn nur in Bilder gezeichnet wird).
    fenster: Zeiger,
    fabrik: Zeiger,
    ziel: Zeiger,
    pinsel: Zeiger,
    schrift_fabrik: Zeiger,
    /// Die Schriften, die schon angelegt sind: (Größe mal 4, fett) -> Format.
    schriften: HashMap<(u32, bool), Zeiger>,
    /// Texte mit bunten Zeichen, fertig gezeichnet als Bild (siehe [`oida_g_text`]).
    text_bilder: HashMap<TextSchluessel, TextBild>,
    /// Fertig gesetzte Texte (siehe `Satz`).
    saetze: HashMap<SatzSchluessel, Satz>,
    /// Der Stil für durchgezogene Linien mit runden Enden; wird beim ersten Mal angelegt.
    rund_stil: Zeiger,
    schrift: (u32, bool),
    breite: i32,
    hoehe: i32,
    farbe: Farbe,
    deckung: f32,
    lage: Lage,
    gemerkt: Vec<Lage>,
    /// Gestrichelte Ränder und Linien: Länge von Strich und Lücke (0, 0: durchgezogen).
    strich: (f32, f32),
    /// Linien und Ränder haben runde Enden und Ecken.
    runde_enden: bool,
    /// Die Farben der zuletzt gebrauchten Verläufe.
    stufenlager: pinsel::Stufenlager,
    /// Zwischen `fensta_offn()` und dem nächsten `fensta_offn()` wird gezeichnet.
    im_bild: bool,
    zu: bool,
    bilder: u64,
    ende_nach: Option<u64>,
    tasten: [bool; 256],
    tasten_neu: [bool; 256],
    maus: (f32, f32),
    maus_tasten: [bool; 4],
    maus_neu: [bool; 4],
    /// Das Mausrad: wie weit in diesem Bild gedreht wurde, und was für das nächste gesammelt wird.
    rad: f32,
    rad_neu: f32,
    /// Ein Farbverlauf statt der einen Farbe (siehe `pinsel.rs`).
    verlauf: Option<pinsel::Verlauf>,
    /// ID2D1DeviceContext zur Zeichenfläche (für Mischen und Leuchten), oder null.
    kontext: Zeiger,
    /// Additiv mischen: Farben werden zusammengezählt statt übereinandergelegt.
    dazu: bool,
    leuchten: Option<pinsel::Leuchten>,
    /// Das Bild, in dem die Form für den Schein entsteht, seine Größe, der Effekt,
    /// und wohin es zurückgeht, während dort gezeichnet wird.
    leucht_ziel: Zeiger,
    leucht_groesse: (i32, i32),
    leucht_effekt: Zeiger,
    leucht_haupt: Zeiger,
    im_leuchten: bool,
    /// Die freie Form, die gerade gebaut wird, und die offenen Ausschnitte.
    pfad: formen::Pfad,
    ausschnitte: Vec<formen::Ausschnitt>,
    /// So viele Ausschnitte gehören zur Fläche darunter (wenn gerade in ein Bild gezeichnet wird).
    ausschnitte_unten: usize,
    /// Die Bilder des Programms; ihre Stelle hier ist ihre Nummer.
    bilder_liste: Vec<Option<bilder::Bild>>,
    wic: Zeiger,
    bild_glatt: bool,
    /// Wird gerade in ein Bild gezeichnet: welches, und was vom Fenster so lange aufgehoben ist.
    bild_ziel: Option<usize>,
    bild_haupt: Zeiger,
    bild_lage: Lage,
    bild_gemerkt: Vec<Lage>,
    anfang: Instant,
    letztes: Instant,
    /// Wann `fensta_offn()` zuletzt zum Programm zurückgekehrt ist.
    zurueck: Instant,
    bild_zeit: f64,
    zeit: f64,
    /// Nur beim Zeichnen in Bilder: Ordner, Speicher-DC, Bild von Windows und seine Punkte.
    bilder_ordner: Option<String>,
    bild_jedes: u64,
    /// Tasten nach Drehbuch: Nummer der Taste, erstes und letztes Bild.
    drehbuch: Vec<(Eingabe, u64, u64)>,
    dc: Zeiger,
    bild: Zeiger,
    punkte: *mut u8,
    // Funktionen von Windows, die in jedem Bild gebraucht werden.
    hol_nachricht: unsafe extern "system" fn(*mut Nachricht, Zeiger, u32, u32, u32) -> i32,
    uebersetz_nachricht: unsafe extern "system" fn(*const Nachricht) -> i32,
    verteil_nachricht: unsafe extern "system" fn(*const Nachricht) -> isize,
    standard: FensterProc,
}

static mut FENSTER: *mut Fenster = std::ptr::null_mut();

/// Das Fenster. Bricht ab, wenn noch keines aufgemacht wurde.
fn fenster<'a>() -> &'a mut Fenster {
    // SAFETY: OIDA-Programme haben einen Thread; der Zeiger wird nur in `oida_g_auf` gesetzt.
    unsafe {
        if FENSTER.is_null() {
            stopp("Es gibt no ka Fensta. Ruaf z'erst fensta_auf(...) auf.");
        }
        &mut *FENSTER
    }
}

/// Wie [`fenster`], aber nur, während ein Bild gezeichnet wird.
fn zeichnen<'a>() -> &'a mut Fenster {
    let f = fenster();
    if !f.im_bild {
        stopp("Zeichnen geht nur in da Schleifn: SOLONG fensta_offn() TUA ... PASST SCHO");
    }
    f
}

/// Die Nachrichten von Windows an unser Fenster.
unsafe extern "system" fn fenster_funktion(hwnd: Zeiger, art: u32, w: usize, l: isize) -> isize {
    unsafe {
        if FENSTER.is_null() {
            // Noch beim Aufmachen: Die eigentliche Funktion kennen wir erst danach.
            let standard: FensterProc = hole("user32.dll", "DefWindowProcW");
            return standard(hwnd, art, w, l);
        }
        let f = &mut *FENSTER;
        let maus_x = (l & 0xffff) as i16 as f32;
        let maus_y = ((l >> 16) & 0xffff) as i16 as f32;
        match art {
            // WM_CLOSE: Das Programm merkt es beim nächsten fensta_offn() und hört selbst auf.
            0x0010 => {
                f.zu = true;
                return 0;
            }
            // WM_KEYDOWN, WM_SYSKEYDOWN
            0x0100 | 0x0104 => {
                if w < 256 {
                    if !f.tasten[w] {
                        f.tasten_neu[w] = true;
                    }
                    f.tasten[w] = true;
                }
                if art == 0x0100 {
                    return 0;
                }
            }
            // WM_KEYUP, WM_SYSKEYUP
            0x0101 | 0x0105 => {
                if w < 256 {
                    f.tasten[w] = false;
                }
                if art == 0x0101 {
                    return 0;
                }
            }
            // WM_KILLFOCUS: Was gedrückt war, gilt nicht mehr (das Loslassen kommt woanders an).
            0x0008 => {
                f.tasten = [false; 256];
                f.maus_tasten = [false; 4];
            }
            // WM_MOUSEWHEEL: oben im w steht, wie weit (120 ist eine Raste; nach vorne ist positiv).
            // Gezählt wird wie im Browser: nach unten rollen ist positiv.
            0x020A => {
                f.rad_neu -= ((w >> 16) & 0xffff) as u16 as i16 as f32 / 120.0;
                return 0;
            }
            // WM_MOUSEMOVE
            0x0200 => f.maus = (maus_x, maus_y),
            // WM_LBUTTONDOWN, WM_RBUTTONDOWN, WM_MBUTTONDOWN
            0x0201 | 0x0204 | 0x0207 => {
                let knopf = [1, 2, 3][((art - 0x0201) / 3) as usize];
                f.maus = (maus_x, maus_y);
                f.maus_tasten[knopf] = true;
                f.maus_neu[knopf] = true;
            }
            // WM_LBUTTONUP, WM_RBUTTONUP, WM_MBUTTONUP
            0x0202 | 0x0205 | 0x0208 => {
                let knopf = [1, 2, 3][((art - 0x0202) / 3) as usize];
                f.maus_tasten[knopf] = false;
            }
            // WM_SIZE: Die Zeichenfläche wächst mit.
            0x0005 => {
                let (breite, hoehe) = ((l & 0xffff) as i32, ((l >> 16) & 0xffff) as i32);
                if breite > 0 && hoehe > 0 && !f.ziel.is_null() {
                    f.breite = breite;
                    f.hoehe = hoehe;
                    let groesse = [breite as u32, hoehe as u32];
                    let anpassen: unsafe extern "system" fn(Zeiger, *const [u32; 2]) -> i32 =
                        methode(f.ziel, FENSTER_ZIEL_GROESSE);
                    anpassen(f.ziel, &groesse);
                }
                return 0;
            }
            // WM_ERASEBKGND: Wir malen alles selbst.
            0x0014 => return 1,
            _ => {}
        }
        (f.standard)(hwnd, art, w, l)
    }
}

/// Legt die Zeichenfläche und den Pinsel an (am Anfang, und wenn Windows sie verloren hat).
unsafe fn ziel_anlegen(f: &mut Fenster) {
    unsafe {
        // Die fertigen Text-Bilder gehören zur alten Zeichenfläche, und alles andere auch.
        for (_, bild) in f.text_bilder.drain() {
            freigeben(bild.bild);
        }
        pinsel::vergiss(f);
        formen::vergiss(f);
        bilder::vergiss(f);
        freigeben(f.pinsel);
        freigeben(f.ziel);
        f.pinsel = std::ptr::null_mut();
        f.ziel = std::ptr::null_mut();
        // 96 Punkte pro Zoll: Eine Einheit ist genau ein Bildpunkt.
        let mut angaben =
            ZielAngaben { art: 0, format: 87, alpha: 3, dpi_x: 96.0, dpi_y: 96.0, verwendung: 0, mindest: 0 };
        let ergebnis = if f.fenster.is_null() {
            // Ins Bild im Speicher, mit dem Rechner von Windows (ohne Grafikkarte): überall dasselbe Ergebnis.
            angaben.art = 1;
            let anlegen: unsafe extern "system" fn(Zeiger, *const ZielAngaben, *mut Zeiger) -> i32 =
                methode(f.fabrik, FABRIK_DC_ZIEL);
            let mut ergebnis = anlegen(f.fabrik, &angaben, &mut f.ziel);
            if ergebnis >= 0 {
                let flaeche = WinRechteck { links: 0, oben: 0, rechts: f.breite, unten: f.hoehe };
                let binden: unsafe extern "system" fn(Zeiger, Zeiger, *const WinRechteck) -> i32 =
                    methode(f.ziel, DC_ZIEL_BINDEN);
                ergebnis = binden(f.ziel, f.dc, &flaeche);
            }
            ergebnis
        } else {
            let fuer_fenster =
                FensterZielAngaben { fenster: f.fenster, breite: f.breite as u32, hoehe: f.hoehe as u32, anzeigen: 0 };
            let anlegen: unsafe extern "system" fn(
                Zeiger,
                *const ZielAngaben,
                *const FensterZielAngaben,
                *mut Zeiger,
            ) -> i32 = methode(f.fabrik, FABRIK_FENSTER_ZIEL);
            anlegen(f.fabrik, &angaben, &fuer_fenster, &mut f.ziel)
        };
        if ergebnis < 0 || f.ziel.is_null() {
            stopp(&format!("Windows gibt ma ka Zeichenfläche (Direct2D sogt 0x{:08X}).", ergebnis as u32));
        }
        let pinsel: unsafe extern "system" fn(Zeiger, *const Farbe, Zeiger, *mut Zeiger) -> i32 =
            methode(f.ziel, ZIEL_PINSEL);
        let ergebnis = pinsel(f.ziel, &f.farbe, std::ptr::null_mut(), &mut f.pinsel);
        if ergebnis < 0 || f.pinsel.is_null() {
            stopp(&format!("Windows gibt ma kan Pinsl (Direct2D sogt 0x{:08X}).", ergebnis as u32));
        }
        f.kontext = pinsel::kontext_von(f.ziel);
        bilder::neu_anlegen(f);
    }
}

/// `fensta_auf(titel, breite, höhe)`: macht das Fenster auf.
///
/// # Safety
/// `titel` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_g_auf(titel: *const TextKopf, breite: i64, hoehe: i64) {
    unsafe {
        if !FENSTER.is_null() {
            stopp("Des Fensta is scho offn. Mehr ois ans gibt's ned.");
        }
        if !(16..=16384).contains(&breite) || !(16..=16384).contains(&hoehe) {
            stopp(&format!("A Fensta mit {breite} moi {hoehe} Punkte? Des geht ned."));
        }
        let titel = String::from_utf8_lossy(bytes(titel)).into_owned();
        let bilder_ordner = std::env::var("OIDA_FENSTER_BILDER").ok().filter(|o| !o.is_empty());
        let ende_nach = std::env::var("OIDA_FENSTER_ENDE").ok().and_then(|n| n.parse::<u64>().ok());

        let d2d: unsafe extern "system" fn(u32, *const Guid, Zeiger, *mut Zeiger) -> i32 =
            hole("d2d1.dll", "D2D1CreateFactory");
        let dwrite: unsafe extern "system" fn(u32, *const Guid, *mut Zeiger) -> i32 =
            hole("dwrite.dll", "DWriteCreateFactory");
        let (mut fabrik, mut schrift_fabrik) = (std::ptr::null_mut(), std::ptr::null_mut());
        if d2d(0, &IID_D2D_FABRIK, std::ptr::null_mut(), &mut fabrik) < 0 || fabrik.is_null() {
            stopp("Direct2D lasst si ned startn.");
        }
        if dwrite(0, &IID_DWRITE_FABRIK, &mut schrift_fabrik) < 0 || schrift_fabrik.is_null() {
            stopp("DirectWrite (fia de Schrift) lasst si ned startn.");
        }

        let jetzt = Instant::now();
        let mut f = Box::new(Fenster {
            fenster: std::ptr::null_mut(),
            fabrik,
            ziel: std::ptr::null_mut(),
            pinsel: std::ptr::null_mut(),
            schrift_fabrik,
            schriften: HashMap::new(),
            text_bilder: HashMap::new(),
            saetze: HashMap::new(),
            rund_stil: std::ptr::null_mut(),
            schrift: (16 * 4, false),
            breite: breite as i32,
            hoehe: hoehe as i32,
            farbe: Farbe { r: 1.0, g: 1.0, b: 1.0, a: 1.0 },
            deckung: 1.0,
            lage: Lage::GERADE,
            gemerkt: Vec::new(),
            strich: (0.0, 0.0),
            runde_enden: false,
            stufenlager: Default::default(),
            im_bild: false,
            zu: false,
            bilder: 0,
            ende_nach,
            tasten: [false; 256],
            tasten_neu: [false; 256],
            maus: (0.0, 0.0),
            maus_tasten: [false; 4],
            maus_neu: [false; 4],
            rad: 0.0,
            rad_neu: 0.0,
            verlauf: None,
            kontext: std::ptr::null_mut(),
            dazu: false,
            leuchten: None,
            leucht_ziel: std::ptr::null_mut(),
            leucht_groesse: (0, 0),
            leucht_effekt: std::ptr::null_mut(),
            leucht_haupt: std::ptr::null_mut(),
            im_leuchten: false,
            pfad: formen::Pfad::default(),
            ausschnitte: Vec::new(),
            ausschnitte_unten: 0,
            bilder_liste: Vec::new(),
            wic: std::ptr::null_mut(),
            bild_glatt: true,
            bild_ziel: None,
            bild_haupt: std::ptr::null_mut(),
            bild_lage: Lage::GERADE,
            bild_gemerkt: Vec::new(),
            anfang: jetzt,
            letztes: jetzt,
            zurueck: jetzt,
            bild_zeit: 0.0,
            zeit: 0.0,
            bilder_ordner,
            bild_jedes: std::env::var("OIDA_FENSTER_BILD_JEDES").ok().and_then(|n| n.parse().ok()).unwrap_or(1).max(1),
            drehbuch: drehbuch_lesen(&std::env::var("OIDA_FENSTER_TASTEN").unwrap_or_default()),
            dc: std::ptr::null_mut(),
            bild: std::ptr::null_mut(),
            punkte: std::ptr::null_mut(),
            hol_nachricht: hole("user32.dll", "PeekMessageW"),
            uebersetz_nachricht: hole("user32.dll", "TranslateMessage"),
            verteil_nachricht: hole("user32.dll", "DispatchMessageW"),
            standard: hole("user32.dll", "DefWindowProcW"),
        });

        if f.bilder_ordner.is_some() {
            // Kein Fenster: ein Bild im Speicher, in das Direct2D zeichnet.
            let neuer_dc: unsafe extern "system" fn(Zeiger) -> Zeiger = hole("gdi32.dll", "CreateCompatibleDC");
            let neues_bild: unsafe extern "system" fn(Zeiger, *const BildKopf, u32, *mut *mut u8, Zeiger, u32) -> Zeiger =
                hole("gdi32.dll", "CreateDIBSection");
            let waehlen: unsafe extern "system" fn(Zeiger, Zeiger) -> Zeiger = hole("gdi32.dll", "SelectObject");
            // Negative Höhe: Die erste Zeile im Speicher ist die oberste im Bild.
            let kopf = BildKopf {
                groesse: 40,
                breite: f.breite,
                hoehe: -f.hoehe,
                ebenen: 1,
                bits: 32,
                packung: 0,
                bild_groesse: 0,
                rest: [0; 4],
            };
            f.dc = neuer_dc(std::ptr::null_mut());
            f.bild = neues_bild(f.dc, &kopf, 0, &mut f.punkte, std::ptr::null_mut(), 0);
            if f.dc.is_null() || f.bild.is_null() || f.punkte.is_null() {
                stopp("Windows gibt ma ka Büd im Speicha.");
            }
            waehlen(f.dc, f.bild);
        } else {
            // Ein Bildpunkt soll ein Bildpunkt sein, auch wenn Windows alles vergrößert darstellt.
            let scharf: unsafe extern "system" fn() -> i32 = hole("user32.dll", "SetProcessDPIAware");
            scharf();
            let klasse_anmelden: unsafe extern "system" fn(*const FensterKlasse) -> u16 =
                hole("user32.dll", "RegisterClassExW");
            let fenster_anlegen: unsafe extern "system" fn(
                u32,
                *const u16,
                *const u16,
                u32,
                i32,
                i32,
                i32,
                i32,
                Zeiger,
                Zeiger,
                Zeiger,
                Zeiger,
            ) -> Zeiger = hole("user32.dll", "CreateWindowExW");
            let rahmen: unsafe extern "system" fn(*mut WinRechteck, u32, i32) -> i32 =
                hole("user32.dll", "AdjustWindowRect");
            let lade_zeiger: unsafe extern "system" fn(Zeiger, *const u16) -> Zeiger = hole("user32.dll", "LoadCursorW");
            let zeigen: unsafe extern "system" fn(Zeiger, i32) -> i32 = hole("user32.dll", "ShowWindow");
            let bildschirm: unsafe extern "system" fn(i32) -> i32 = hole("user32.dll", "GetSystemMetrics");

            let programm = GetModuleHandleW(std::ptr::null());
            let name = breit("OidaFensta");
            let klasse = FensterKlasse {
                groesse: std::mem::size_of::<FensterKlasse>() as u32,
                stil: 0x0003, // CS_HREDRAW | CS_VREDRAW
                funktion: fenster_funktion,
                extra_klasse: 0,
                extra_fenster: 0,
                programm,
                bild: std::ptr::null_mut(),
                zeiger: lade_zeiger(std::ptr::null_mut(), 32512 as *const u16), // der normale Pfeil
                hintergrund: std::ptr::null_mut(),
                menue: std::ptr::null(),
                name: name.as_ptr(),
                bild_klein: std::ptr::null_mut(),
            };
            if klasse_anmelden(&klasse) == 0 {
                stopp("Windows wü des Fensta ned aumödn.");
            }
            // Ein gewöhnliches Fenster mit Titelleiste. Verlangt ist die Größe der Fläche innen.
            let stil = 0x00CF0000u32; // WS_OVERLAPPEDWINDOW
            let mut aussen = WinRechteck { links: 0, oben: 0, rechts: f.breite, unten: f.hoehe };
            rahmen(&mut aussen, stil, 0);
            let (b, h) = (aussen.rechts - aussen.links, aussen.unten - aussen.oben);
            // In die Mitte vom Bildschirm.
            let x = ((bildschirm(0) - b) / 2).max(0);
            let y = ((bildschirm(1) - h) / 2).max(0);
            f.fenster = fenster_anlegen(
                0,
                name.as_ptr(),
                breit(&titel).as_ptr(),
                stil,
                x,
                y,
                b,
                h,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                programm,
                std::ptr::null_mut(),
            );
            if f.fenster.is_null() {
                stopp("Windows mocht des Fensta ned auf.");
            }
            ziel_anlegen(&mut f);
            FENSTER = Box::into_raw(f);
            zeigen((*FENSTER).fenster, 5); // SW_SHOW
            return;
        }
        ziel_anlegen(&mut f);
        FENSTER = Box::into_raw(f);
    }
}

/// Schreibt das Bild im Speicher als BMP-Datei (nur beim Zeichnen in Bilder).
fn bild_speichern(f: &Fenster) {
    let Some(ordner) = &f.bilder_ordner else { return };
    let (breite, hoehe) = (f.breite as usize, f.hoehe as usize);
    let punkte = unsafe { std::slice::from_raw_parts(f.punkte, breite * hoehe * 4) };
    let mut datei = Vec::with_capacity(54 + punkte.len());
    datei.extend_from_slice(b"BM");
    datei.extend_from_slice(&((54 + punkte.len()) as u32).to_le_bytes());
    datei.extend_from_slice(&[0; 4]);
    datei.extend_from_slice(&54u32.to_le_bytes());
    datei.extend_from_slice(&40u32.to_le_bytes());
    datei.extend_from_slice(&(breite as i32).to_le_bytes());
    // Positive Höhe heißt bei BMP: Die unterste Zeile steht zuerst in der Datei.
    datei.extend_from_slice(&(hoehe as i32).to_le_bytes());
    datei.extend_from_slice(&1u16.to_le_bytes());
    datei.extend_from_slice(&32u16.to_le_bytes());
    datei.extend_from_slice(&[0; 24]);
    for zeile in (0..hoehe).rev() {
        datei.extend_from_slice(&punkte[zeile * breite * 4..(zeile + 1) * breite * 4]);
    }
    let pfad = format!("{ordner}/bild_{:03}.bmp", f.bilder);
    if let Err(fehler) = std::fs::write(&pfad, datei) {
        stopp(&format!("I kaun {pfad} ned schreibn: {fehler}"));
    }
}

/// `fensta_offn()`: zeigt, was seit dem letzten Aufruf gezeichnet wurde,
/// schaut nach, was Tasten und Maus inzwischen gemacht haben, und beginnt ein
/// neues Bild. Gibt FOISCH zurück, sobald das Fenster zugemacht wurde.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_offen() -> bool {
    let f = fenster();
    unsafe {
        if f.im_bild {
            // Was das Programm offen gelassen hat, wird hier zugemacht.
            bilder::ins_fenster(f);
            formen::ausschnitte_zu_bis(f, 0);
            // So lang hat das Programm für dieses Bild gerechnet und gezeichnet.
            let eintritt = Instant::now();
            let arbeit = eintritt.duration_since(f.zurueck).as_nanos() as u64;
            GEZAEHLTE_BILDER.fetch_add(1, Ordering::Relaxed);
            ARBEIT_SUMME.fetch_add(arbeit, Ordering::Relaxed);
            ARBEIT_LAENGSTE.fetch_max(arbeit, Ordering::Relaxed);
            if arbeit > 16_000_000 {
                ARBEIT_UEBER.fetch_add(1, Ordering::Relaxed);
            }
            let ende: unsafe extern "system" fn(Zeiger, *mut u64, *mut u64) -> i32 = methode(f.ziel, ZIEL_ENDE);
            let ergebnis = ende(f.ziel, std::ptr::null_mut(), std::ptr::null_mut());
            ANZEIGEN_SUMME.fetch_add(eintritt.elapsed().as_nanos() as u64, Ordering::Relaxed);
            f.im_bild = false;
            f.bilder += 1;
            if f.bilder_ordner.is_some() && f.bilder % f.bild_jedes == 0 {
                bild_speichern(f);
            }
            // D2DERR_RECREATE_TARGET: Windows hat die Zeichenfläche verloren (etwa nach dem Aufwachen).
            if ergebnis as u32 == 0x8899000C {
                ziel_anlegen(f);
            }
        }
        if f.ende_nach.is_some_and(|n| f.bilder >= n) {
            f.zu = true;
        }
        // "Neu gedrückt" gilt für genau ein Bild.
        f.tasten_neu = [false; 256];
        f.maus_neu = [false; 4];
        f.rad = std::mem::take(&mut f.rad_neu);
        if !f.fenster.is_null() {
            let mut nachricht: Nachricht = std::mem::zeroed();
            while (f.hol_nachricht)(&mut nachricht, std::ptr::null_mut(), 0, 0, 1) != 0 {
                (f.uebersetz_nachricht)(&nachricht);
                (f.verteil_nachricht)(&nachricht);
            }
        }
        // Tasten nach Drehbuch, für das Bild, das jetzt beginnt.
        let bild = f.bilder + 1;
        for i in 0..f.drehbuch.len() {
            let (eingabe, von, bis) = f.drehbuch[i];
            let unten = (von..=bis).contains(&bild);
            match eingabe {
                Eingabe::Taste(taste) => {
                    if unten && !f.tasten[taste] {
                        f.tasten_neu[taste] = true;
                    }
                    if unten || bild == bis + 1 {
                        f.tasten[taste] = unten;
                    }
                }
                Eingabe::Klick(knopf) => {
                    if unten && !f.maus_tasten[knopf] {
                        f.maus_neu[knopf] = true;
                    }
                    if unten || bild == bis + 1 {
                        f.maus_tasten[knopf] = unten;
                    }
                }
                Eingabe::Maus(x, y) if unten => f.maus = (x, y),
                Eingabe::Rad(rasten) if unten => f.rad += rasten,
                _ => {}
            }
        }
        if f.zu {
            return false;
        }
        // Die Zeit: beim Zeichnen in Bilder in festen Schritten, sonst wie sie wirklich vergeht.
        if f.bilder_ordner.is_some() {
            f.bild_zeit = if f.bilder == 0 { 0.0 } else { 1.0 / 60.0 };
            f.zeit = f.bilder as f64 / 60.0;
        } else {
            let jetzt = Instant::now();
            // Nach einer langen Pause (Fenster verschoben, Rechner hing) nicht alles auf einmal nachholen.
            f.bild_zeit = jetzt.duration_since(f.letztes).as_secs_f64().min(0.1);
            f.zeit = jetzt.duration_since(f.anfang).as_secs_f64();
            f.letztes = jetzt;
        }
        let anfang: unsafe extern "system" fn(Zeiger) = methode(f.ziel, ZIEL_ANFANG);
        anfang(f.ziel);
        f.im_bild = true;
        f.gemerkt.clear();
        f.lage = Lage::GERADE;
        lage_setzen(f);
        pinsel::neues_bild(f);
        f.zurueck = Instant::now();
    }
    true
}

/// `fensta_zua()`: Das nächste `fensta_offn()` gibt FOISCH zurück.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_zu() {
    fenster().zu = true;
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_g_breite() -> i64 {
    fenster().breite as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_g_hoehe() -> i64 {
    fenster().hoehe as i64
}

/// Sekunden, seit das Fenster aufgegangen ist.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_zeit() -> f64 {
    fenster().zeit
}

/// Sekunden, die das letzte Bild gedauert hat.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_bild_zeit() -> f64 {
    fenster().bild_zeit
}

// ------------------------------------------------------------ Farbe und Lage

/// Stellt den Pinsel auf die aktuelle Farbe und gibt ihn zurück.
unsafe fn pinsel(f: &mut Fenster) -> Zeiger {
    unsafe {
        if f.verlauf.is_some()
            && let Some(verlauf) = pinsel::verlauf_pinsel(f)
        {
            return verlauf;
        }
        let farbe = Farbe { a: f.farbe.a * f.deckung, ..f.farbe };
        let setzen: unsafe extern "system" fn(Zeiger, *const Farbe) = methode(f.pinsel, PINSEL_FARBE);
        setzen(f.pinsel, &farbe);
        f.pinsel
    }
}

unsafe fn lage_setzen(f: &mut Fenster) {
    unsafe {
        let setzen: unsafe extern "system" fn(Zeiger, *const Lage) = methode(f.ziel, ZIEL_LAGE);
        setzen(f.ziel, &f.lage);
    }
}

/// `foab(rot, grün, blau)`: jeweils 0 bis 255.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_farbe(rot: i64, gruen: i64, blau: i64) {
    let teil = |wert: i64| wert.clamp(0, 255) as f32 / 255.0;
    let f = fenster();
    f.farbe = Farbe { r: teil(rot), g: teil(gruen), b: teil(blau), a: 1.0 };
    pinsel::farbe_gesetzt(f);
}

/// `foab_hex("#ff8800")`: wie im Web, auch kurz (`#f80`) und mit Deckung (`#ff880080`).
///
/// # Safety
/// `code` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_g_farbe_hex(code: *const TextKopf) {
    let roh = unsafe { bytes(code) };
    let Some(farbe) = farbe_aus_hex(roh) else {
        stopp(&format!(
            "De Foab \"{}\" versteh i ned. So schaut ane aus: \"#ff8800\" oda \"#f80\".",
            String::from_utf8_lossy(roh)
        ));
    };
    let f = fenster();
    f.farbe = farbe;
    pinsel::farbe_gesetzt(f);
}

/// Liest eine Farbe wie im Web: `#rgb`, `#rgba`, `#rrggbb` oder `#rrggbbaa`.
fn farbe_aus_hex(roh: &[u8]) -> Option<Farbe> {
    let ziffern = roh.strip_prefix(b"#").unwrap_or(roh);
    let wert = |z: u8| (z as char).to_digit(16).map(|w| w as u8);
    let alle: Option<Vec<u8>> = ziffern.iter().map(|z| wert(*z)).collect();
    let teile: Vec<u8> = match (alle, ziffern.len()) {
        (Some(z), 3 | 4) => z.iter().map(|w| w * 17).collect(),
        (Some(z), 6 | 8) => z.chunks(2).map(|p| p[0] * 16 + p[1]).collect(),
        _ => return None,
    };
    let teil = |i: usize| teile.get(i).map_or(1.0, |w| *w as f32 / 255.0);
    Some(Farbe { r: teil(0), g: teil(1), b: teil(2), a: teil(3) })
}

/// `deckung(wert)`: 1 ist ganz deckend, 0 unsichtbar. Gilt für alles, was danach gezeichnet wird.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_deckung(wert: f64) {
    fenster().deckung = wert.clamp(0.0, 1.0) as f32;
}

/// `schiab(dx, dy)`: Alles, was danach gezeichnet wird, ist verschoben.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_schieb(dx: f64, dy: f64) {
    let f = zeichnen();
    f.lage = Lage { dx: dx as f32, dy: dy as f32, ..Lage::GERADE }.dann(f.lage);
    unsafe { lage_setzen(f) };
}

/// `drah(winkel)`: dreht um den Punkt (0, 0), im Uhrzeigersinn, Winkel im Bogenmaß.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_dreh(winkel: f64) {
    let f = zeichnen();
    let (s, c) = (winkel.sin() as f32, winkel.cos() as f32);
    f.lage = Lage { m11: c, m12: s, m21: -s, m22: c, dx: 0.0, dy: 0.0 }.dann(f.lage);
    unsafe { lage_setzen(f) };
}

/// `vergrößa(faktor)`: vergrößert vom Punkt (0, 0) aus.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_gross(faktor: f64) {
    let f = zeichnen();
    let k = faktor as f32;
    f.lage = Lage { m11: k, m12: 0.0, m21: 0.0, m22: k, dx: 0.0, dy: 0.0 }.dann(f.lage);
    unsafe { lage_setzen(f) };
}

/// `lage_merkn()`: merkt sich, wie gerade verschoben, gedreht und vergrößert ist.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_lage_merken() {
    let f = zeichnen();
    f.gemerkt.push(f.lage);
}

/// `lage_zruck()`: stellt wieder her, was das letzte `lage_merkn()` gemerkt hat.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_lage_zurueck() {
    let f = zeichnen();
    match f.gemerkt.pop() {
        Some(lage) => f.lage = lage,
        None => stopp("lage_zruck() ohne lage_merkn() davua."),
    }
    unsafe { lage_setzen(f) };
}

// ------------------------------------------------------------------ Zeichnen

/// `gstrichlt(strich, lückn)`: Ränder und Linien werden ab jetzt gestrichelt,
/// mit Strichen und Lücken dieser Länge. Mit (0, 0) wieder durchgezogen.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_strich(strich: f64, luecke: f64) {
    let gut = |w: f64| if w.is_finite() { w.clamp(0.0, 100_000.0) as f32 } else { 0.0 };
    fenster().strich = (gut(strich), gut(luecke));
}

/// `linien_rund(jo)`: Linien und Ränder bekommen ab jetzt runde Enden und Ecken (oder wieder eckige).
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_linien_rund(rund: bool) {
    fenster().runde_enden = rund;
}

/// Der Stil für einen Rand der Dicke `dicke`: null (durchgezogen, eckig) oder ein
/// eigener Stil (gestrichelt, runde Enden), den der Aufrufer nach dem Zeichnen wieder freigibt.
unsafe fn strich_stil(f: &mut Fenster, dicke: f32) -> Zeiger {
    let (strich, luecke) = f.strich;
    let gestrichelt = strich > 0.0 && luecke > 0.0 && dicke > 0.0;
    if !gestrichelt && !f.runde_enden {
        return std::ptr::null_mut();
    }
    // Durchgezogen und rund ist immer derselbe Stil: einmal anlegen, dann nur
    // noch mitzählen lassen (der Aufrufer gibt seinen Anteil wieder her).
    if !gestrichelt && !f.rund_stil.is_null() {
        unsafe {
            let mitzaehlen: unsafe extern "system" fn(Zeiger) -> u32 = methode(f.rund_stil, 1);
            mitzaehlen(f.rund_stil);
        }
        return f.rund_stil;
    }
    #[repr(C)]
    struct Angaben {
        anfang: u32,
        ende: u32,
        strich_ende: u32,
        ecke: u32,
        spitze: f32,
        muster: u32,
        versatz: f32,
    }
    // Enden: 0 gerade abgeschnitten, 2 rund. Ecken: 0 spitz, 2 rund.
    let rund = if f.runde_enden { 2 } else { 0 };
    // Muster 0: durchgezogen, 5: eigenes. Direct2D misst Strich und Lücke in Vielfachen der Dicke.
    let muster = if gestrichelt { 5 } else { 0 };
    let angaben = Angaben { anfang: rund, ende: rund, strich_ende: rund, ecke: rund, spitze: 10.0, muster, versatz: 0.0 };
    let laengen = [strich / dicke.max(0.001), luecke / dicke.max(0.001)];
    let (zeiger, anzahl) = if gestrichelt { (laengen.as_ptr(), 2) } else { (std::ptr::null(), 0) };
    unsafe {
        let anlegen: unsafe extern "system" fn(Zeiger, *const Angaben, *const f32, u32, *mut Zeiger) -> i32 =
            methode(f.fabrik, FABRIK_STRICH);
        let mut stil = std::ptr::null_mut();
        if anlegen(f.fabrik, &angaben, zeiger, anzahl, &mut stil) < 0 {
            return std::ptr::null_mut();
        }
        if !gestrichelt && !stil.is_null() {
            let mitzaehlen: unsafe extern "system" fn(Zeiger) -> u32 = methode(stil, 1);
            mitzaehlen(stil);
            f.rund_stil = stil;
        }
        stil
    }
}

/// `hintagrund()`: füllt das ganze Fenster mit der aktuellen Farbe.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_hintergrund() {
    let f = zeichnen();
    unsafe {
        let leeren: unsafe extern "system" fn(Zeiger, *const Farbe) = methode(f.ziel, ZIEL_LEEREN);
        leeren(f.ziel, &Farbe { a: 1.0, ..f.farbe });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_g_rechteck(x: f64, y: f64, breite: f64, hoehe: f64) {
    let f = zeichnen();
    leuchtend!(f, oida_g_rechteck(x, y, breite, hoehe));
    let flaeche = Rechteck { links: x as f32, oben: y as f32, rechts: (x + breite) as f32, unten: (y + hoehe) as f32 };
    unsafe {
        let p = pinsel(f);
        let fuellen: unsafe extern "system" fn(Zeiger, *const Rechteck, Zeiger) = methode(f.ziel, ZIEL_RECHTECK);
        fuellen(f.ziel, &flaeche, p);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_g_rechteck_rand(x: f64, y: f64, breite: f64, hoehe: f64, dicke: f64) {
    let f = zeichnen();
    leuchtend!(f, oida_g_rechteck_rand(x, y, breite, hoehe, dicke));
    let flaeche = Rechteck { links: x as f32, oben: y as f32, rechts: (x + breite) as f32, unten: (y + hoehe) as f32 };
    unsafe {
        let p = pinsel(f);
        let rand: unsafe extern "system" fn(Zeiger, *const Rechteck, Zeiger, f32, Zeiger) =
            methode(f.ziel, ZIEL_RECHTECK_RAND);
        let stil = strich_stil(f, dicke as f32);
        rand(f.ziel, &flaeche, p, dicke as f32, stil);
        freigeben(stil);
    }
}

#[repr(C)]
struct Rund {
    flaeche: Rechteck,
    radius_x: f32,
    radius_y: f32,
}

/// Der Rand von einem Rechteck mit runden Ecken.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_rechteck_rund_rand(x: f64, y: f64, breite: f64, hoehe: f64, radius: f64, dicke: f64) {
    let f = zeichnen();
    leuchtend!(f, oida_g_rechteck_rund_rand(x, y, breite, hoehe, radius, dicke));
    let rund = Rund {
        flaeche: Rechteck { links: x as f32, oben: y as f32, rechts: (x + breite) as f32, unten: (y + hoehe) as f32 },
        radius_x: radius as f32,
        radius_y: radius as f32,
    };
    unsafe {
        let p = pinsel(f);
        let rand: unsafe extern "system" fn(Zeiger, *const Rund, Zeiger, f32, Zeiger) = methode(f.ziel, ZIEL_RUND_RAND);
        let stil = strich_stil(f, dicke as f32);
        rand(f.ziel, &rund, p, dicke as f32, stil);
        freigeben(stil);
    }
}

/// Ein Rechteck mit runden Ecken.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_rechteck_rund(x: f64, y: f64, breite: f64, hoehe: f64, radius: f64) {
    let f = zeichnen();
    leuchtend!(f, oida_g_rechteck_rund(x, y, breite, hoehe, radius));
    let rund = Rund {
        flaeche: Rechteck { links: x as f32, oben: y as f32, rechts: (x + breite) as f32, unten: (y + hoehe) as f32 },
        radius_x: radius as f32,
        radius_y: radius as f32,
    };
    unsafe {
        let p = pinsel(f);
        let fuellen: unsafe extern "system" fn(Zeiger, *const Rund, Zeiger) = methode(f.ziel, ZIEL_RUND);
        fuellen(f.ziel, &rund, p);
    }
}

#[repr(C)]
struct Ellipse {
    mitte: Punkt,
    radius_x: f32,
    radius_y: f32,
}

/// Eine gefüllte Ellipse um den Punkt (x, y); mit gleichen Radien ein Kreis.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_ellipse(x: f64, y: f64, radius_x: f64, radius_y: f64) {
    let f = zeichnen();
    leuchtend!(f, oida_g_ellipse(x, y, radius_x, radius_y));
    let form = Ellipse { mitte: Punkt { x: x as f32, y: y as f32 }, radius_x: radius_x as f32, radius_y: radius_y as f32 };
    unsafe {
        let p = pinsel(f);
        let fuellen: unsafe extern "system" fn(Zeiger, *const Ellipse, Zeiger) = methode(f.ziel, ZIEL_ELLIPSE);
        fuellen(f.ziel, &form, p);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_g_kreis_rand(x: f64, y: f64, radius: f64, dicke: f64) {
    let f = zeichnen();
    leuchtend!(f, oida_g_kreis_rand(x, y, radius, dicke));
    let form = Ellipse { mitte: Punkt { x: x as f32, y: y as f32 }, radius_x: radius as f32, radius_y: radius as f32 };
    unsafe {
        let p = pinsel(f);
        let rand: unsafe extern "system" fn(Zeiger, *const Ellipse, Zeiger, f32, Zeiger) =
            methode(f.ziel, ZIEL_ELLIPSE_RAND);
        let stil = strich_stil(f, dicke as f32);
        rand(f.ziel, &form, p, dicke as f32, stil);
        freigeben(stil);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_g_linie(x1: f64, y1: f64, x2: f64, y2: f64, dicke: f64) {
    let f = zeichnen();
    leuchtend!(f, oida_g_linie(x1, y1, x2, y2, dicke));
    unsafe {
        let p = pinsel(f);
        let linie: unsafe extern "system" fn(Zeiger, Punkt, Punkt, Zeiger, f32, Zeiger) = methode(f.ziel, ZIEL_LINIE);
        let stil = strich_stil(f, dicke as f32);
        linie(f.ziel, Punkt { x: x1 as f32, y: y1 as f32 }, Punkt { x: x2 as f32, y: y2 as f32 }, p, dicke as f32, stil);
        freigeben(stil);
    }
}

/// Zeichnet einen Linienzug: gefüllt (dann ist er immer geschlossen) oder nur der Rand.
unsafe fn form(f: &mut Fenster, punkte: &[Punkt], fuellen: bool, geschlossen: bool, dicke: f32) {
    if punkte.len() < 2 {
        return;
    }
    unsafe {
        let neuer_pfad: unsafe extern "system" fn(Zeiger, *mut Zeiger) -> i32 = methode(f.fabrik, FABRIK_PFAD);
        let mut pfad = std::ptr::null_mut();
        if neuer_pfad(f.fabrik, &mut pfad) < 0 || pfad.is_null() {
            return;
        }
        let oeffnen: unsafe extern "system" fn(Zeiger, *mut Zeiger) -> i32 = methode(pfad, PFAD_OEFFNEN);
        let mut senke = std::ptr::null_mut();
        if oeffnen(pfad, &mut senke) >= 0 && !senke.is_null() {
            let anfang: unsafe extern "system" fn(Zeiger, Punkt, u32) = methode(senke, SENKE_ANFANG);
            let linien: unsafe extern "system" fn(Zeiger, *const Punkt, u32) = methode(senke, SENKE_LINIEN);
            let ende: unsafe extern "system" fn(Zeiger, u32) = methode(senke, SENKE_ENDE);
            let zu: unsafe extern "system" fn(Zeiger) -> i32 = methode(senke, SENKE_ZU);
            anfang(senke, punkte[0], if fuellen { 0 } else { 1 });
            linien(senke, punkte[1..].as_ptr(), (punkte.len() - 1) as u32);
            ende(senke, (fuellen || geschlossen) as u32);
            zu(senke);
            freigeben(senke);
            let p = pinsel(f);
            if fuellen {
                let zeichne: unsafe extern "system" fn(Zeiger, Zeiger, Zeiger, Zeiger) = methode(f.ziel, ZIEL_FORM);
                zeichne(f.ziel, pfad, p, std::ptr::null_mut());
            } else {
                let zeichne: unsafe extern "system" fn(Zeiger, Zeiger, Zeiger, f32, Zeiger) =
                    methode(f.ziel, ZIEL_FORM_RAND);
                let stil = strich_stil(f, dicke);
                zeichne(f.ziel, pfad, p, dicke, stil);
                freigeben(stil);
            }
        }
        freigeben(pfad);
    }
}

/// Die Punkte aus einer Liste von Kommazahlen: x, y, x, y ...
///
/// # Safety
/// `liste` muss eine gültige Liste von Kommazahlen sein.
unsafe fn punkte_aus(liste: *const ListenKopf) -> Vec<Punkt> {
    unsafe {
        let l = &*liste;
        if l.laenge % 2 != 0 {
            stopp(&format!(
                "De Punkte fia a Vüeck kumman paarweis (x, y, x, y ...), in da LISTN stengan oba {} Zoin.",
                l.laenge
            ));
        }
        let zahlen = std::slice::from_raw_parts(l.daten as *const f64, l.laenge as usize);
        zahlen.chunks(2).map(|p| Punkt { x: p[0] as f32, y: p[1] as f32 }).collect()
    }
}

/// `vieleck(punkte)`: eine gefüllte Form durch die Punkte x, y, x, y ...
///
/// # Safety
/// `punkte` muss eine gültige Liste von Kommazahlen sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_g_vieleck(punkte: *const ListenKopf) {
    let f = zeichnen();
    leuchtend!(f, unsafe { oida_g_vieleck(punkte) });
    unsafe {
        let punkte = punkte_aus(punkte);
        form(f, &punkte, true, true, 0.0);
    }
}

/// `linienzug(punkte, dicke, zua)`: Linien von Punkt zu Punkt; mit `zua` zurück zum ersten.
///
/// # Safety
/// `punkte` muss eine gültige Liste von Kommazahlen sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_g_linienzug(punkte: *const ListenKopf, dicke: f64, zu: bool) {
    let f = zeichnen();
    leuchtend!(f, unsafe { oida_g_linienzug(punkte, dicke, zu) });
    unsafe {
        let punkte = punkte_aus(punkte);
        form(f, &punkte, false, zu, dicke as f32);
    }
}

/// `bogn(x, y, radius, von, bis, dicke)`: ein Stück Kreislinie zwischen zwei
/// Winkeln (im Bogenmaß; 0 zeigt nach rechts, weiter geht es im Uhrzeigersinn).
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_bogen(x: f64, y: f64, radius: f64, von: f64, bis: f64, dicke: f64) {
    let f = zeichnen();
    leuchtend!(f, oida_g_bogen(x, y, radius, von, bis, dicke));
    // Als Linienzug: je größer der Bogen am Bildschirm, desto mehr Stücke.
    let weite = (bis - von).abs();
    let stuecke = ((weite * radius.abs().max(1.0) / 4.0).ceil() as usize).clamp(8, 720);
    let punkte: Vec<Punkt> = (0..=stuecke)
        .map(|i| {
            let winkel = von + (bis - von) * i as f64 / stuecke as f64;
            Punkt { x: (x + radius * winkel.cos()) as f32, y: (y + radius * winkel.sin()) as f32 }
        })
        .collect();
    unsafe { form(f, &punkte, false, false, dicke as f32) };
}

// ---------------------------------------------------------------------- Text

/// `schrift(größe, fett)`: die Schrift für alles, was danach geschrieben wird.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_schrift(groesse: f64, fett: bool) {
    let viertel = (groesse.clamp(1.0, 2000.0) * 4.0).round() as u32;
    fenster().schrift = (viertel, fett);
}

/// Das Format zur aktuellen Schrift; wird beim ersten Mal angelegt.
unsafe fn schrift_format(f: &mut Fenster) -> Zeiger {
    if let Some(format) = f.schriften.get(&f.schrift) {
        return *format;
    }
    unsafe {
        let anlegen: unsafe extern "system" fn(
            Zeiger,
            *const u16,
            Zeiger,
            u32,
            u32,
            u32,
            f32,
            *const u16,
            *mut Zeiger,
        ) -> i32 = methode(f.schrift_fabrik, SCHRIFT_FORMAT);
        let (viertel, fett) = f.schrift;
        let mut format = std::ptr::null_mut();
        let ergebnis = anlegen(
            f.schrift_fabrik,
            breit("Segoe UI").as_ptr(),
            std::ptr::null_mut(),
            if fett { 700 } else { 400 },
            0,
            5,
            viertel as f32 / 4.0,
            breit("de-at").as_ptr(),
            &mut format,
        );
        if ergebnis < 0 || format.is_null() {
            stopp(&format!("Windows gibt ma de Schrift ned (DirectWrite sogt 0x{:08X}).", ergebnis as u32));
        }
        // Kein Umbruch: Der Text bleibt in einer Zeile, außer er enthält selbst Zeilenenden.
        let umbruch: unsafe extern "system" fn(Zeiger, u32) -> i32 = methode(format, FORMAT_UMBRUCH);
        umbruch(format, 1);
        f.schriften.insert(f.schrift, format);
        format
    }
}

/// Ein Text, der schon einmal gezeichnet wurde: Text, Schrift, Farbe,
/// Ausrichtung und Vergrößerung (in Zweiunddreißigsteln).
type TextSchluessel = (Vec<u8>, (u32, bool), [u32; 4], u8, u32);

struct TextBild {
    bild: Zeiger,
    breite: f32,
    hoehe: f32,
    /// Das Bild, in dem es zuletzt gebraucht wurde.
    gebraucht: u64,
}

/// Ein Text, den DirectWrite schon gesetzt hat (welches Zeichen wo steht):
/// Text, Schrift und Ausrichtung. Das Setzen kostet bei jedem Text gut 30
/// Mikrosekunden, und die meisten Texte stehen viele Bilder lang unverändert da.
type SatzSchluessel = (Vec<u8>, (u32, bool), u8);

struct Satz {
    satz: Zeiger,
    /// Das Bild, in dem er zuletzt gebraucht wurde.
    gebraucht: u64,
}

/// Der gesetzte Text zur aktuellen Schrift und Ausrichtung, aus dem Lager oder
/// neu. Null, wenn DirectWrite ihn nicht hergibt.
unsafe fn satz_holen(f: &mut Fenster, roh: &[u8], zeichen: &[u16], format: Zeiger, ausrichtung: u8, weite: f32) -> Zeiger {
    let schluessel: SatzSchluessel = (roh.to_vec(), f.schrift, ausrichtung);
    let jetzt = f.bilder;
    if let Some(satz) = f.saetze.get_mut(&schluessel) {
        satz.gebraucht = jetzt;
        return satz.satz;
    }
    unsafe {
        // Wird es zu viel, fliegt hinaus, was schon länger niemand gebraucht hat.
        if f.saetze.len() >= 800 {
            f.saetze.retain(|_, satz| {
                let bleibt = satz.gebraucht + 120 >= jetzt;
                if !bleibt {
                    freigeben(satz.satz);
                }
                bleibt
            });
        }
        let anlegen: unsafe extern "system" fn(Zeiger, *const u16, u32, Zeiger, f32, f32, *mut Zeiger) -> i32 =
            methode(f.schrift_fabrik, formen::SCHRIFT_SATZ);
        let mut satz = std::ptr::null_mut();
        if anlegen(f.schrift_fabrik, zeichen.as_ptr(), zeichen.len() as u32, format, weite, weite, &mut satz) < 0 || satz.is_null() {
            return std::ptr::null_mut();
        }
        f.saetze.insert(schluessel, Satz { satz, gebraucht: jetzt });
        satz
    }
}

/// Hat der Text Zeichen, die Windows bunt zeichnet (Emoji) oder aus einer
/// Ersatzschrift holt (Pfeile, Sterne ...)? Die sind so langsam (über eine
/// Millisekunde pro Text), dass sich ein fertiges Bild lohnt.
fn bunt(text: &str) -> bool {
    text.chars().any(|z| z as u32 >= 0x2190)
}

/// Wie ein Text am Punkt (x, y) hängt: Ausrichtung quer und hoch für
/// DirectWrite, und wo im Kasten um den Text der Punkt sitzt (0 = links/oben,
/// 0,5 = Mitte, 1 = rechts/unten).
fn ausrichtung_teile(ausrichtung: i64) -> (u32, u32, f32, f32) {
    match ausrichtung {
        1 => (2, 2, 0.5, 0.5),
        2 => (1, 0, 1.0, 0.0),
        3 => (0, 2, 0.0, 0.5),
        4 => (1, 2, 1.0, 0.5),
        5 => (2, 1, 0.5, 1.0),
        6 => (2, 0, 0.5, 0.0),
        _ => (0, 0, 0.0, 0.0),
    }
}

/// Zeichnet den Text einmal in ein eigenes Bild, mit durchsichtigem Grund.
/// Das Bild ist `massstab`-mal so fein wie seine Größe in Einheiten, damit
/// der Text auch im vergrößerten Fenster scharf bleibt.
unsafe fn text_bild_anlegen(f: &mut Fenster, zeichen: &[u16], format: Zeiger, massstab: f32) -> Option<TextBild> {
    unsafe {
        let groesse = f.schrift.0 as f32 / 4.0;
        let zeilen = zeichen.iter().filter(|z| **z == 10).count() as f32 + 1.0;
        // Großzügig geschätzt: Wie breit der Text wirklich ist, muss niemand wissen,
        // weil er im Bild genauso ausgerichtet ist wie später am Punkt.
        let breite_roh = (zeichen.len() as f32 * groesse * 1.3 + 2.0 * groesse).min(4096.0);
        let hoehe_roh = (groesse * 1.5 * zeilen + groesse * 0.5).min(4096.0);
        // Gerade Zahlen von Bildpunkten, damit auch die Mitte auf einem Punkt liegt.
        let punkte = [
            (2.0 * (breite_roh * massstab / 2.0).ceil()).clamp(2.0, 8192.0) as u32,
            (2.0 * (hoehe_roh * massstab / 2.0).ceil()).clamp(2.0, 8192.0) as u32,
        ];
        let mass = [punkte[0] as f32 / massstab, punkte[1] as f32 / massstab];
        let anlegen: unsafe extern "system" fn(Zeiger, *const [f32; 2], *const [u32; 2], Zeiger, u32, *mut Zeiger) -> i32 =
            methode(f.ziel, ZIEL_NEBENZIEL);
        let mut neben = std::ptr::null_mut();
        if anlegen(f.ziel, &mass, &punkte, std::ptr::null_mut(), 0, &mut neben) < 0 || neben.is_null() {
            return None;
        }
        let anfang: unsafe extern "system" fn(Zeiger) = methode(neben, ZIEL_ANFANG);
        let leeren: unsafe extern "system" fn(Zeiger, *const Farbe) = methode(neben, ZIEL_LEEREN);
        let neuer_pinsel: unsafe extern "system" fn(Zeiger, *const Farbe, Zeiger, *mut Zeiger) -> i32 =
            methode(neben, ZIEL_PINSEL);
        let schreiben: unsafe extern "system" fn(Zeiger, *const u16, u32, Zeiger, *const Rechteck, Zeiger, u32, u32) =
            methode(neben, ZIEL_TEXT);
        let ende: unsafe extern "system" fn(Zeiger, *mut u64, *mut u64) -> i32 = methode(neben, ZIEL_ENDE);
        let hol_bild: unsafe extern "system" fn(Zeiger, *mut Zeiger) -> i32 = methode(neben, NEBENZIEL_BILD);

        let mut pinsel = std::ptr::null_mut();
        let mut bild = std::ptr::null_mut();
        anfang(neben);
        leeren(neben, &Farbe { r: 0.0, g: 0.0, b: 0.0, a: 0.0 });
        if neuer_pinsel(neben, &f.farbe, std::ptr::null_mut(), &mut pinsel) >= 0 && !pinsel.is_null() {
            let flaeche = Rechteck { links: 0.0, oben: 0.0, rechts: mass[0], unten: mass[1] };
            schreiben(neben, zeichen.as_ptr(), zeichen.len() as u32, format, &flaeche, pinsel, 4, 0);
        }
        let gut = ende(neben, std::ptr::null_mut(), std::ptr::null_mut()) >= 0 && hol_bild(neben, &mut bild) >= 0;
        freigeben(pinsel);
        freigeben(neben);
        if !gut || bild.is_null() {
            freigeben(bild);
            return None;
        }
        Some(TextBild { bild, breite: mass[0], hoehe: mass[1], gebraucht: f.bilder })
    }
}

/// `text_links`, `text_mittig`, `text_rechts` ...: schreibt mit der aktuellen
/// Farbe und Schrift. `ausrichtung` 0: (x, y) ist links oben; 1: die Mitte;
/// 2: rechts oben; 3: links, auf halber Höhe; 4: rechts, auf halber Höhe;
/// 5: unten in der Mitte; 6: oben in der Mitte.
///
/// Texte mit bunten Zeichen (Emoji) werden beim ersten Mal in ein Bild
/// gezeichnet und danach nur noch als Bild hingelegt: Windows braucht für
/// so einen Text über eine Millisekunde, für das Bild fast nichts.
///
/// # Safety
/// `text` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_g_text(x: f64, y: f64, text: *const TextKopf, ausrichtung: i64) {
    let f = zeichnen();
    leuchtend!(f, unsafe { oida_g_text(x, y, text, ausrichtung) });
    unsafe {
        let inhalt = String::from_utf8_lossy(bytes(text));
        let zeichen: Vec<u16> = inhalt.encode_utf16().collect();
        let format = schrift_format(f);
        let (quer_art, hoch_art, anteil_x, anteil_y) = ausrichtung_teile(ausrichtung);
        let quer: unsafe extern "system" fn(Zeiger, u32) -> i32 = methode(format, FORMAT_QUER);
        let hoch: unsafe extern "system" fn(Zeiger, u32) -> i32 = methode(format, FORMAT_HOCH);
        quer(format, quer_art);
        hoch(format, hoch_art);
        let (x, y) = (x as f32, y as f32);

        if bunt(&inhalt) {
            let lage = f.lage;
            let massstab = ((lage.m11 * lage.m11 + lage.m12 * lage.m12).sqrt() * 32.0).round().clamp(1.0, 32.0 * 16.0);
            let farbe = [f.farbe.r.to_bits(), f.farbe.g.to_bits(), f.farbe.b.to_bits(), f.farbe.a.to_bits()];
            let schluessel: TextSchluessel = (bytes(text).to_vec(), f.schrift, farbe, ausrichtung.clamp(0, 6) as u8, massstab as u32);
            if !f.text_bilder.contains_key(&schluessel) {
                // Wird es zu viel, fliegt hinaus, was schon länger niemand gebraucht hat.
                if f.text_bilder.len() >= 400 {
                    let jetzt = f.bilder;
                    f.text_bilder.retain(|_, bild| {
                        let bleibt = bild.gebraucht + 300 >= jetzt;
                        if !bleibt {
                            freigeben(bild.bild);
                        }
                        bleibt
                    });
                }
                if let Some(bild) = text_bild_anlegen(f, &zeichen, format, massstab / 32.0) {
                    f.text_bilder.insert(schluessel.clone(), bild);
                }
            }
            let jetzt = f.bilder;
            if let Some(bild) = f.text_bilder.get_mut(&schluessel) {
                bild.gebraucht = jetzt;
                let (mut links, mut oben) = (x - anteil_x * bild.breite, y - anteil_y * bild.hoehe);
                // Ohne Drehung genau auf die Bildpunkte gelegt, sonst wird der Text unscharf.
                if lage.m12 == 0.0 && lage.m21 == 0.0 && lage.m11 > 0.0 && lage.m22 > 0.0 {
                    links = ((links * lage.m11 + lage.dx).round() - lage.dx) / lage.m11;
                    oben = ((oben * lage.m22 + lage.dy).round() - lage.dy) / lage.m22;
                }
                let flaeche = Rechteck { links, oben, rechts: links + bild.breite, unten: oben + bild.hoehe };
                let zeichne: unsafe extern "system" fn(Zeiger, Zeiger, *const Rechteck, f32, u32, *const Rechteck) =
                    methode(f.ziel, ZIEL_BILD);
                zeichne(f.ziel, bild.bild, &flaeche, f.deckung, 1, std::ptr::null());
                return;
            }
            // Hat das Bild nicht geklappt, wird der Text eben direkt gezeichnet.
        }

        // Ein großzügiger Rahmen um den Punkt; Direct2D richtet den Text darin aus.
        const WEIT: f32 = 100_000.0;
        let (links, oben) = (x - anteil_x * 2.0 * WEIT, y - anteil_y * 2.0 * WEIT);
        let flaeche = Rechteck { links, oben, rechts: links + 2.0 * WEIT, unten: oben + 2.0 * WEIT };
        let p = pinsel(f);
        // Die 4 lässt bunte Zeichen (Emoji) bunt.
        let satz = satz_holen(f, bytes(text), &zeichen, format, ausrichtung.clamp(0, 6) as u8, 2.0 * WEIT);
        if !satz.is_null() {
            let zeichne: unsafe extern "system" fn(Zeiger, Punkt, Zeiger, Zeiger, u32) = methode(f.ziel, ZIEL_SATZ);
            zeichne(f.ziel, Punkt { x: links, y: oben }, satz, p, 4);
            return;
        }
        let schreiben: unsafe extern "system" fn(Zeiger, *const u16, u32, Zeiger, *const Rechteck, Zeiger, u32, u32) =
            methode(f.ziel, ZIEL_TEXT);
        schreiben(f.ziel, zeichen.as_ptr(), zeichen.len() as u32, format, &flaeche, p, 4, 0);
    }
}

// ------------------------------------------------------------------- Eingabe

/// Die Nummer, unter der Windows eine Taste kennt.
fn tasten_nummer(name: &[u8]) -> usize {
    let gross = String::from_utf8_lossy(name).to_uppercase();
    if let [zeichen] = gross.as_bytes()
        && zeichen.is_ascii_alphanumeric()
    {
        return *zeichen as usize;
    }
    match gross.as_str() {
        "LINKS" => 0x25,
        "AUFFE" | "OBN" => 0x26,
        "RECHTS" => 0x27,
        "OBE" | "UNTN" => 0x28,
        "LAA" | "LEER" => 0x20,
        "ENTER" => 0x0D,
        "ESC" => 0x1B,
        "SHIFT" => 0x10,
        "STRG" => 0x11,
        "ALT" => 0x12,
        "TAB" => 0x09,
        "ZRUCK" => 0x08,
        _ => {
            if let Some(nummer) = gross.strip_prefix('F').and_then(|n| n.parse::<usize>().ok())
                && (1..=12).contains(&nummer)
            {
                return 0x70 + nummer - 1;
            }
            stopp(&format!(
                "De Tastn \"{}\" kenn i ned. Es gibt: an Buchstobn oda a Ziffa, LINKS, RECHTS, AUFFE, OBE, \
                 LAA, ENTER, ESC, SHIFT, STRG, ALT, TAB, ZRUCK, F1 bis F12.",
                String::from_utf8_lossy(name)
            ));
        }
    }
}

/// Liest ein Drehbuch wie `RECHTS@10-50,LAA@20`. Was sich nicht lesen lässt, bricht ab.
fn drehbuch_lesen(text: &str) -> Vec<(Eingabe, u64, u64)> {
    let mut aus = Vec::new();
    for eintrag in text.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let gelesen = eintrag.split_once('@').and_then(|(name, bilder)| {
            let (von, bis) = bilder.split_once('-').unwrap_or((bilder, bilder));
            Some((eingabe_aus(name.trim())?, von.parse().ok()?, bis.parse().ok()?))
        });
        match gelesen {
            Some(eintrag) => aus.push(eintrag),
            None => stopp(&format!(
                "OIDA_FENSTER_TASTEN: \"{eintrag}\" versteh i ned. So geht's: RECHTS@10-50,LAA@20,MAUS(480;350)@30,KLICK@31"
            )),
        }
    }
    aus
}

/// Was das Drehbuch tun kann.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Eingabe {
    /// Eine Taste (ihre Nummer bei Windows) ist gedrückt.
    Taste(usize),
    /// Ein Mausknopf ist gedrückt: 1 links, 2 rechts, 3 Mitte.
    Klick(usize),
    /// Die Maus steht an dieser Stelle im Fenster.
    Maus(f32, f32),
    /// Das Mausrad dreht sich in jedem Bild um so viele Rasten (nach unten positiv).
    Rad(f32),
}

fn eingabe_aus(name: &str) -> Option<Eingabe> {
    let gross = name.to_uppercase();
    if let Some(ort) = gross.strip_prefix("MAUS(").and_then(|rest| rest.strip_suffix(')')) {
        let (x, y) = ort.split_once(';')?;
        return Some(Eingabe::Maus(x.trim().parse().ok()?, y.trim().parse().ok()?));
    }
    Some(match gross.as_str() {
        "KLICK" | "KLICK1" => Eingabe::Klick(1),
        "KLICK2" => Eingabe::Klick(2),
        "KLICK3" => Eingabe::Klick(3),
        "RADL_OBE" => Eingabe::Rad(1.0),
        "RADL_AUFFE" => Eingabe::Rad(-1.0),
        _ => Eingabe::Taste(tasten_nummer(name.as_bytes())),
    })
}

/// `taste(name)`: Ist die Taste gerade gedrückt?
///
/// # Safety
/// `name` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_g_taste(name: *const TextKopf) -> bool {
    fenster().tasten[tasten_nummer(unsafe { bytes(name) })]
}

/// `taste_neich(name)`: Ist die Taste seit dem letzten Bild neu gedrückt worden?
///
/// # Safety
/// `name` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_g_taste_neu(name: *const TextKopf) -> bool {
    fenster().tasten_neu[tasten_nummer(unsafe { bytes(name) })]
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_g_maus_x() -> f64 {
    fenster().maus.0 as f64
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_g_maus_y() -> f64 {
    fenster().maus.1 as f64
}

fn maus_knopf(knopf: i64) -> usize {
    if !(1..=3).contains(&knopf) {
        stopp(&format!("An Mausknopf {knopf} gibt's ned. 1 is links, 2 rechts, 3 de Mittn."));
    }
    knopf as usize
}

/// `maus_taste(knopf)`: Ist der Knopf gerade gedrückt? 1 links, 2 rechts, 3 Mitte.
/// `maus_radl()`: wie viele Rasten das Mausrad seit dem letzten Bild gedreht wurde (nach unten positiv).
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_maus_rad() -> f64 {
    fenster().rad as f64
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_g_maus(knopf: i64) -> bool {
    fenster().maus_tasten[maus_knopf(knopf)]
}

/// `maus_taste_neich(knopf)`: seit dem letzten Bild neu gedrückt?
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_maus_neu(knopf: i64) -> bool {
    fenster().maus_neu[maus_knopf(knopf)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lagen_hintereinander() {
        // Erst um (10, 0) verschieben, dann alles verdoppeln: aus (1, 1) wird (22, 2).
        let schieben = Lage { dx: 10.0, dy: 0.0, ..Lage::GERADE };
        let doppelt = Lage { m11: 2.0, m22: 2.0, ..Lage::GERADE };
        let beides = schieben.dann(doppelt);
        let (x, y) = (1.0 * beides.m11 + 1.0 * beides.m21 + beides.dx, 1.0 * beides.m12 + 1.0 * beides.m22 + beides.dy);
        assert_eq!((x, y), (22.0, 2.0));
    }

    #[test]
    fn stoppuhr_merkt_sich_den_namen_der_funktion() {
        // Eingeschaltet zählt sie mit, und vom Aufruf im Quelltext bleibt der Name.
        UHREN_AN.store(2, Ordering::Relaxed);
        drop(Stoppuhr::neu("unsafe { oida_g_probe_uhr(x, y, text, ausrichtung) }"));
        drop(Stoppuhr::neu("oida_g_probe_uhr(1.0)"));
        UHREN_AN.store(1, Ordering::Relaxed);
        drop(Stoppuhr::neu("oida_g_probe_aus()"));
        let bericht = uhren_bericht(2);
        let zeile = bericht.lines().find(|z| z.contains("oida_g_probe_uhr")).unwrap_or_else(|| panic!("{bericht}"));
        assert!(zeile.contains("1.0 moi im Büd") && zeile.trim_end().ends_with("ms"), "{zeile}");
        assert!(!bericht.contains("oida_g_probe_aus") && !bericht.contains("unsafe"), "{bericht}");
    }

    #[test]
    fn tasten_haben_nummern() {
        assert_eq!(tasten_nummer(b"a"), 0x41);
        assert_eq!(tasten_nummer(b"A"), 0x41);
        assert_eq!(tasten_nummer(b"7"), 0x37);
        assert_eq!(tasten_nummer(b"links"), 0x25);
        assert_eq!(tasten_nummer(b"LAA"), 0x20);
        assert_eq!(tasten_nummer(b"F5"), 0x74);
    }

    #[test]
    fn bunte_texte_und_ausrichtung() {
        assert!(!bunt("HP  100 / 100"));
        assert!(!bunt("Zeit: 00:12 • ä ö ü ß —"));
        assert!(bunt("☠ 12"));
        assert!(bunt("🔥"));
        assert!(bunt("★ BIG BOSS ★"));
        // links oben, Mitte, rechts unten ...
        assert_eq!(ausrichtung_teile(0), (0, 0, 0.0, 0.0));
        assert_eq!(ausrichtung_teile(1), (2, 2, 0.5, 0.5));
        assert_eq!(ausrichtung_teile(4), (1, 2, 1.0, 0.5));
        assert_eq!(ausrichtung_teile(5), (2, 1, 0.5, 1.0));
    }

    #[test]
    fn drehbuch() {
        use Eingabe::*;
        assert_eq!(
            drehbuch_lesen("RECHTS@10-50, LAA@20,a@3-3"),
            vec![(Taste(0x27), 10, 50), (Taste(0x20), 20, 20), (Taste(0x41), 3, 3)]
        );
        assert_eq!(
            drehbuch_lesen("MAUS(480;350.5)@30,klick@31,KLICK2@32-33,RADL_OBE@40-42,RADL_AUFFE@50"),
            vec![(Maus(480.0, 350.5), 30, 30), (Klick(1), 31, 31), (Klick(2), 32, 33), (Rad(1.0), 40, 42), (Rad(-1.0), 50, 50)]
        );
        assert!(drehbuch_lesen("").is_empty());
    }

    #[test]
    fn aufbau_wie_bei_windows() {
        // Die Größen müssen zu den Strukturen von Windows passen (64 Bit).
        assert_eq!(std::mem::size_of::<FensterKlasse>(), 80);
        assert_eq!(std::mem::size_of::<ZielAngaben>(), 28);
        assert_eq!(std::mem::size_of::<FensterZielAngaben>(), 24);
        assert_eq!(std::mem::size_of::<BildKopf>(), 40);
        assert_eq!(std::mem::size_of::<Lage>(), 24);
        assert!(std::mem::size_of::<Nachricht>() >= 48);
    }
}
