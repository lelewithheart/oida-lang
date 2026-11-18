//! Zeichnet das Ergebnis als SVG: je Programm ein kleines Feld mit
//! waagrechten Balken, einer je Sprache.
//!
//! Gestaltung:
//!  - OIDA ist farbig, die anderen Sprachen sind grau: es geht um den Vergleich mit OIDA.
//!  - Die Balken stehen in der Reihenfolge, in der der Aufrufer sie übergibt
//!    (das Messwerkzeug sortiert: der beste zuerst).
//!  - Jedes Feld hat seinen eigenen Maßstab, an jedem Balken steht der Wert.
//!  - Auf Wunsch wird ein Balken abgeschnitten (mit Bruchstelle), der sonst so
//!    lang wäre, dass man die anderen nicht mehr sähe.

use std::fmt::Write as _;

const FLAECHE: &str = "#fcfcfb";
const TINTE: &str = "#0b0b0b";
const TINTE_2: &str = "#52514e";
const GEDAEMPFT: &str = "#898781";
const LINIE: &str = "#e1e0d9";
const AKZENT: &str = "#2a78d6";
const GRAU: &str = "#c3c2b7";
const SCHRIFT: &str = "system-ui, -apple-system, 'Segoe UI', sans-serif";

const RAND: f64 = 28.0;
const FELD_BREITE: f64 = 420.0;
const FELD_ABSTAND: f64 = 36.0;
const NAMEN_BREITE: f64 = 82.0;
const BALKEN_BREITE: f64 = 200.0;
const ZEILE: f64 = 22.0;
const BALKEN_DICKE: f64 = 12.0;
const KOPF_HOEHE: f64 = 84.0;
const FELD_KOPF: f64 = 46.0;
const FELD_FUSS: f64 = 26.0;

pub struct Balken {
    /// Steht links vom Balken.
    pub name: String,
    /// Bestimmt die Länge.
    pub wert: f64,
    /// Steht rechts vom Balken, zum Beispiel "398 ms".
    pub text: String,
    /// Steht blasser dahinter, zum Beispiel "1,16×".
    pub zusatz: String,
    /// Farbig statt grau.
    pub hervor: bool,
}

