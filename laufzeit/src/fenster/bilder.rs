//! Bilder: aus Dateien geladen (PNG, JPG, BMP ... was Windows lesen kann)
//! oder leer angelegt, um selbst hineinzuzeichnen und das Ergebnis dann
//! immer wieder hinzulegen.
//!
//! Gelesen wird mit der Windows Imaging Component (WIC). Das Programm kennt
//! ein Bild nur als Nummer; hier steht, was dahintersteckt.

use super::*;
use crate::neuer_text;

const CLSID_WIC_FABRIK: Guid = Guid(0xcacaf262, 0x9370, 0x4615, [0xa1, 0x3b, 0x9f, 0x55, 0x39, 0xda, 0x4c, 0x0a]);
const IID_WIC_FABRIK: Guid = Guid(0xec5ec8a9, 0xc395, 0x4314, [0x9c, 0x77, 0x54, 0xd7, 0xa9, 0x35, 0xff, 0x70]);
/// 32 Bit pro Punkt, Blau-Grün-Rot-Deckung, Farben schon mit der Deckung malgenommen.
const WIC_FORMAT: Guid = Guid(0x6fddc324, 0x4e03, 0x4bfe, [0xb1, 0x85, 0x3d, 0x77, 0x76, 0x8d, 0xc9, 0x10]);

// IWICImagingFactory, IWICBitmapDecoder, IWICFormatConverter
const WIC_LESER: usize = 3; // CreateDecoderFromFilename
const WIC_WANDLER: usize = 10; // CreateFormatConverter
const LESER_BILD: usize = 13; // GetFrame
const WANDLER_GROESSE: usize = 3; // GetSize
const WANDLER_START: usize = 8; // Initialize
// ID2D1RenderTarget
const ZIEL_BILD_AUS_WIC: usize = 5; // CreateBitmapFromWicBitmap

pub(super) struct Bild {
    /// Das Bild bei Direct2D; null, wenn es (nach einem Verlust der Zeichenfläche) nicht wiederkam.
    bild: Zeiger,
    /// Nur bei Bildern zum Hineinzeichnen: ihre eigene Zeichenfläche und deren Kontext.
    ziel: Zeiger,
    pub(super) kontext: Zeiger,
    breite: u32,
    hoehe: u32,
    /// Bei Bildern aus Dateien: woher, um sie notfalls noch einmal zu laden.
    pfad: Option<String>,
}

/// Die Zeichenfläche ist weg: Die Bilder darauf auch. Was aus einer Datei kam, kommt wieder.
pub(super) unsafe fn vergiss(f: &mut Fenster) {
    unsafe {
        for bild in f.bilder_liste.iter_mut().flatten() {
            freigeben(bild.kontext);
            freigeben(bild.bild);
            freigeben(bild.ziel);
            bild.kontext = std::ptr::null_mut();
            bild.bild = std::ptr::null_mut();
            bild.ziel = std::ptr::null_mut();
        }
        f.bild_ziel = None;
    }
}

/// Nach einer neuen Zeichenfläche: Bilder aus Dateien noch einmal laden,
/// Bilder zum Hineinzeichnen leer neu anlegen.
pub(super) unsafe fn neu_anlegen(f: &mut Fenster) {
    for nummer in 0..f.bilder_liste.len() {
        let Some(alt) = &f.bilder_liste[nummer] else { continue };
        let (pfad, breite, hoehe) = (alt.pfad.clone(), alt.breite, alt.hoehe);
        let neu = unsafe {
            match &pfad {
                Some(pfad) => aus_datei(f, pfad).ok(),
                None => leeres(f, breite, hoehe),
            }
        };
        if let Some(neu) = neu {
            f.bilder_liste[nummer] = Some(neu);
        }
    }
}

fn ablegen(f: &mut Fenster, bild: Bild) -> i64 {
    match f.bilder_liste.iter().position(|platz| platz.is_none()) {
        Some(frei) => {
            f.bilder_liste[frei] = Some(bild);
            frei as i64
        }
        None => {
            f.bilder_liste.push(Some(bild));
            f.bilder_liste.len() as i64 - 1
        }
    }
}

fn hol(f: &mut Fenster, nummer: i64) -> &mut Bild {
    match f.bilder_liste.get_mut(nummer.max(0) as usize).and_then(|platz| platz.as_mut()) {
        Some(bild) if nummer >= 0 => bild,
        _ => stopp(&format!("A Büd mit da Numma {nummer} gibt's ned (oda nimma).")),
    }
}

