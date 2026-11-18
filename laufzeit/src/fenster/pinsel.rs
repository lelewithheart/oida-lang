//! Womit gezeichnet wird, über die eine Farbe hinaus: Farbverläufe,
//! additives Mischen ("Licht auf Licht") und Leuchten um das Gezeichnete.
//!
//! Mischen und Leuchten brauchen `ID2D1DeviceContext` (Direct2D 1.1, ab
//! Windows 8). Die Zeichenfläche von Direct2D 1.0 gibt diese Schnittstelle
//! über QueryInterface her; wo nicht, bleiben die zwei Dinge einfach aus.

use super::*;

const IID_KONTEXT: Guid = Guid(0xe8f7fe7a, 0x191c, 0x466d, [0xad, 0x95, 0x97, 0x56, 0x78, 0xbd, 0xa9, 0x98]);
const CLSID_SCHATTEN: Guid = Guid(0xc67ea361, 0x1863, 0x4e69, [0x89, 0xdb, 0x69, 0x5d, 0x3e, 0x9a, 0x5b, 0x6b]);

// ID2D1RenderTarget
const ZIEL_STUFEN: usize = 9; // CreateGradientStopCollection
const ZIEL_VERLAUF_GERADE: usize = 10; // CreateLinearGradientBrush
const ZIEL_VERLAUF_RUND: usize = 11; // CreateRadialGradientBrush
// ID2D1Brush
const PINSEL_DECKUNG: usize = 4; // SetOpacity
// ID2D1DeviceContext (nach den 57 Methoden von ID2D1RenderTarget)
const KONTEXT_EFFEKT: usize = 63; // CreateEffect
const KONTEXT_MISCHEN: usize = 78; // SetPrimitiveBlend
const KONTEXT_BILD: usize = 83; // DrawImage
// ID2D1Effect (nach IUnknown und ID2D1Properties)
const EFFEKT_WERT: usize = 9; // SetValue
const EFFEKT_EINGANG: usize = 14; // SetInput
const EFFEKT_AUSGANG: usize = 18; // GetOutput

/// Ein Farbverlauf: gerade von einem Punkt zum anderen, oder rund von innen nach außen.
pub(super) struct Verlauf {
    rund: bool,
    /// Gerade: x1, y1, x2, y2. Rund: x, y, innerer und äußerer Radius.
    masse: [f32; 4],
    /// Die Farben entlang des Wegs: Stelle (0 bis 1) und Farbe.
    stufen: Vec<(f32, Farbe)>,
    /// Der fertige Pinsel; null, solange noch Stufen dazukommen können.
    pinsel: Zeiger,
}

/// Leuchten um alles, was gezeichnet wird: wie weit es reicht und in welcher Farbe.
pub(super) struct Leuchten {
    weite: f32,
    farbe: Farbe,
}

#[repr(C)]
struct Stufe {
    stelle: f32,
    farbe: Farbe,
}

/// Holt `ID2D1DeviceContext` von einer Zeichenfläche (null, wenn es das hier nicht gibt).
pub(super) unsafe fn kontext_von(ziel: Zeiger) -> Zeiger {
    if ziel.is_null() {
        return std::ptr::null_mut();
    }
    unsafe {
        let frag: unsafe extern "system" fn(Zeiger, *const Guid, *mut Zeiger) -> i32 = methode(ziel, 0);
        let mut kontext = std::ptr::null_mut();
        if frag(ziel, &IID_KONTEXT, &mut kontext) < 0 {
            return std::ptr::null_mut();
        }
        kontext
    }
}

/// Die Zeichenfläche ist weg (oder wird gleich neu angelegt): Alles, was an ihr hing, auch.
pub(super) unsafe fn vergiss(f: &mut Fenster) {
    unsafe {
        if let Some(verlauf) = &mut f.verlauf {
            freigeben(verlauf.pinsel);
            verlauf.pinsel = std::ptr::null_mut();
        }
        freigeben(f.leucht_effekt);
        freigeben(f.leucht_ziel);
        freigeben(f.kontext);
        f.leucht_effekt = std::ptr::null_mut();
        f.leucht_ziel = std::ptr::null_mut();
        f.kontext = std::ptr::null_mut();
        f.im_leuchten = false;
    }
}

/// Mit jedem neuen Bild: wieder gewöhnlich mischen, kein Leuchten.
pub(super) unsafe fn neues_bild(f: &mut Fenster) {
    f.dazu = false;
    f.leuchten = None;
    unsafe { mischen_setzen(f) };
}