pub struct Feld {
    pub titel: String,
    pub unter: String,
    pub balken: Vec<Balken>,
    /// Balken, die mehr als so viel Mal so lang wären wie der kürzeste,
    /// werden abgeschnitten. `None`: nie abschneiden.
    pub abschneiden_ab: Option<f64>,
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn text(aus: &mut String, x: f64, y: f64, groesse: f64, farbe: &str, extra: &str, inhalt: &str) {
    let _ = writeln!(
        aus,
        r#"<text x="{x:.1}" y="{y:.1}" font-size="{groesse}" fill="{farbe}"{extra}>{}</text>"#,
        xml(inhalt)
    );
}

fn feld(aus: &mut String, x: f64, y: f64, feld: &Feld) {
    text(aus, x, y + 16.0, 14.0, TINTE, r#" font-weight="600""#, &feld.titel);
    text(aus, x, y + 33.0, 11.5, TINTE_2, "", &feld.unter);

    let kuerzester = feld.balken.iter().map(|b| b.wert).fold(f64::INFINITY, f64::min);
    let grenze = feld.abschneiden_ab.map_or(f64::INFINITY, |faktor| kuerzester * faktor);
    let groesster = feld.balken.iter().map(|b| b.wert).filter(|w| *w <= grenze).fold(0.0, f64::max);
    let links = x + NAMEN_BREITE;

    for (i, b) in feld.balken.iter().enumerate() {
        let mitte = y + FELD_KOPF + ZEILE * i as f64 + ZEILE / 2.0;
        let oben = mitte - BALKEN_DICKE / 2.0;
        let abgeschnitten = b.wert > grenze;
        let breite = if abgeschnitten { BALKEN_BREITE } else { (b.wert / groesster * BALKEN_BREITE).max(2.0) };
        let farbe = if b.hervor { AKZENT } else { GRAU };
        let gewicht = if b.hervor { r#" font-weight="600""# } else { "" };

        let _ = writeln!(aus, "<g><title>{}: {} {}</title>", xml(&b.name), xml(&b.text), xml(&b.zusatz));
        text(aus, x, mitte + 4.0, 12.0, TINTE, gewicht, &b.name);
        // Am Anfang eckig, am Ende mit 4 px gerundet.
        let r = 4.0f64.min(breite / 2.0);
        let _ = writeln!(
            aus,
            r#"<path d="M{links:.1},{oben:.1} h{:.1} a{r},{r} 0 0 1 {r},{r} v{:.1} a{r},{r} 0 0 1 -{r},{r} h-{:.1} z" fill="{farbe}"/>"#,
            breite - r,
            BALKEN_DICKE - 2.0 * r,
            breite - r,
        );
        if abgeschnitten {
            // Bruchstelle: zwei schräge Lücken in der Farbe der Fläche.
            for versatz in [26.0, 18.0] {
                let s = links + breite - versatz;
                let _ = writeln!(
                    aus,
                    r#"<path d="M{:.1},{:.1} h3.5 l-4,{:.1} h-3.5 z" fill="{FLAECHE}"/>"#,
                    s + 4.0,
                    oben - 1.0,
                    BALKEN_DICKE + 2.0,
                );
            }
        }
        let _ = writeln!(
            aus,
            r#"<text x="{:.1}" y="{:.1}" font-size="12" fill="{TINTE}"{gewicht}>{}<tspan fill="{GEDAEMPFT}" font-weight="400">{}</tspan></text>"#,
            links + breite + 8.0,
            mitte + 4.0,
            xml(&b.text),
            xml(&if b.zusatz.is_empty() { String::new() } else { format!("  ·  {}", b.zusatz) }),
        );
        let _ = writeln!(aus, "</g>");
    }
    // Die Grundlinie, an der alle Balken anfangen.
    let _ = writeln!(
        aus,
        r#"<line x1="{links:.1}" y1="{:.1}" x2="{links:.1}" y2="{:.1}" stroke="{GEDAEMPFT}" stroke-width="1"/>"#,
        y + FELD_KOPF - 2.0,
        y + FELD_KOPF + ZEILE * feld.balken.len() as f64 + 2.0,
    );
}

/// Zeichnet alle Felder in zwei Spalten.
pub fn zeichnen(titel: &str, unter: &str, fuss: &[String], felder: &[Feld]) -> String {
    let zeilen = felder.iter().map(|f| f.balken.len()).max().unwrap_or(0);
    let feld_hoehe = FELD_KOPF + ZEILE * zeilen as f64 + FELD_FUSS;
    let reihen = felder.len().div_ceil(2);
    let breite = RAND * 2.0 + FELD_BREITE * 2.0 + FELD_ABSTAND;
    let fuss_hoehe = 18.0 * fuss.len() as f64 + 16.0;
    let hoehe = KOPF_HOEHE + feld_hoehe * reihen as f64 + fuss_hoehe;

    let mut aus = String::new();
    let _ = writeln!(
        aus,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {breite} {hoehe:.0}" width="{breite}" height="{hoehe:.0}" font-family="{SCHRIFT}" role="img" aria-label="{}">"#,
        xml(titel)
    );
    let _ = writeln!(aus, "<!-- Erzeugt vom Messwerkzeug (leistung/messen). Nicht von Hand ändern. -->");
    let _ = writeln!(
        aus,
        r#"<rect x="0.5" y="0.5" width="{:.0}" height="{:.0}" rx="10" fill="{FLAECHE}" stroke="{LINIE}"/>"#,
        breite - 1.0,
        hoehe - 1.0
    );
    text(&mut aus, RAND, 38.0, 20.0, TINTE, r#" font-weight="600""#, titel);
    text(&mut aus, RAND, 60.0, 12.5, TINTE_2, "", unter);

    // Legende rechts oben: was farbig ist und was grau.
    let legende = breite - RAND - 190.0;
    for (i, (farbe, name)) in [(AKZENT, "OIDA"), (GRAU, "andere Sprachen")].iter().enumerate() {
        let lx = legende + 70.0 * i as f64;
        let _ = writeln!(aus, r#"<rect x="{lx:.1}" y="27" width="12" height="12" rx="3" fill="{farbe}"/>"#);
        text(&mut aus, lx + 18.0, 37.5, 12.0, TINTE_2, "", name);
    }

    for (i, f) in felder.iter().enumerate() {
        let spalte = (i % 2) as f64;
        let reihe = (i / 2) as f64;
        let x = RAND + spalte * (FELD_BREITE + FELD_ABSTAND);
        let y = KOPF_HOEHE + reihe * feld_hoehe;
        if i >= 2 && i % 2 == 0 {
            // Feine Trennlinie zwischen den Reihen.
            let _ = writeln!(
                aus,
                r#"<line x1="{RAND}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{LINIE}" stroke-width="1"/>"#,
                y - 14.0,
                breite - RAND,
                y - 14.0
            );
        }
        feld(&mut aus, x, y, f);
    }

    let fuss_y = KOPF_HOEHE + feld_hoehe * reihen as f64 + 6.0;
    for (i, zeile) in fuss.iter().enumerate() {
        text(&mut aus, RAND, fuss_y + 18.0 * i as f64, 11.5, GEDAEMPFT, "", zeile);
    }
    aus.push_str("</svg>\n");
    aus
}
