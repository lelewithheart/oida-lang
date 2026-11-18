//! Freie Formen aus Linien und Kurven, Ausschnitte (nur innerhalb einer
//! Form wird gezeichnet) und die Breite von Text.

use super::*;

// ID2D1Factory
const FABRIK_RUND: usize = 6; // CreateRoundedRectangleGeometry
const FABRIK_ELLIPSE: usize = 7; // CreateEllipseGeometry
// ID2D1RenderTarget
const ZIEL_EBENE_AUF: usize = 40; // PushLayer
const ZIEL_EBENE_ZU: usize = 41; // PopLayer
const ZIEL_KANTE_AUF: usize = 45; // PushAxisAlignedClip
const ZIEL_KANTE_ZU: usize = 46; // PopAxisAlignedClip
// ID2D1GeometrySink
const SENKE_FUELLART: usize = 3; // SetFillMode
const SENKE_LINIE: usize = 10; // AddLine
const SENKE_KURVE3: usize = 11; // AddBezier
const SENKE_KURVE: usize = 12; // AddQuadraticBezier
// IDWriteFactory, IDWriteTextLayout
const SCHRIFT_SATZ: usize = 18; // CreateTextLayout
const SATZ_MASSE: usize = 60; // GetMetrics

/// Ein Stück einer freien Form.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Stueck {
    /// Hierhin gehen, ohne zu zeichnen: Ein neuer Teil der Form beginnt.
    Geh(Punkt),
    Linie(Punkt),
    /// Eine Kurve zum zweiten Punkt, die sich zum ersten hin biegt.
    Kurve(Punkt, Punkt),
    /// Eine Kurve zum dritten Punkt mit zwei Punkten, zu denen sie sich biegt.
    Kurve3(Punkt, Punkt, Punkt),
    /// Zurück zum Anfang dieses Teils.
    Zu,
}

/// Die freie Form, die gerade gebaut wird.
#[derive(Default)]
pub(super) struct Pfad {
    stuecke: Vec<Stueck>,
    /// Die fertige Form für Direct2D; null, solange sich die Stücke noch ändern.
    form: Zeiger0,
}

/// Ein Zeiger, der mit null anfängt (damit `Pfad` ein `Default` haben kann).
pub(super) struct Zeiger0(Zeiger);

impl Default for Zeiger0 {
    fn default() -> Self {
        Zeiger0(std::ptr::null_mut())
    }
}

/// Ein offener Ausschnitt: eine Kante (gerades Rechteck) oder eine Ebene mit Form.
pub(super) enum Ausschnitt {
    Kante,
    Ebene(Zeiger),
}

pub(super) unsafe fn vergiss(f: &mut Fenster) {
    unsafe {
        freigeben(f.pfad.form.0);
        f.pfad.form.0 = std::ptr::null_mut();
        // Die Ausschnitte hingen an der alten Zeichenfläche.
        for ausschnitt in f.ausschnitte.drain(..) {
            if let Ausschnitt::Ebene(form) = ausschnitt {
                freigeben(form);
            }
        }
    }
}

// ------------------------------------------------------------- freie Formen

fn stueck(f: &mut Fenster, neu: Stueck) {
    unsafe { freigeben(f.pfad.form.0) };
    f.pfad.form.0 = std::ptr::null_mut();
    f.pfad.stuecke.push(neu);
}

fn punkt(x: f64, y: f64) -> Punkt {
    Punkt { x: x as f32, y: y as f32 }
}

/// `form_anfang()`: Eine neue Form beginnt, die alte ist vergessen.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_form_anfang() {
    let f = fenster();
    unsafe { freigeben(f.pfad.form.0) };
    f.pfad.form.0 = std::ptr::null_mut();
    f.pfad.stuecke.clear();
}

/// `form_geh(x, y)`: hierhin, ohne zu zeichnen.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_form_geh(x: f64, y: f64) {
    stueck(fenster(), Stueck::Geh(punkt(x, y)));
}

/// `form_linie(x, y)`: eine gerade Linie hierher.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_form_linie(x: f64, y: f64) {
    stueck(fenster(), Stueck::Linie(punkt(x, y)));
}

/// `form_kurvn(zx, zy, x, y)`: eine Kurve nach (x, y), die sich zu (zx, zy) hin biegt.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_form_kurve(zx: f64, zy: f64, x: f64, y: f64) {
    stueck(fenster(), Stueck::Kurve(punkt(zx, zy), punkt(x, y)));
}