/// Stellt an der Fläche, auf die gerade gezeichnet wird, die Art zu mischen ein.
pub(super) unsafe fn mischen_setzen(f: &mut Fenster) {
    let kontext = match f.bild_ziel {
        Some(nummer) => f.bilder_liste.get(nummer).and_then(|b| b.as_ref()).map_or(std::ptr::null_mut(), |b| b.kontext),
        None => f.kontext,
    };
    if !kontext.is_null() {
        unsafe {
            let setzen: unsafe extern "system" fn(Zeiger, u32) = methode(kontext, KONTEXT_MISCHEN);
            // 0: das Neue deckt das Alte zu. 3: Die Farben werden zusammengezählt.
            setzen(kontext, if f.dazu { 3 } else { 0 });
        }
    }
}

/// `mischn_dazua()` und `mischn_normal()`.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_mischen(dazu: bool) {
    let f = zeichnen();
    f.dazu = dazu;
    unsafe { mischen_setzen(f) };
}

// ------------------------------------------------------------- Farbverläufe

unsafe fn verlauf_weg(f: &mut Fenster) {
    if let Some(verlauf) = f.verlauf.take() {
        unsafe { freigeben(verlauf.pinsel) };
    }
}

/// Eine einzelne Farbe gilt wieder: Der Verlauf ist vorbei.
pub(super) fn farbe_gesetzt(f: &mut Fenster) {
    unsafe { verlauf_weg(f) };
}

/// `valauf_grod(x1, y1, x2, y2)`: Die Farbe geht vom ersten Punkt zum zweiten über.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_verlauf_gerade(x1: f64, y1: f64, x2: f64, y2: f64) {
    let f = fenster();
    unsafe { verlauf_weg(f) };
    f.verlauf = Some(Verlauf {
        rund: false,
        masse: [x1 as f32, y1 as f32, x2 as f32, y2 as f32],
        stufen: Vec::new(),
        pinsel: std::ptr::null_mut(),
    });
}

/// `valauf_rund(x, y, innen, außn)`: Die Farbe geht von der Mitte nach außen über,
/// vom inneren zum äußeren Radius.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_verlauf_rund(x: f64, y: f64, innen: f64, aussen: f64) {
    let f = fenster();
    unsafe { verlauf_weg(f) };
    f.verlauf = Some(Verlauf {
        rund: true,
        masse: [x as f32, y as f32, innen.max(0.0) as f32, aussen.max(0.001) as f32],
        stufen: Vec::new(),
        pinsel: std::ptr::null_mut(),
    });
}

/// `valauf_foab(stö, foab, deckung)`: an der Stelle (0 = Anfang, 1 = Ende) ist es diese Farbe.
///
/// # Safety
/// `code` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_g_verlauf_stufe(stelle: f64, code: *const TextKopf, deckung: f64) {
    let f = fenster();
    let roh = unsafe { bytes(code) };
    let Some(farbe) = farbe_aus_hex(roh) else {
        stopp(&format!(
            "De Foab \"{}\" versteh i ned. So schaut ane aus: \"#ff8800\" oda \"#f80\".",
            String::from_utf8_lossy(roh)
        ));
    };
    let Some(verlauf) = &mut f.verlauf else {
        stopp("valauf_foab() geht erst noch valauf_grod() oda valauf_rund().");
    };
    unsafe { freigeben(verlauf.pinsel) };
    verlauf.pinsel = std::ptr::null_mut();
    let farbe = Farbe { a: farbe.a * deckung.clamp(0.0, 1.0) as f32, ..farbe };
    verlauf.stufen.push((stelle.clamp(0.0, 1.0) as f32, farbe));
}