/// Liest ein Bild aus einer Datei. Der Fehler sagt in einem Wort, woran es lag.
unsafe fn aus_datei(f: &mut Fenster, pfad: &str) -> Result<Bild, &'static str> {
    unsafe {
        if f.wic.is_null() {
            let starten: unsafe extern "system" fn(Zeiger, u32) -> i32 = hole("ole32.dll", "CoInitializeEx");
            let anlegen: unsafe extern "system" fn(*const Guid, Zeiger, u32, *const Guid, *mut Zeiger) -> i32 =
                hole("ole32.dll", "CoCreateInstance");
            // 2: ein Faden für sich. Dass COM schon läuft, ist kein Fehler.
            starten(std::ptr::null_mut(), 2);
            let mut fabrik = std::ptr::null_mut();
            if anlegen(&CLSID_WIC_FABRIK, std::ptr::null_mut(), 1, &IID_WIC_FABRIK, &mut fabrik) < 0 || fabrik.is_null() {
                return Err("Windows kaun do kane Büda lesn.");
            }
            f.wic = fabrik;
        }
        let leser_anlegen: unsafe extern "system" fn(Zeiger, *const u16, Zeiger, u32, u32, *mut Zeiger) -> i32 =
            methode(f.wic, WIC_LESER);
        let mut leser = std::ptr::null_mut();
        // 0x80000000: nur lesen
        if leser_anlegen(f.wic, breit(pfad).as_ptr(), std::ptr::null_mut(), 0x8000_0000, 0, &mut leser) < 0 || leser.is_null() {
            return Err(if std::path::Path::new(pfad).is_file() { "Des is ka Büd, des i lesn kaun." } else { "De gibt's ned." });
        }
        let (mut erstes, mut wandler, mut bild) = (std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
        let hol_bild: unsafe extern "system" fn(Zeiger, u32, *mut Zeiger) -> i32 = methode(leser, LESER_BILD);
        let wandler_anlegen: unsafe extern "system" fn(Zeiger, *mut Zeiger) -> i32 = methode(f.wic, WIC_WANDLER);
        let (mut breite, mut hoehe) = (0u32, 0u32);
        let mut gut = hol_bild(leser, 0, &mut erstes) >= 0 && !erstes.is_null();
        gut = gut && wandler_anlegen(f.wic, &mut wandler) >= 0 && !wandler.is_null();
        if gut {
            let start: unsafe extern "system" fn(Zeiger, Zeiger, *const Guid, u32, Zeiger, f64, u32) -> i32 =
                methode(wandler, WANDLER_START);
            let groesse: unsafe extern "system" fn(Zeiger, *mut u32, *mut u32) -> i32 = methode(wandler, WANDLER_GROESSE);
            gut = start(wandler, erstes, &WIC_FORMAT, 0, std::ptr::null_mut(), 0.0, 0) >= 0
                && groesse(wandler, &mut breite, &mut hoehe) >= 0;
        }
        if gut {
            let anlegen: unsafe extern "system" fn(Zeiger, Zeiger, Zeiger, *mut Zeiger) -> i32 =
                methode(f.ziel, ZIEL_BILD_AUS_WIC);
            gut = anlegen(f.ziel, wandler, std::ptr::null_mut(), &mut bild) >= 0 && !bild.is_null();
        }
        freigeben(wandler);
        freigeben(erstes);
        freigeben(leser);
        if !gut {
            return Err("Des is ka Büd, des i lesn kaun.");
        }
        Ok(Bild {
            bild,
            ziel: std::ptr::null_mut(),
            kontext: std::ptr::null_mut(),
            breite,
            hoehe,
            pfad: Some(pfad.to_string()),
        })
    }
}