/// `form_kurvn3(z1x, z1y, z2x, z2y, x, y)`: eine Kurve nach (x, y) mit zwei Punkten, zu denen sie sich biegt.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_form_kurve3(z1x: f64, z1y: f64, z2x: f64, z2y: f64, x: f64, y: f64) {
    stueck(fenster(), Stueck::Kurve3(punkt(z1x, z1y), punkt(z2x, z2y), punkt(x, y)));
}

/// `form_bogn(x, y, radius, von, bis)`: ein Stück Kreis um (x, y) vom Winkel
/// `von` bis `bis`. Gibt es schon einen Punkt, führt eine Linie zum Anfang des Bogens.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_form_bogen(x: f64, y: f64, radius: f64, von: f64, bis: f64) {
    let f = fenster();
    let offen = !matches!(f.pfad.stuecke.last(), None | Some(Stueck::Zu));
    let weite = (bis - von).abs();
    let stuecke = ((weite * radius.abs().max(1.0) / 4.0).ceil() as usize).clamp(8, 720);
    for i in 0..=stuecke {
        let winkel = von + (bis - von) * i as f64 / stuecke as f64;
        let p = punkt(x + radius * winkel.cos(), y + radius * winkel.sin());
        stueck(f, if i == 0 && !offen { Stueck::Geh(p) } else { Stueck::Linie(p) });
    }
}

/// `form_zua()`: zurück zum Anfang dieses Teils der Form.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_form_zu() {
    stueck(fenster(), Stueck::Zu);
}

/// Baut aus den Stücken die Form für Direct2D (einmal; danach wird sie wiederverwendet).
unsafe fn form_bauen(f: &mut Fenster) -> Zeiger {
    if !f.pfad.form.0.is_null() {
        return f.pfad.form.0;
    }
    if f.pfad.stuecke.is_empty() {
        return std::ptr::null_mut();
    }
    unsafe {
        let neuer_pfad: unsafe extern "system" fn(Zeiger, *mut Zeiger) -> i32 = methode(f.fabrik, FABRIK_PFAD);
        let mut pfad = std::ptr::null_mut();
        if neuer_pfad(f.fabrik, &mut pfad) < 0 || pfad.is_null() {
            return std::ptr::null_mut();
        }
        let oeffnen: unsafe extern "system" fn(Zeiger, *mut Zeiger) -> i32 = methode(pfad, PFAD_OEFFNEN);
        let mut senke = std::ptr::null_mut();
        if oeffnen(pfad, &mut senke) < 0 || senke.is_null() {
            freigeben(pfad);
            return std::ptr::null_mut();
        }
        let fuellart: unsafe extern "system" fn(Zeiger, u32) = methode(senke, SENKE_FUELLART);
        let anfang: unsafe extern "system" fn(Zeiger, Punkt, u32) = methode(senke, SENKE_ANFANG);
        let linie: unsafe extern "system" fn(Zeiger, Punkt) = methode(senke, SENKE_LINIE);
        let kurve: unsafe extern "system" fn(Zeiger, *const [Punkt; 2]) = methode(senke, SENKE_KURVE);
        let kurve3: unsafe extern "system" fn(Zeiger, *const [Punkt; 3]) = methode(senke, SENKE_KURVE3);
        let ende: unsafe extern "system" fn(Zeiger, u32) = methode(senke, SENKE_ENDE);
        let zu: unsafe extern "system" fn(Zeiger) -> i32 = methode(senke, SENKE_ZU);
        // 1: Was die Linie umschließt, ist innen, egal wie oft sie sich kreuzt (wie im Browser).
        fuellart(senke, 1);
        let mut offen = false;
        // Wo dieser Teil begonnen hat: Dorthin führt `Zu`, und von dort geht es danach weiter.
        let mut start = Punkt { x: 0.0, y: 0.0 };
        for stueck in &f.pfad.stuecke {
            // Jedes Stück außer `Geh` braucht einen Anfang.
            let braucht_anfang = !offen && !matches!(stueck, Stueck::Geh(_) | Stueck::Zu);
            if braucht_anfang {
                let p = match stueck {
                    Stueck::Linie(p) => *p,
                    _ => start,
                };
                anfang(senke, p, 0);
                start = p;
                offen = true;
                if matches!(stueck, Stueck::Linie(_)) {
                    continue;
                }
            }
            match stueck {
                Stueck::Geh(p) => {
                    if offen {
                        ende(senke, 0);
                    }
                    anfang(senke, *p, 0);
                    start = *p;
                    offen = true;
                }
                Stueck::Linie(p) => linie(senke, *p),
                Stueck::Kurve(z, p) => kurve(senke, &[*z, *p]),
                Stueck::Kurve3(z1, z2, p) => kurve3(senke, &[*z1, *z2, *p]),
                Stueck::Zu => {
                    if offen {
                        ende(senke, 1);
                        offen = false;
                    }
                }
            }
        }
        if offen {
            ende(senke, 0);
        }
        zu(senke);
        freigeben(senke);
        f.pfad.form.0 = pfad;
        pfad
    }
}