/// Der Pinsel zum aktuellen Verlauf, oder `None`, wenn es keinen gibt (oder er nicht gelingt).
pub(super) unsafe fn verlauf_pinsel(f: &mut Fenster) -> Option<Zeiger> {
    let ziel = f.ziel;
    let deckung = f.deckung;
    let verlauf = f.verlauf.as_mut()?;
    if verlauf.stufen.is_empty() {
        return None;
    }
    unsafe {
        if verlauf.pinsel.is_null() {
            let [a, b, c, d] = verlauf.masse;
            // Beim runden Verlauf mit innerem Radius rücken die Stufen nach außen:
            // Direct2D kennt nur "von der Mitte bis zum Rand".
            let innen = if verlauf.rund { (c / d).clamp(0.0, 1.0) } else { 0.0 };
            let mut stufen: Vec<Stufe> =
                verlauf.stufen.iter().map(|(s, farbe)| Stufe { stelle: innen + s * (1.0 - innen), farbe: *farbe }).collect();
            stufen.sort_by(|x, y| x.stelle.total_cmp(&y.stelle));
            let sammeln: unsafe extern "system" fn(Zeiger, *const Stufe, u32, u32, u32, *mut Zeiger) -> i32 =
                methode(ziel, ZIEL_STUFEN);
            let mut sammlung = std::ptr::null_mut();
            if sammeln(ziel, stufen.as_ptr(), stufen.len() as u32, 0, 0, &mut sammlung) < 0 || sammlung.is_null() {
                return None;
            }
            let mut pinsel = std::ptr::null_mut();
            let ergebnis = if verlauf.rund {
                #[repr(C)]
                struct Rund {
                    mitte: Punkt,
                    versatz: Punkt,
                    radius_x: f32,
                    radius_y: f32,
                }
                let angaben = Rund { mitte: Punkt { x: a, y: b }, versatz: Punkt { x: 0.0, y: 0.0 }, radius_x: d, radius_y: d };
                let anlegen: unsafe extern "system" fn(Zeiger, *const Rund, Zeiger, Zeiger, *mut Zeiger) -> i32 =
                    methode(ziel, ZIEL_VERLAUF_RUND);
                anlegen(ziel, &angaben, std::ptr::null_mut(), sammlung, &mut pinsel)
            } else {
                let angaben = [Punkt { x: a, y: b }, Punkt { x: c, y: d }];
                let anlegen: unsafe extern "system" fn(Zeiger, *const [Punkt; 2], Zeiger, Zeiger, *mut Zeiger) -> i32 =
                    methode(ziel, ZIEL_VERLAUF_GERADE);
                anlegen(ziel, &angaben, std::ptr::null_mut(), sammlung, &mut pinsel)
            };
            freigeben(sammlung);
            if ergebnis < 0 || pinsel.is_null() {
                return None;
            }
            verlauf.pinsel = pinsel;
        }
        let deckung_setzen: unsafe extern "system" fn(Zeiger, f32) = methode(verlauf.pinsel, PINSEL_DECKUNG);
        deckung_setzen(verlauf.pinsel, deckung);
        Some(verlauf.pinsel)
    }
}

// ----------------------------------------------------------------- Leuchten

/// `leuchtn(weite, foab, deckung)`: Alles, was ab jetzt gezeichnet wird,
/// bekommt einen weichen Schein in dieser Farbe. Mit Weite 0 hört es auf.
///
/// # Safety
/// `code` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_g_leuchten(weite: f64, code: *const TextKopf, deckung: f64) {
    let f = zeichnen();
    if !(weite > 0.0) {
        f.leuchten = None;
        return;
    }
    let roh = unsafe { bytes(code) };
    let Some(farbe) = farbe_aus_hex(roh) else {
        stopp(&format!(
            "De Foab \"{}\" versteh i ned. So schaut ane aus: \"#ff8800\" oda \"#f80\".",
            String::from_utf8_lossy(roh)
        ));
    };
    let farbe = Farbe { a: farbe.a * deckung.clamp(0.0, 1.0) as f32, ..farbe };
    f.leuchten = Some(Leuchten { weite: weite.min(500.0) as f32, farbe });
}

/// Vor einer Form: Soll sie leuchten, wird ab jetzt in ein eigenes, leeres
/// Bild gezeichnet. Der Aufrufer zeichnet die Form dann einmal dorthin
/// (indem er sich selbst noch einmal aufruft), ruft [`leuchten_ende`] und
/// zeichnet sie danach wie gewohnt.
pub(super) unsafe fn leuchten_anfang(f: &mut Fenster) -> bool {
    if f.leuchten.is_none() || f.im_leuchten || f.kontext.is_null() || f.bild_ziel.is_some() {
        return false;
    }
    unsafe {
        // Das Bild fürs Leuchten: so groß wie das Fenster, bei anderer Größe neu.
        if !f.leucht_ziel.is_null() && f.leucht_groesse != (f.breite, f.hoehe) {
            freigeben(f.leucht_ziel);
            f.leucht_ziel = std::ptr::null_mut();
        }
        if f.leucht_ziel.is_null() {
            let anlegen: unsafe extern "system" fn(Zeiger, Zeiger, Zeiger, Zeiger, u32, *mut Zeiger) -> i32 =
                methode(f.ziel, ZIEL_NEBENZIEL);
            let mut neben = std::ptr::null_mut();
            let nix = std::ptr::null_mut();
            if anlegen(f.ziel, nix, nix, nix, 0, &mut neben) < 0 || neben.is_null() {
                return false;
            }
            f.leucht_ziel = neben;
            f.leucht_groesse = (f.breite, f.hoehe);
        }
        f.leucht_haupt = f.ziel;
        f.ziel = f.leucht_ziel;
        f.im_leuchten = true;
        let anfang: unsafe extern "system" fn(Zeiger) = methode(f.ziel, ZIEL_ANFANG);
        let leeren: unsafe extern "system" fn(Zeiger, *const Farbe) = methode(f.ziel, ZIEL_LEEREN);
        anfang(f.ziel);
        leeren(f.ziel, &Farbe { r: 0.0, g: 0.0, b: 0.0, a: 0.0 });
        lage_setzen(f);
    }
    true
}