/// Legt ein leeres (durchsichtiges) Bild an, in das man zeichnen kann.
unsafe fn leeres(f: &mut Fenster, breite: u32, hoehe: u32) -> Option<Bild> {
    unsafe {
        let mass = [breite as f32, hoehe as f32];
        let punkte = [breite, hoehe];
        let anlegen: unsafe extern "system" fn(Zeiger, *const [f32; 2], *const [u32; 2], Zeiger, u32, *mut Zeiger) -> i32 =
            methode(f.ziel, ZIEL_NEBENZIEL);
        let mut ziel = std::ptr::null_mut();
        if anlegen(f.ziel, &mass, &punkte, std::ptr::null_mut(), 0, &mut ziel) < 0 || ziel.is_null() {
            return None;
        }
        let anfang: unsafe extern "system" fn(Zeiger) = methode(ziel, ZIEL_ANFANG);
        let leeren: unsafe extern "system" fn(Zeiger, *const Farbe) = methode(ziel, ZIEL_LEEREN);
        let ende: unsafe extern "system" fn(Zeiger, *mut u64, *mut u64) -> i32 = methode(ziel, ZIEL_ENDE);
        let hol_bild: unsafe extern "system" fn(Zeiger, *mut Zeiger) -> i32 = methode(ziel, NEBENZIEL_BILD);
        anfang(ziel);
        leeren(ziel, &Farbe { r: 0.0, g: 0.0, b: 0.0, a: 0.0 });
        ende(ziel, std::ptr::null_mut(), std::ptr::null_mut());
        let mut bild = std::ptr::null_mut();
        if hol_bild(ziel, &mut bild) < 0 || bild.is_null() {
            freigeben(ziel);
            return None;
        }
        Some(Bild { bild, ziel, kontext: pinsel::kontext_von(ziel), breite, hoehe, pfad: None })
    }
}