/// `form_fülln()`: füllt die Form mit der aktuellen Farbe. Die Form bleibt, man kann sie danach noch umranden.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_form_fuellen() {
    let f = zeichnen();
    leuchtend!(f, oida_g_form_fuellen());
    unsafe {
        let form = form_bauen(f);
        if form.is_null() {
            return;
        }
        let p = pinsel(f);
        let zeichne: unsafe extern "system" fn(Zeiger, Zeiger, Zeiger, Zeiger) = methode(f.ziel, ZIEL_FORM);
        zeichne(f.ziel, form, p, std::ptr::null_mut());
    }
}

/// `form_rand(dicke)`: zeichnet die Linie der Form.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_form_rand(dicke: f64) {
    let f = zeichnen();
    leuchtend!(f, oida_g_form_rand(dicke));
    unsafe {
        let form = form_bauen(f);
        if form.is_null() {
            return;
        }
        let p = pinsel(f);
        let zeichne: unsafe extern "system" fn(Zeiger, Zeiger, Zeiger, f32, Zeiger) = methode(f.ziel, ZIEL_FORM_RAND);
        let stil = strich_stil(f, dicke as f32);
        zeichne(f.ziel, form, p, dicke as f32, stil);
        freigeben(stil);
    }
}

// -------------------------------------------------------------- Ausschnitte

#[repr(C)]
struct EbenenAngaben {
    grenzen: Rechteck,
    maske: Zeiger,
    kanten: u32,
    masken_lage: Lage,
    deckung: f32,
    deck_pinsel: Zeiger,
    art: u32,
}

/// Öffnet eine Ebene, in der nur innerhalb von `form` gezeichnet wird.
unsafe fn ebene_auf(f: &mut Fenster, form: Zeiger) {
    if form.is_null() {
        return;
    }
    let weit = f32::MAX;
    let angaben = EbenenAngaben {
        grenzen: Rechteck { links: -weit, oben: -weit, rechts: weit, unten: weit },
        maske: form,
        kanten: 0,
        masken_lage: Lage::GERADE,
        deckung: 1.0,
        deck_pinsel: std::ptr::null_mut(),
        art: 0,
    };
    unsafe {
        let auf: unsafe extern "system" fn(Zeiger, *const EbenenAngaben, Zeiger) = methode(f.ziel, ZIEL_EBENE_AUF);
        auf(f.ziel, &angaben, std::ptr::null_mut());
    }
    f.ausschnitte.push(Ausschnitt::Ebene(form));
}

/// `ausschnitt_rechteck(x, y, breite, höhe)`: Ab jetzt wird nur noch in diesem Rechteck gezeichnet.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_ausschnitt_rechteck(x: f64, y: f64, breite: f64, hoehe: f64) {
    let f = zeichnen();
    let flaeche = Rechteck { links: x as f32, oben: y as f32, rechts: (x + breite) as f32, unten: (y + hoehe) as f32 };
    unsafe {
        let auf: unsafe extern "system" fn(Zeiger, *const Rechteck, u32) = methode(f.ziel, ZIEL_KANTE_AUF);
        auf(f.ziel, &flaeche, 0);
    }
    f.ausschnitte.push(Ausschnitt::Kante);
}

/// `ausschnitt_rund(x, y, breite, höhe, radius)`: ... in einem Rechteck mit runden Ecken.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_ausschnitt_rund(x: f64, y: f64, breite: f64, hoehe: f64, radius: f64) {
    let f = zeichnen();
    let rund = Rund {
        flaeche: Rechteck { links: x as f32, oben: y as f32, rechts: (x + breite) as f32, unten: (y + hoehe) as f32 },
        radius_x: radius as f32,
        radius_y: radius as f32,
    };
    unsafe {
        let anlegen: unsafe extern "system" fn(Zeiger, *const Rund, *mut Zeiger) -> i32 = methode(f.fabrik, FABRIK_RUND);
        let mut form = std::ptr::null_mut();
        anlegen(f.fabrik, &rund, &mut form);
        ebene_auf(f, form);
    }
}