/// Nach der Form im eigenen Bild: Aus ihrem Umriss wird der weiche Schein, und der kommt ins Fenster.
pub(super) unsafe fn leuchten_ende(f: &mut Fenster) {
    unsafe {
        let ende: unsafe extern "system" fn(Zeiger, *mut u64, *mut u64) -> i32 = methode(f.ziel, ZIEL_ENDE);
        ende(f.ziel, std::ptr::null_mut(), std::ptr::null_mut());
        f.ziel = f.leucht_haupt;
        f.im_leuchten = false;
        let Some(leuchten) = &f.leuchten else { return };
        let (weite, farbe) = (leuchten.weite, leuchten.farbe);

        if f.leucht_effekt.is_null() {
            let anlegen: unsafe extern "system" fn(Zeiger, *const Guid, *mut Zeiger) -> i32 = methode(f.kontext, KONTEXT_EFFEKT);
            let mut effekt = std::ptr::null_mut();
            if anlegen(f.kontext, &CLSID_SCHATTEN, &mut effekt) < 0 || effekt.is_null() {
                return;
            }
            f.leucht_effekt = effekt;
        }
        let hol_bild: unsafe extern "system" fn(Zeiger, *mut Zeiger) -> i32 = methode(f.leucht_ziel, NEBENZIEL_BILD);
        let mut bild = std::ptr::null_mut();
        if hol_bild(f.leucht_ziel, &mut bild) < 0 || bild.is_null() {
            return;
        }
        let effekt = f.leucht_effekt;
        let eingang: unsafe extern "system" fn(Zeiger, u32, Zeiger, i32) = methode(effekt, EFFEKT_EINGANG);
        let wert: unsafe extern "system" fn(Zeiger, u32, u32, *const u8, u32) -> i32 = methode(effekt, EFFEKT_WERT);
        let ausgang: unsafe extern "system" fn(Zeiger, *mut Zeiger) = methode(effekt, EFFEKT_AUSGANG);
        eingang(effekt, 0, bild, 1);
        // Eigenschaft 0: wie weich (Standardabweichung; im Browser ist shadowBlur das Doppelte).
        // Eigenschaft 1: die Farbe. Die 5 und die 8 sagen, was für Werte das sind (Zahl, vier Zahlen).
        let weich = weite / 2.0;
        wert(effekt, 0, 5, (&weich as *const f32).cast(), 4);
        let vier = [farbe.r, farbe.g, farbe.b, farbe.a];
        wert(effekt, 1, 8, vier.as_ptr().cast(), 16);
        let mut schein = std::ptr::null_mut();
        ausgang(effekt, &mut schein);
        if !schein.is_null() {
            // Das Bild ist schon in Bildpunkten: ohne Verschieben und Drehen hinlegen.
            let setzen: unsafe extern "system" fn(Zeiger, *const Lage) = methode(f.ziel, ZIEL_LAGE);
            setzen(f.ziel, &Lage::GERADE);
            let zeichne: unsafe extern "system" fn(Zeiger, Zeiger, *const Punkt, *const Rechteck, u32, u32) =
                methode(f.kontext, KONTEXT_BILD);
            // 0: über das Alte legen. 9: dazuzählen (wenn gerade so gemischt wird).
            zeichne(f.kontext, schein, std::ptr::null(), std::ptr::null(), 1, if f.dazu { 9 } else { 0 });
            lage_setzen(f);
            freigeben(schein);
        }
        // Der Effekt soll das Bild nicht festhalten.
        eingang(effekt, 0, std::ptr::null_mut(), 1);
        freigeben(bild);
    }
}