thread_local! {
    /// Warum das letzte Laden nicht geklappt hat.
    static LETZTER_FEHLER: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// `lod_büd(pfad)`: liest ein Bild aus einer Datei und gibt seine Nummer
/// zurück, oder -1 (dann sagt `oida_g_bild_fehler`, warum).
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_g_bild_laden(pfad: *const TextKopf) -> i64 {
    let f = fenster();
    let pfad = String::from_utf8_lossy(unsafe { bytes(pfad) }).into_owned();
    match unsafe { aus_datei(f, &pfad) } {
        Ok(bild) => ablegen(f, bild),
        Err(grund) => {
            LETZTER_FEHLER.with(|fehler| *fehler.borrow_mut() = grund.to_string());
            -1
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_g_bild_fehler() -> *mut TextKopf {
    LETZTER_FEHLER.with(|fehler| neuer_text(fehler.borrow().as_bytes()))
}

/// `neichs_büd(breite, höhe)`: ein leeres Bild zum Hineinzeichnen.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_bild_neu(breite: i64, hoehe: i64) -> i64 {
    let f = fenster();
    if !(1..=16384).contains(&breite) || !(1..=16384).contains(&hoehe) {
        stopp(&format!("A Büd mit {breite} moi {hoehe} Punkte? Des geht ned."));
    }
    match unsafe { leeres(f, breite as u32, hoehe as u32) } {
        Some(bild) => ablegen(f, bild),
        None => stopp("Windows gibt ma ka Büd zum Einezeichnen."),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_g_bild_breite(nummer: i64) -> i64 {
    hol(fenster(), nummer).breite as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_g_bild_hoehe(nummer: i64) -> i64 {
    hol(fenster(), nummer).hoehe as i64
}

/// Gibt ein Bild wieder her. Seine Nummer gilt danach nicht mehr.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_bild_weg(nummer: i64) {
    let f = fenster();
    hol(f, nummer);
    if f.bild_ziel == Some(nummer as usize) {
        unsafe { ins_fenster(f) };
    }
    if let Some(bild) = f.bilder_liste[nummer as usize].take() {
        unsafe {
            freigeben(bild.kontext);
            freigeben(bild.bild);
            freigeben(bild.ziel);
        }
    }
}

/// `büd_glott(jo)`: Vergrößerte Bilder werden weich (WOAHR) oder zeigen ihre Punkte als Kästchen (FOISCH).
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_bild_glatt(glatt: bool) {
    fenster().bild_glatt = glatt;
}

/// Zeichnet den Ausschnitt (qx, qy, qb, qh) vom Bild in das Rechteck (x, y, breite, höhe).
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn oida_g_bild_teil(nummer: i64, qx: f64, qy: f64, qb: f64, qh: f64, x: f64, y: f64, breite: f64, hoehe: f64) {
    let f = zeichnen();
    leuchtend!(f, oida_g_bild_teil(nummer, qx, qy, qb, qh, x, y, breite, hoehe));
    if f.bild_ziel == Some(nummer.max(0) as usize) {
        stopp("A Büd kaun ma ned in si söwa einezeichnen.");
    }
    let (deckung, glatt, ziel) = (f.deckung, f.bild_glatt, f.ziel);
    let bild = hol(f, nummer);
    if bild.bild.is_null() {
        return;
    }
    let wohin = Rechteck { links: x as f32, oben: y as f32, rechts: (x + breite) as f32, unten: (y + hoehe) as f32 };
    let woher = Rechteck { links: qx as f32, oben: qy as f32, rechts: (qx + qb) as f32, unten: (qy + qh) as f32 };
    unsafe {
        let zeichne: unsafe extern "system" fn(Zeiger, Zeiger, *const Rechteck, f32, u32, *const Rechteck) = methode(ziel, ZIEL_BILD);
        zeichne(ziel, bild.bild, &wohin, deckung, glatt as u32, &woher);
    }
}

/// Zeichnet das ganze Bild in das Rechteck (x, y, breite, höhe).
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_bild(nummer: i64, x: f64, y: f64, breite: f64, hoehe: f64) {
    let bild = hol(fenster(), nummer);
    let (b, h) = (bild.breite as f64, bild.hoehe as f64);
    oida_g_bild_teil(nummer, 0.0, 0.0, b, h, x, y, breite, hoehe);
}

/// Ab jetzt wird wieder ins Fenster gezeichnet (falls gerade in ein Bild gezeichnet wurde).
pub(super) unsafe fn ins_fenster(f: &mut Fenster) {
    let Some(nummer) = f.bild_ziel.take() else { return };
    unsafe {
        formen::ausschnitte_zu_bis(f, f.ausschnitte_unten);
        f.ausschnitte_unten = 0;
        let ende: unsafe extern "system" fn(Zeiger, *mut u64, *mut u64) -> i32 = methode(f.ziel, ZIEL_ENDE);
        ende(f.ziel, std::ptr::null_mut(), std::ptr::null_mut());
        f.ziel = f.bild_haupt;
        f.lage = f.bild_lage;
        f.gemerkt = std::mem::take(&mut f.bild_gemerkt);
        lage_setzen(f);
        pinsel::mischen_setzen(f);
    }
    let _ = nummer;
}

/// `moi_in(büd)`: Ab jetzt landet alles Gezeichnete in diesem Bild statt im Fenster,
/// bis `moi_ins_fensta()` kommt (oder das Bild zu Ende ist). Mit `nummer` -1: zurück ins Fenster.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_bild_ziel(nummer: i64) {
    let f = zeichnen();
    unsafe { ins_fenster(f) };
    if nummer < 0 {
        return;
    }
    let bild = hol(f, nummer);
    let ziel = bild.ziel;
    if ziel.is_null() {
        stopp("In a Büd aus ana Datei kaun ma ned einezeichnen. Leg ans mit neichs_büd() au und zeichn des aundare eine.");
    }
    unsafe {
        f.bild_haupt = f.ziel;
        f.bild_lage = f.lage;
        f.bild_gemerkt = std::mem::take(&mut f.gemerkt);
        f.ausschnitte_unten = f.ausschnitte.len();
        f.ziel = ziel;
        f.bild_ziel = Some(nummer as usize);
        f.lage = Lage::GERADE;
        let anfang: unsafe extern "system" fn(Zeiger) = methode(ziel, ZIEL_ANFANG);
        anfang(ziel);
        lage_setzen(f);
        pinsel::mischen_setzen(f);
    }
}

/// `büd_laar(büd)`: macht ein Bild zum Hineinzeichnen wieder ganz durchsichtig.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_bild_leeren(nummer: i64) {
    let f = zeichnen();
    let gerade_ziel = f.bild_ziel == Some(nummer.max(0) as usize);
    let bild = hol(f, nummer);
    let ziel = bild.ziel;
    if ziel.is_null() {
        stopp("A Büd aus ana Datei kaun ma ned laar mochn.");
    }
    unsafe {
        let anfang: unsafe extern "system" fn(Zeiger) = methode(ziel, ZIEL_ANFANG);
        let leeren: unsafe extern "system" fn(Zeiger, *const Farbe) = methode(ziel, ZIEL_LEEREN);
        let ende: unsafe extern "system" fn(Zeiger, *mut u64, *mut u64) -> i32 = methode(ziel, ZIEL_ENDE);
        if !gerade_ziel {
            anfang(ziel);
        }
        leeren(ziel, &Farbe { r: 0.0, g: 0.0, b: 0.0, a: 0.0 });
        if !gerade_ziel {
            ende(ziel, std::ptr::null_mut(), std::ptr::null_mut());
        }
    }
}