/// `ausschnitt_kreis(x, y, radius)`: ... in einem Kreis.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_ausschnitt_kreis(x: f64, y: f64, radius: f64) {
    let f = zeichnen();
    let kreis = Ellipse { mitte: punkt(x, y), radius_x: radius as f32, radius_y: radius as f32 };
    unsafe {
        let anlegen: unsafe extern "system" fn(Zeiger, *const Ellipse, *mut Zeiger) -> i32 = methode(f.fabrik, FABRIK_ELLIPSE);
        let mut form = std::ptr::null_mut();
        anlegen(f.fabrik, &kreis, &mut form);
        ebene_auf(f, form);
    }
}

/// Macht den zuletzt geöffneten Ausschnitt wieder zu. Gibt FOISCH zurück, wenn keiner offen war.
pub(super) unsafe fn ausschnitt_zu(f: &mut Fenster) -> bool {
    let Some(ausschnitt) = f.ausschnitte.pop() else { return false };
    unsafe {
        match ausschnitt {
            Ausschnitt::Kante => {
                let zu: unsafe extern "system" fn(Zeiger) = methode(f.ziel, ZIEL_KANTE_ZU);
                zu(f.ziel);
            }
            Ausschnitt::Ebene(form) => {
                let zu: unsafe extern "system" fn(Zeiger) = methode(f.ziel, ZIEL_EBENE_ZU);
                zu(f.ziel);
                freigeben(form);
            }
        }
    }
    true
}

/// Macht alle Ausschnitte zu, die über `bleiben` hinaus offen sind (am Ende eines Bildes alle).
pub(super) unsafe fn ausschnitte_zu_bis(f: &mut Fenster, bleiben: usize) {
    while f.ausschnitte.len() > bleiben {
        unsafe { ausschnitt_zu(f) };
    }
}

/// `ausschnitt_aus()`: Der zuletzt gesetzte Ausschnitt gilt nicht mehr.
#[unsafe(no_mangle)]
pub extern "C" fn oida_g_ausschnitt_aus() {
    let f = zeichnen();
    if f.ausschnitte.len() <= f.ausschnitte_unten || !unsafe { ausschnitt_zu(f) } {
        stopp("ausschnitt_aus() ohne an Ausschnitt davua.");
    }
}

// --------------------------------------------------------------- Textbreite

/// `text_breite(inhalt)`: wie breit der Text in der aktuellen Schrift wird.
///
/// # Safety
/// `text` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_g_text_breite(text: *const TextKopf) -> f64 {
    let f = fenster();
    #[repr(C)]
    #[derive(Default)]
    struct Masse {
        links: f32,
        oben: f32,
        breite: f32,
        breite_mit_leer: f32,
        hoehe: f32,
        satz_breite: f32,
        satz_hoehe: f32,
        tiefe: u32,
        zeilen: u32,
    }
    unsafe {
        let zeichen: Vec<u16> = String::from_utf8_lossy(bytes(text)).encode_utf16().collect();
        let format = schrift_format(f);
        let anlegen: unsafe extern "system" fn(Zeiger, *const u16, u32, Zeiger, f32, f32, *mut Zeiger) -> i32 =
            methode(f.schrift_fabrik, SCHRIFT_SATZ);
        let mut satz = std::ptr::null_mut();
        if anlegen(f.schrift_fabrik, zeichen.as_ptr(), zeichen.len() as u32, format, 100_000.0, 100_000.0, &mut satz) < 0
            || satz.is_null()
        {
            return 0.0;
        }
        let mut masse = Masse::default();
        let messen: unsafe extern "system" fn(Zeiger, *mut Masse) -> i32 = methode(satz, SATZ_MASSE);
        messen(satz, &mut masse);
        freigeben(satz);
        masse.breite_mit_leer as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ebenen_angaben_wie_bei_windows() {
        // D2D1_LAYER_PARAMETERS: 16 + 8 + 4 + 24 + 4 + 8 + 4, mit Lücken 72 Bytes.
        assert_eq!(std::mem::size_of::<EbenenAngaben>(), 72);
        assert_eq!(std::mem::offset_of!(EbenenAngaben, masken_lage), 28);
        assert_eq!(std::mem::offset_of!(EbenenAngaben, deck_pinsel), 56);
    }
}
