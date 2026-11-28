//! JSON lesen und schreiben (für `OIS JSON(wert)` und `AUS JSON VON Typ(text)`).
//!
//! Die Laufzeit weiß nicht, welche Strukturen ein Programm hat. Der Compiler
//! gibt darum zu jedem Wert eine Beschreibung seines Typs mit ([`JsonTyp`]):
//! was es ist, wie groß, und bei einer Struktur, welches Feld wo liegt. Damit
//! geht die Laufzeit durch den Speicher und schreibt JSON, oder sie baut aus
//! JSON den Wert genau so in den Speicher, wie der erzeugte Code ihn erwartet.
//!
//! Beim Lesen wird erst alles geprüft und dann erst gebaut: So bleibt bei
//! einem Fehler nichts Halbfertiges zurück, das wieder freigegeben werden müsste.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use super::{
    ListenKopf, TextKopf, WbKopf, abbruch, bytes, neuer_text, oida_liste_neu, oida_wb_dazu, oida_wb_finde, oida_wb_neu,
    text_behalten, text_loslassen,
};

// Die Arten von Typen. Die Nummern müssen zu denen im Compiler passen (irgen.rs).
pub const ART_ZOI: i64 = 0;
pub const ART_KOMMAZOI: i64 = 1;
pub const ART_JONA: i64 = 2;
pub const ART_KETTN: i64 = 3;
pub const ART_STRUKTUR: i64 = 4;
pub const ART_LISTN: i64 = 5;
/// `VIELLEICHT` um etwas, das ein Zeiger ist (Text, Liste, Verzeichnis): NIX ist der Nullzeiger.
pub const ART_VIELLEICHT_ZEIGER: i64 = 6;
/// `VIELLEICHT` um einen direkten Wert: vorne ein Merker "ist da", dahinter (ab `anzahl` Bytes) der Wert.
pub const ART_VIELLEICHT_WERT: i64 = 7;
/// `VAZEICHNIS VON KETTN UND <typ>`
pub const ART_VAZEICHNIS: i64 = 8;
/// Die Struktur `Json` aus der Bibliothek: JSON, von dem man den Aufbau nicht vorher kennt.
/// Ihre sieben Felder stehen in `felder`: oat, is_woahr, gaunze, komma, inhalt, teile, nama.
pub const ART_BELIEBIG: i64 = 9;
/// Ein Objekt (ein Zeiger). Das kommt nur beim Vergleichen vor: als JSON gehen Objekte nicht.
pub const ART_OBJEKT: i64 = 10;
/// Eine Aufzählung: im Speicher die Nummer des Werts (64 Bit), im JSON sein Name als Text.
/// Die Namen stehen in `felder`, in der Reihenfolge der Nummern.
pub const ART_AUFZAEHLUNG: i64 = 11;

// Was ein `Json`-Wert sein kann (das Feld `oat`); steht so auch in bibliothek/json.oida.
const OAT_NIX: i64 = 0;
const OAT_JONA: i64 = 1;
const OAT_ZOI: i64 = 2;
const OAT_KOMMAZOI: i64 = 3;
const OAT_TEXT: i64 = 4;
const OAT_LISTN: i64 = 5;
const OAT_OBJEKT: i64 = 6;

/// Tiefer verschachtelt geht es nicht (schützt vor Kreisen und vor einem Überlauf des Stapels).
const HOECHSTE_TIEFE: usize = 400;

/// Ein Feld einer Struktur.
#[repr(C)]
pub struct JsonFeld {
    /// Der Name, ein fester Text.
    pub name: *const TextKopf,
    /// Wo das Feld in der Struktur liegt, in Bytes ab ihrem Anfang.
    pub stelle: i64,
    pub typ: *const JsonTyp,
}

/// Die Beschreibung eines Typs, wie der Compiler sie ins Programm legt.
#[repr(C)]
pub struct JsonTyp {
    pub art: i64,
    /// Wie viele Bytes ein Wert dieses Typs braucht.
    pub groesse: i64,
    /// Struktur: Anzahl der Felder. `VIELLEICHT` um einen direkten Wert: wo der Wert hinter dem Merker liegt.
    pub anzahl: i64,
    /// Liste: der Typ der Elemente. `VIELLEICHT`: was da sein kann. Verzeichnis: der Typ der Werte.
    pub innen: *const JsonTyp,
    pub felder: *const JsonFeld,
}

impl JsonTyp {
    pub(crate) unsafe fn felder(&self) -> &[JsonFeld] {
        unsafe { std::slice::from_raw_parts(self.felder, self.anzahl as usize) }
    }

    pub(crate) unsafe fn innen(&self) -> &JsonTyp {
        unsafe { &*self.innen }
    }
}

/// JSON, wie es nach dem Lesen im Speicher steht. Texte ohne `\` darin zeigen
/// einfach in den gelesenen Text hinein: Das spart bei großem JSON Millionen Kopien.
#[derive(Debug, PartialEq)]
enum Wert<'a> {
    Nix,
    Jona(bool),
    /// Eine Zahl ohne Komma und ohne Hochzahl, die in 64 Bit passt.
    Ganz(i64),
    Komma(f64),
    Text(Cow<'a, str>),
    Liste(Vec<Wert<'a>>),
    /// Die Einträge in der Reihenfolge, in der sie dastehen.
    Objekt(Vec<(Cow<'a, str>, Wert<'a>)>),
}

// ------------------------------------------------------------------- Lesen

struct Leser<'a> {
    text: &'a [u8],
    stelle: usize,
}

impl<'a> Leser<'a> {
    /// Eine Meldung mit der Zeile und Spalte, an der der Leser gerade steht.
    fn fehler(&self, meldung: &str) -> String {
        let davor = &self.text[..self.stelle.min(self.text.len())];
        let zeile = davor.iter().filter(|b| **b == b'\n').count() + 1;
        let anfang = davor.iter().rposition(|b| *b == b'\n').map_or(0, |n| n + 1);
        let spalte = String::from_utf8_lossy(&davor[anfang..]).chars().count() + 1;
        format!("Zeile {zeile}, Spoitn {spalte}: {meldung}")
    }

    fn zu_frueh(&self) -> String {
        self.fehler("Des JSON hört z'fria auf.")
    }

    /// Das Zeichen, an dem der Leser steht (ganz, auch wenn es mehrere Bytes hat).
    fn zeichen(&self) -> String {
        let rest = String::from_utf8_lossy(&self.text[self.stelle..]);
        rest.chars().next().map(String::from).unwrap_or_default()
    }

    fn leerraum(&mut self) {
        while matches!(self.text.get(self.stelle), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.stelle += 1;
        }
    }

    fn wert(&mut self, tiefe: usize) -> Result<Wert<'a>, String> {
        if tiefe > HOECHSTE_TIEFE {
            return Err(self.fehler("Des JSON is z'tiaf verschachtelt."));
        }
        self.leerraum();
        match self.text.get(self.stelle) {
            None => Err(self.zu_frueh()),
            Some(b'{') => {
                self.stelle += 1;
                // Platz für acht Einträge gleich am Anfang: Die meisten Objekte kommen damit aus.
                let mut eintraege = Vec::with_capacity(8);
                self.leerraum();
                if self.text.get(self.stelle) == Some(&b'}') {
                    self.stelle += 1;
                    return Ok(Wert::Objekt(eintraege));
                }
                loop {
                    self.leerraum();
                    match self.text.get(self.stelle) {
                        Some(b'"') => self.stelle += 1,
                        None => return Err(self.zu_frueh()),
                        Some(_) => return Err(self.fehler("Do ghört a Nom in Anführungszeichn her.")),
                    }
                    let name = self.text()?;
                    self.leerraum();
                    match self.text.get(self.stelle) {
                        Some(b':') => self.stelle += 1,
                        None => return Err(self.zu_frueh()),
                        Some(_) => return Err(self.fehler("Do ghört a ':' her.")),
                    }
                    eintraege.push((name, self.wert(tiefe + 1)?));
                    self.leerraum();
                    match self.text.get(self.stelle) {
                        Some(b',') => self.stelle += 1,
                        Some(b'}') => {
                            self.stelle += 1;
                            return Ok(Wert::Objekt(eintraege));
                        }
                        None => return Err(self.zu_frueh()),
                        Some(_) => return Err(self.fehler("Do ghört a ',' oda a '}' her.")),
                    }
                }
            }
            Some(b'[') => {
                self.stelle += 1;
                let mut elemente = Vec::new();
                self.leerraum();
                if self.text.get(self.stelle) == Some(&b']') {
                    self.stelle += 1;
                    return Ok(Wert::Liste(elemente));
                }
                loop {
                    elemente.push(self.wert(tiefe + 1)?);
                    self.leerraum();
                    match self.text.get(self.stelle) {
                        Some(b',') => self.stelle += 1,
                        Some(b']') => {
                            self.stelle += 1;
                            return Ok(Wert::Liste(elemente));
                        }
                        None => return Err(self.zu_frueh()),
                        Some(_) => return Err(self.fehler("Do ghört a ',' oda a ']' her.")),
                    }
                }
            }
            Some(b'"') => {
                self.stelle += 1;
                Ok(Wert::Text(self.text()?))
            }
            Some(b'-' | b'0'..=b'9') => self.zahl(),
            Some(_) => {
                for (wort, wert) in [("true", Wert::Jona(true)), ("false", Wert::Jona(false)), ("null", Wert::Nix)] {
                    if self.text[self.stelle..].starts_with(wort.as_bytes()) {
                        self.stelle += wort.len();
                        return Ok(wert);
                    }
                }
                Err(self.fehler(&format!("Mit '{}' kaun i do nix aufaunga.", self.zeichen())))
            }
        }
    }

    /// Ein Text; das Anführungszeichen am Anfang ist schon gelesen.
    fn text(&mut self) -> Result<Cow<'a, str>, String> {
        let text: &'a [u8] = self.text;
        let anfang = self.stelle;
        // Der gewöhnliche Fall: Bis zum Anführungszeichen ist nichts zu übersetzen.
        let glatt = text[anfang..].iter().position(|b| matches!(b, b'"' | b'\\' | 0..=0x1F));
        self.stelle = anfang + glatt.unwrap_or(text.len() - anfang);
        if text.get(self.stelle) == Some(&b'"') {
            self.stelle += 1;
            // Der Text kommt aus einer KETTN und ist damit UTF-8; "lossy" ist nur zur Sicherheit.
            return Ok(String::from_utf8_lossy(&text[anfang..self.stelle - 1]));
        }
        let mut aus: Vec<u8> = text[anfang..self.stelle].to_vec();
        loop {
            let Some(&b) = self.text.get(self.stelle) else {
                return Err(self.fehler("Der Text hört nimma auf: Do fehlt des \" am End."));
            };
            self.stelle += 1;
            match b {
                b'"' => break,
                b'\\' => {
                    let Some(&art) = self.text.get(self.stelle) else {
                        return Err(self.zu_frueh());
                    };
                    self.stelle += 1;
                    match art {
                        b'"' | b'\\' | b'/' => aus.push(art),
                        b'b' => aus.push(8),
                        b'f' => aus.push(12),
                        b'n' => aus.push(b'\n'),
                        b'r' => aus.push(b'\r'),
                        b't' => aus.push(b'\t'),
                        b'u' => {
                            let mut zeichen = self.vier_hex()?;
                            // Zeichen jenseits von U+FFFF stehen als Paar da (\uD83D\uDE00).
                            if (0xD800..0xDC00).contains(&zeichen) && self.text[self.stelle..].starts_with(b"\\u") {
                                let merk = self.stelle;
                                self.stelle += 2;
                                let zweites = self.vier_hex()?;
                                if (0xDC00..0xE000).contains(&zweites) {
                                    zeichen = 0x10000 + ((zeichen - 0xD800) << 10) + (zweites - 0xDC00);
                                } else {
                                    self.stelle = merk;
                                }
                            }
                            // Eine halbe Paarung allein gibt es als Zeichen nicht: Dafür steht das Ersatzzeichen.
                            let zeichen = char::from_u32(zeichen).unwrap_or('\u{FFFD}');
                            aus.extend_from_slice(zeichen.encode_utf8(&mut [0; 4]).as_bytes());
                        }
                        _ => {
                            self.stelle -= 1;
                            return Err(self.fehler(&format!("'\\{}' gibt's in JSON ned.", self.zeichen())));
                        }
                    }
                }
                0..=0x1F => {
                    self.stelle -= 1;
                    return Err(self.fehler("In an Text derf ka Zeilenend und ka Steuerzeichn stehn (schreib \\n)."));
                }
                _ => aus.push(b),
            }
        }
        Ok(Cow::Owned(String::from_utf8(aus).unwrap_or_else(|f| String::from_utf8_lossy(f.as_bytes()).into_owned())))
    }

    fn vier_hex(&mut self) -> Result<u32, String> {
        let stueck = self.text.get(self.stelle..self.stelle + 4).ok_or_else(|| self.zu_frueh())?;
        let zahl = std::str::from_utf8(stueck).ok().filter(|s| s.bytes().all(|b| b.is_ascii_hexdigit()));
        match zahl.and_then(|s| u32::from_str_radix(s, 16).ok()) {
            Some(zahl) => {
                self.stelle += 4;
                Ok(zahl)
            }
            None => Err(self.fehler("Noch \\u ghörn vier Ziffan (0-9, a-f).")),
        }
    }

    fn zahl(&mut self) -> Result<Wert<'a>, String> {
        let anfang = self.stelle;
        let ziffern = |l: &mut Self| {
            let vorher = l.stelle;
            while matches!(l.text.get(l.stelle), Some(b'0'..=b'9')) {
                l.stelle += 1;
            }
            l.stelle - vorher
        };
        if self.text.get(self.stelle) == Some(&b'-') {
            self.stelle += 1;
        }
        let erste = self.text.get(self.stelle).copied();
        let vor_komma = ziffern(self);
        if vor_komma == 0 {
            return Err(self.fehler("Do ghört a Ziffa her."));
        }
        if vor_komma > 1 && erste == Some(b'0') {
            self.stelle = anfang;
            return Err(self.fehler("A Zoi derf ned mit ana Null aufaunga (außa sie is 0 oda 0,irgendwos)."));
        }
        let mut ganz = true;
        if self.text.get(self.stelle) == Some(&b'.') {
            ganz = false;
            self.stelle += 1;
            if ziffern(self) == 0 {
                return Err(self.fehler("Noch'm Punkt ghört a Ziffa her."));
            }
        }
        if matches!(self.text.get(self.stelle), Some(b'e' | b'E')) {
            ganz = false;
            self.stelle += 1;
            if matches!(self.text.get(self.stelle), Some(b'+' | b'-')) {
                self.stelle += 1;
            }
            if ziffern(self) == 0 {
                return Err(self.fehler("Noch'm e ghört a Ziffa her."));
            }
        }
        let geschrieben = std::str::from_utf8(&self.text[anfang..self.stelle]).unwrap_or("0");
        if ganz && let Ok(zahl) = geschrieben.parse::<i64>() {
            return Ok(Wert::Ganz(zahl));
        }
        // Zu groß für 64 Bit, oder mit Komma oder Hochzahl: eine Kommazahl.
        Ok(Wert::Komma(geschrieben.parse::<f64>().unwrap_or(0.0)))
    }
}

/// Macht aus einem JSON-Text den Baum der Werte.
fn lies(text: &[u8]) -> Result<Wert<'_>, String> {
    let mut leser = Leser { text, stelle: 0 };
    let wert = leser.wert(0)?;
    leser.leerraum();
    if leser.stelle < text.len() {
        return Err(leser.fehler("Do is des JSON scho aus, oba es kummt no wos."));
    }
    Ok(wert)
}

// ------------------------------------------------------- Prüfen und Bauen

/// Wie man sagt, was an einer Stelle erwartet wird.
unsafe fn erwartet(typ: &JsonTyp) -> &'static str {
    match typ.art {
        ART_ZOI => "a gaunze Zoi",
        ART_KOMMAZOI => "a Zoi",
        ART_JONA => "true oda false",
        ART_KETTN => "a Text",
        ART_AUFZAEHLUNG => "a Text mit an von de Werte",
        ART_STRUKTUR | ART_VAZEICHNIS => "a Objekt ({...})",
        ART_LISTN => "a Listn ([...])",
        ART_VIELLEICHT_ZEIGER | ART_VIELLEICHT_WERT => unsafe { erwartet(typ.innen()) },
        _ => "irgendwos",
    }
}

fn gefunden(wert: &Wert) -> &'static str {
    match wert {
        Wert::Nix => "null",
        Wert::Jona(true) => "true",
        Wert::Jona(false) => "false",
        Wert::Ganz(_) => "a gaunze Zoi",
        Wert::Komma(_) => "a Kommazoi",
        Wert::Text(_) => "a Text",
        Wert::Liste(_) => "a Listn",
        Wert::Objekt(_) => "a Objekt",
    }
}

/// Der Text hinter einem Zeiger auf einen festen Text (ein Feldname, vom Compiler und damit UTF-8).
unsafe fn name_von(feld: &JsonFeld) -> &str {
    std::str::from_utf8(unsafe { bytes(feld.name) }).unwrap_or("?")
}

/// Der letzte Eintrag mit diesem Namen (steht ein Name doppelt da, gilt der spätere).
fn eintrag<'b, 'a>(eintraege: &'b [(Cow<'a, str>, Wert<'a>)], name: &str) -> Option<&'b Wert<'a>> {
    eintraege.iter().rev().find(|(n, _)| n == name).map(|(_, w)| w)
}

/// Ein Schritt auf dem Weg zu einer Stelle im JSON.
enum Glied {
    Name(String),
    Stelle(usize),
}

/// Was beim Prüfen nicht gepasst hat. Der Weg dorthin wird erst beim Zurückgehen
/// gesammelt (von innen nach außen): Solange alles passt, kostet er nichts.
struct Daneben {
    weg: Vec<Glied>,
    /// Was nicht passt; `None` heißt: Da fehlt etwas.
    was: Option<String>,
}

impl Daneben {
    fn ueber(mut self, glied: Glied) -> Daneben {
        self.weg.push(glied);
        self
    }

    /// Die Meldung, mit dem Weg so, wie man ihn auch in JavaScript schriebe: `spiela[0].wo.x`.
    fn meldung(&self) -> String {
        let mut pfad = String::new();
        for glied in self.weg.iter().rev() {
            match glied {
                Glied::Name(name) => {
                    if !pfad.is_empty() {
                        pfad.push('.');
                    }
                    pfad.push_str(name);
                }
                Glied::Stelle(i) => {
                    let _ = write!(pfad, "[{i}]");
                }
            }
        }
        match &self.was {
            None => format!("Im JSON fehlt '{pfad}'."),
            Some(was) if pfad.is_empty() => was.clone(),
            Some(was) => format!("Bei '{pfad}': {was}"),
        }
    }
}

/// Passt das JSON zum Typ? Gebaut wird hier noch nichts.
unsafe fn pruefe(wert: &Wert, typ: &JsonTyp) -> Result<(), Daneben> {
    unsafe {
        match (typ.art, wert) {
            (ART_BELIEBIG, _) => Ok(()),
            (ART_ZOI, Wert::Ganz(_)) => Ok(()),
            // 3.0 ist auch eine ganze Zahl; JSON unterscheidet das nicht.
            (ART_ZOI, Wert::Komma(k)) if k.fract() == 0.0 && k.abs() < 9.0e18 => Ok(()),
            (ART_KOMMAZOI, Wert::Ganz(_) | Wert::Komma(_)) => Ok(()),
            (ART_JONA, Wert::Jona(_)) => Ok(()),
            (ART_KETTN, Wert::Text(_)) => Ok(()),
            (ART_AUFZAEHLUNG, Wert::Text(text)) => {
                if typ.felder().iter().any(|feld| name_von(feld) == text.as_ref()) {
                    return Ok(());
                }
                let alle: Vec<&str> = typ.felder().iter().map(|feld| name_von(feld)).collect();
                Err(Daneben { weg: Vec::new(), was: Some(format!("'{text}' gibt's do ned. Es gibt: {}.", alle.join(", "))) })
            }
            (ART_VIELLEICHT_ZEIGER | ART_VIELLEICHT_WERT, Wert::Nix) => Ok(()),
            (ART_VIELLEICHT_ZEIGER | ART_VIELLEICHT_WERT, _) => pruefe(wert, typ.innen()),
            (ART_LISTN, Wert::Liste(elemente)) => {
                let innen = typ.innen();
                for (i, element) in elemente.iter().enumerate() {
                    pruefe(element, innen).map_err(|d| d.ueber(Glied::Stelle(i)))?;
                }
                Ok(())
            }
            (ART_VAZEICHNIS, Wert::Objekt(eintraege)) => {
                let innen = typ.innen();
                for (name, wert) in eintraege {
                    pruefe(wert, innen).map_err(|d| d.ueber(Glied::Name(name.to_string())))?;
                }
                Ok(())
            }
            (ART_STRUKTUR, Wert::Objekt(eintraege)) => {
                for feld in typ.felder() {
                    let name = name_von(feld);
                    match eintrag(eintraege, name) {
                        Some(innen) => pruefe(innen, &*feld.typ).map_err(|d| d.ueber(Glied::Name(name.to_string())))?,
                        // Was fehlen darf, fehlt eben; alles andere muss dastehen.
                        None if matches!((*feld.typ).art, ART_VIELLEICHT_ZEIGER | ART_VIELLEICHT_WERT | ART_BELIEBIG) => {}
                        None => return Err(Daneben { weg: vec![Glied::Name(name.to_string())], was: None }),
                    }
                }
                Ok(())
            }
            _ => Err(Daneben {
                weg: Vec::new(),
                was: Some(format!("Do ghört {} her, im JSON steht oba {}.", erwartet(typ), gefunden(wert))),
            }),
        }
    }
}

/// Eine neue Liste mit Platz für genau `anzahl` Elemente dieser Größe; die Länge steht schon drin.
unsafe fn neue_liste(groesse: i64, anzahl: usize) -> *mut ListenKopf {
    let liste = oida_liste_neu(groesse, anzahl as i64);
    unsafe { (*liste).laenge = anzahl as i64 };
    liste
}

/// Schreibt den Wert so nach `ziel`, wie der erzeugte Code ihn dort erwartet.
/// Davor muss [`pruefe`] gesagt haben, dass alles passt.
unsafe fn baue(wert: &Wert, typ: &JsonTyp, ziel: *mut u8) {
    unsafe {
        match (typ.art, wert) {
            (ART_BELIEBIG, _) => baue_beliebig(wert, typ, ziel, &mut HashMap::new()),
            (ART_ZOI, Wert::Ganz(z)) => (ziel as *mut i64).write_unaligned(*z),
            (ART_ZOI, Wert::Komma(k)) => (ziel as *mut i64).write_unaligned(*k as i64),
            (ART_KOMMAZOI, Wert::Ganz(z)) => (ziel as *mut f64).write_unaligned(*z as f64),
            (ART_KOMMAZOI, Wert::Komma(k)) => (ziel as *mut f64).write_unaligned(*k),
            (ART_JONA, Wert::Jona(j)) => ziel.write(*j as u8),
            (ART_KETTN, Wert::Text(t)) => (ziel as *mut *mut TextKopf).write_unaligned(neuer_text(t.as_bytes())),
            (ART_AUFZAEHLUNG, Wert::Text(text)) => {
                let nummer = typ.felder().iter().position(|feld| name_von(feld) == text.as_ref()).unwrap_or(0);
                (ziel as *mut i64).write_unaligned(nummer as i64);
            }
            (ART_VIELLEICHT_ZEIGER | ART_VIELLEICHT_WERT, Wert::Nix) => std::ptr::write_bytes(ziel, 0, typ.groesse as usize),
            (ART_VIELLEICHT_ZEIGER, _) => baue(wert, typ.innen(), ziel),
            (ART_VIELLEICHT_WERT, _) => {
                std::ptr::write_bytes(ziel, 0, typ.groesse as usize);
                ziel.write(1);
                baue(wert, typ.innen(), ziel.add(typ.anzahl as usize));
            }
            (ART_LISTN, Wert::Liste(elemente)) => {
                let innen = typ.innen();
                let liste = neue_liste(innen.groesse, elemente.len());
                for (i, element) in elemente.iter().enumerate() {
                    baue(element, innen, (*liste).daten.add(i * innen.groesse as usize));
                }
                (ziel as *mut *mut ListenKopf).write_unaligned(liste);
            }
            (ART_VAZEICHNIS, Wert::Objekt(eintraege)) => {
                let innen = typ.innen();
                let verzeichnis = oida_wb_neu(1);
                // Steht ein Name doppelt da, gilt der spätere: von hinten merken, welche schon da waren.
                let mut gesehen = HashSet::with_capacity(eintraege.len());
                let mut gilt: Vec<bool> = eintraege.iter().rev().map(|(name, _)| gesehen.insert(name.as_ref())).collect();
                gilt.reverse();
                for ((name, wert), _) in eintraege.iter().zip(&gilt).filter(|(_, gilt)| **gilt) {
                    let schluessel = neuer_text(name.as_bytes());
                    let stelle = oida_wb_dazu(verzeichnis, schluessel as i64, innen.groesse);
                    // Das Verzeichnis zählt den Text jetzt selbst mit.
                    text_loslassen(schluessel);
                    baue(wert, innen, (*verzeichnis).werte.add(stelle as usize * innen.groesse as usize));
                }
                (ziel as *mut *mut WbKopf).write_unaligned(verzeichnis);
            }
            (ART_STRUKTUR, Wert::Objekt(eintraege)) => {
                std::ptr::write_bytes(ziel, 0, typ.groesse as usize);
                for feld in typ.felder() {
                    let platz = ziel.add(feld.stelle as usize);
                    match eintrag(eintraege, name_von(feld)) {
                        Some(innen) => baue(innen, &*feld.typ, platz),
                        // Fehlt etwas, das fehlen darf: NIX.
                        None => baue(&Wert::Nix, &*feld.typ, platz),
                    }
                }
            }
            _ => unreachable!("vor dem Bauen wird geprüft"),
        }
    }
}

/// Baut einen `Json`-Wert aus der Bibliothek: JSON, wie es dasteht, ohne festen Aufbau.
///
/// In `namen` stehen die Namen von Einträgen, die schon vorgekommen sind: In einer
/// Liste von tausend Objekten heißen die Einträge tausendmal gleich, und so teilen
/// sich alle denselben Text, statt dass jeder seinen eigenen bekommt.
unsafe fn baue_beliebig<'b>(wert: &'b Wert, typ: &JsonTyp, ziel: *mut u8, namen: &mut HashMap<&'b str, *mut TextKopf>) {
    unsafe {
        let felder = typ.felder();
        let platz = |nummer: usize| ziel.add(felder[nummer].stelle as usize);
        std::ptr::write_bytes(ziel, 0, typ.groesse as usize);
        let oat = match wert {
            Wert::Nix => OAT_NIX,
            Wert::Jona(j) => {
                platz(1).write(*j as u8);
                OAT_JONA
            }
            Wert::Ganz(z) => {
                (platz(2) as *mut i64).write_unaligned(*z);
                (platz(3) as *mut f64).write_unaligned(*z as f64);
                OAT_ZOI
            }
            Wert::Komma(k) => {
                (platz(3) as *mut f64).write_unaligned(*k);
                OAT_KOMMAZOI
            }
            Wert::Text(t) => {
                (platz(4) as *mut *mut TextKopf).write_unaligned(neuer_text(t.as_bytes()));
                OAT_TEXT
            }
            Wert::Liste(elemente) => {
                let liste = neue_liste(typ.groesse, elemente.len());
                for (i, element) in elemente.iter().enumerate() {
                    baue_beliebig(element, typ, (*liste).daten.add(i * typ.groesse as usize), namen);
                }
                (platz(5) as *mut *mut ListenKopf).write_unaligned(liste);
                OAT_LISTN
            }
            Wert::Objekt(eintraege) => {
                let liste = neue_liste(typ.groesse, eintraege.len());
                for (i, (_, innen)) in eintraege.iter().enumerate() {
                    baue_beliebig(innen, typ, (*liste).daten.add(i * typ.groesse as usize), namen);
                }
                (platz(5) as *mut *mut ListenKopf).write_unaligned(liste);
                let namensliste = neue_liste(8, eintraege.len());
                for (i, (name, _)) in eintraege.iter().enumerate() {
                    let text = match namen.get(name.as_ref()) {
                        Some(&text) => {
                            text_behalten(text);
                            text
                        }
                        None => {
                            let text = neuer_text(name.as_bytes());
                            namen.insert(name.as_ref(), text);
                            text
                        }
                    };
                    ((*namensliste).daten as *mut *mut TextKopf).add(i).write(text);
                }
                (platz(6) as *mut *mut ListenKopf).write_unaligned(namensliste);
                OAT_OBJEKT
            }
        };
        (platz(0) as *mut i64).write_unaligned(oat);
    }
}

/// `AUS JSON VON Typ(text)`: liest den Text und baut den Wert nach `ziel`.
/// Gibt null zurück, wenn es geklappt hat, sonst die Meldung als neuen Text
/// (dann bleibt `ziel`, wie es war).
///
/// # Safety
/// `text` muss ein gültiger Text sein, `typ` eine Beschreibung vom Compiler
/// und `ziel` Platz für einen Wert dieses Typs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_json_lesen(text: *const TextKopf, typ: *const JsonTyp, ziel: *mut u8) -> *mut TextKopf {
    unsafe {
        let typ = &*typ;
        let gelesen = lies(bytes(text)).and_then(|wert| pruefe(&wert, typ).map(|()| wert).map_err(|d| d.meldung()));
        match gelesen {
            Ok(wert) => {
                baue(&wert, typ, ziel);
                std::ptr::null_mut()
            }
            Err(meldung) => neuer_text(format!("JSON: {meldung}").as_bytes()),
        }
    }
}

// -------------------------------------------------------------- Vergleichen

/// `a == b` für Strukturen, Listen und Verzeichnisse: Zwei Werte sind gleich,
/// wenn alles darin gleich ist. Die Beschreibung des Typs ist dieselbe wie für
/// JSON; hier dürfen auch Objekte vorkommen (gleich heißt bei ihnen: dasselbe).
///
/// # Safety
/// `a` und `b` müssen auf Werte des Typs zeigen, den `typ` beschreibt.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_gleich(a: *const u8, b: *const u8, typ: *const JsonTyp, zeile: i64) -> bool {
    unsafe { gleich(a, b, &*typ, 0, zeile) }
}

unsafe fn gleich(a: *const u8, b: *const u8, typ: &JsonTyp, tiefe: usize, zeile: i64) -> bool {
    if tiefe > 10_000 {
        abbruch(zeile, "Des is z'tiaf verschachtelt zum Vergleichn (oda es zagt im Kreis auf si söwa).");
    }
    unsafe {
        let zeiger = |wo: *const u8| (wo as *const *const u8).read_unaligned();
        match typ.art {
            ART_ZOI => (a as *const i64).read_unaligned() == (b as *const i64).read_unaligned(),
            ART_KOMMAZOI => (a as *const f64).read_unaligned() == (b as *const f64).read_unaligned(),
            ART_JONA => (a.read() != 0) == (b.read() != 0),
            ART_KETTN => bytes(zeiger(a).cast()) == bytes(zeiger(b).cast()),
            ART_AUFZAEHLUNG => (a as *const i64).read_unaligned() == (b as *const i64).read_unaligned(),
            ART_OBJEKT => zeiger(a) == zeiger(b),
            ART_VIELLEICHT_ZEIGER => match (zeiger(a).is_null(), zeiger(b).is_null()) {
                (true, true) => true,
                (false, false) => gleich(a, b, typ.innen(), tiefe, zeile),
                _ => false,
            },
            ART_VIELLEICHT_WERT => match (a.read() != 0, b.read() != 0) {
                (false, false) => true,
                (true, true) => gleich(a.add(typ.anzahl as usize), b.add(typ.anzahl as usize), typ.innen(), tiefe, zeile),
                _ => false,
            },
            ART_STRUKTUR => typ.felder().iter().all(|feld| {
                gleich(a.add(feld.stelle as usize), b.add(feld.stelle as usize), &*feld.typ, tiefe + 1, zeile)
            }),
            ART_LISTN => listen_gleich(zeiger(a).cast(), zeiger(b).cast(), typ.innen(), tiefe, zeile),
            ART_VAZEICHNIS => {
                let (va, vb) = (zeiger(a) as *mut WbKopf, zeiger(b) as *mut WbKopf);
                if va == vb {
                    return true;
                }
                let innen = typ.innen();
                let groesse = innen.groesse as usize;
                // Dieselben Schlüssel mit denselben Werten; die Reihenfolge zählt nicht.
                (*va).laenge == (*vb).laenge
                    && (0..(*va).laenge as usize).all(|i| {
                        let stelle = oida_wb_finde(vb, *(*va).schluessel.add(i));
                        stelle >= 0
                            && gleich((*va).werte.add(i * groesse), (*vb).werte.add(stelle as usize * groesse), innen, tiefe + 1, zeile)
                    })
            }
            ART_BELIEBIG => {
                // Die Struktur Json: Von ihren Feldern steht in der Beschreibung nur, wo sie liegen.
                let felder = typ.felder();
                let (pa, pb) = (|n: usize| a.add(felder[n].stelle as usize), |n: usize| b.add(felder[n].stelle as usize));
                let oat = (pa(0) as *const i64).read_unaligned();
                if oat != (pb(0) as *const i64).read_unaligned() {
                    return false;
                }
                match oat {
                    OAT_JONA => (pa(1).read() != 0) == (pb(1).read() != 0),
                    OAT_ZOI => (pa(2) as *const i64).read_unaligned() == (pb(2) as *const i64).read_unaligned(),
                    OAT_KOMMAZOI => (pa(3) as *const f64).read_unaligned() == (pb(3) as *const f64).read_unaligned(),
                    OAT_TEXT => texte_gleich(zeiger(pa(4)).cast(), zeiger(pb(4)).cast()),
                    OAT_LISTN => listen_gleich(zeiger(pa(5)).cast(), zeiger(pb(5)).cast(), typ, tiefe, zeile),
                    // Bei Objekten zählt hier auch die Reihenfolge der Einträge.
                    OAT_OBJEKT => {
                        let (na, nb): (*const ListenKopf, *const ListenKopf) = (zeiger(pa(6)).cast(), zeiger(pb(6)).cast());
                        let namen_gleich = match (na.is_null(), nb.is_null()) {
                            (true, true) => true,
                            (false, false) => {
                                (*na).laenge == (*nb).laenge
                                    && (0..(*na).laenge as usize).all(|i| {
                                        texte_gleich(
                                            ((*na).daten as *const *const TextKopf).add(i).read(),
                                            ((*nb).daten as *const *const TextKopf).add(i).read(),
                                        )
                                    })
                            }
                            _ => false,
                        };
                        namen_gleich && listen_gleich(zeiger(pa(5)).cast(), zeiger(pb(5)).cast(), typ, tiefe, zeile)
                    }
                    _ => true,
                }
            }
            _ => false,
        }
    }
}

/// Zwei Texte, von denen jeder auch fehlen kann (null).
unsafe fn texte_gleich(a: *const TextKopf, b: *const TextKopf) -> bool {
    match (a.is_null(), b.is_null()) {
        (true, true) => true,
        (false, false) => unsafe { bytes(a) == bytes(b) },
        _ => false,
    }
}

/// Zwei Listen (jede kann auch fehlen): gleich lang und Stelle für Stelle gleich.
unsafe fn listen_gleich(a: *const ListenKopf, b: *const ListenKopf, element: &JsonTyp, tiefe: usize, zeile: i64) -> bool {
    if a == b {
        return true;
    }
    if a.is_null() || b.is_null() {
        return false;
    }
    unsafe {
        let groesse = element.groesse as usize;
        (*a).laenge == (*b).laenge
            && (0..(*a).laenge as usize)
                .all(|i| gleich((*a).daten.add(i * groesse), (*b).daten.add(i * groesse), element, tiefe + 1, zeile))
    }
}

// --------------------------------------------------------------- Schreiben

/// Was JSON schreibt: wohin, und ob schön mit Zeilen und Einrückung.
struct Schreiber {
    aus: String,
    schoen: bool,
    zeile: i64,
}

impl Schreiber {
    fn neue_zeile(&mut self, tiefe: usize) {
        if self.schoen {
            self.aus.push('\n');
            for _ in 0..tiefe {
                self.aus.push_str("  ");
            }
        }
    }

    fn text(&mut self, inhalt: &[u8]) {
        self.aus.push('"');
        for zeichen in String::from_utf8_lossy(inhalt).chars() {
            match zeichen {
                '"' => self.aus.push_str("\\\""),
                '\\' => self.aus.push_str("\\\\"),
                '\n' => self.aus.push_str("\\n"),
                '\r' => self.aus.push_str("\\r"),
                '\t' => self.aus.push_str("\\t"),
                '\u{8}' => self.aus.push_str("\\b"),
                '\u{c}' => self.aus.push_str("\\f"),
                z if (z as u32) < 0x20 => self.aus.push_str(&format!("\\u{:04x}", z as u32)),
                z => self.aus.push(z),
            }
        }
        self.aus.push('"');
    }

    fn kommazahl(&mut self, zahl: f64) {
        // Unendlich und "keine Zahl" kennt JSON nicht.
        if zahl.is_finite() {
            self.aus.push_str(&zahl.to_string());
        } else {
            self.aus.push_str("null");
        }
    }

    /// Schreibt die Elemente zwischen zwei Klammern, mit Beistrichen (und Zeilen, wenn schön).
    fn klammern(&mut self, auf: char, zu: char, anzahl: usize, tiefe: usize, mut element: impl FnMut(&mut Self, usize)) {
        self.aus.push(auf);
        for i in 0..anzahl {
            if i > 0 {
                self.aus.push(',');
            }
            self.neue_zeile(tiefe + 1);
            element(self, i);
        }
        if anzahl > 0 {
            self.neue_zeile(tiefe);
        }
        self.aus.push(zu);
    }

    fn name(&mut self, name: &[u8]) {
        self.text(name);
        self.aus.push_str(if self.schoen { ": " } else { ":" });
    }

    fn zu_tief(&self, tiefe: usize) {
        if tiefe > HOECHSTE_TIEFE {
            abbruch(self.zeile, "Des is z'tiaf verschachtelt fia JSON (oda es zagt im Kreis auf si söwa).");
        }
    }

    unsafe fn wert(&mut self, wert: *const u8, typ: &JsonTyp, tiefe: usize) {
        self.zu_tief(tiefe);
        unsafe {
            match typ.art {
                ART_ZOI => self.aus.push_str(&(wert as *const i64).read_unaligned().to_string()),
                ART_KOMMAZOI => self.kommazahl((wert as *const f64).read_unaligned()),
                ART_JONA => self.aus.push_str(if wert.read() & 1 != 0 { "true" } else { "false" }),
                ART_KETTN => self.text(bytes((wert as *const *const TextKopf).read_unaligned())),
                ART_AUFZAEHLUNG => {
                    let nummer = (wert as *const i64).read_unaligned();
                    match usize::try_from(nummer).ok().and_then(|n| typ.felder().get(n)) {
                        Some(feld) => self.text(bytes(feld.name)),
                        None => self.aus.push_str("null"),
                    }
                }
                ART_VIELLEICHT_ZEIGER => {
                    if (wert as *const *const u8).read_unaligned().is_null() {
                        self.aus.push_str("null");
                    } else {
                        self.wert(wert, typ.innen(), tiefe);
                    }
                }
                ART_VIELLEICHT_WERT => {
                    if wert.read() & 1 == 0 {
                        self.aus.push_str("null");
                    } else {
                        self.wert(wert.add(typ.anzahl as usize), typ.innen(), tiefe);
                    }
                }
                ART_LISTN => {
                    let liste = &*(wert as *const *const ListenKopf).read_unaligned();
                    let innen = typ.innen();
                    self.klammern('[', ']', liste.laenge as usize, tiefe, |s, i| {
                        s.wert(liste.daten.add(i * innen.groesse as usize), innen, tiefe + 1);
                    });
                }
                ART_VAZEICHNIS => {
                    let verzeichnis = &*(wert as *const *const WbKopf).read_unaligned();
                    let innen = typ.innen();
                    self.klammern('{', '}', verzeichnis.laenge as usize, tiefe, |s, i| {
                        s.name(bytes(*verzeichnis.schluessel.add(i) as *const TextKopf));
                        s.wert(verzeichnis.werte.add(i * innen.groesse as usize), innen, tiefe + 1);
                    });
                }
                ART_STRUKTUR => {
                    let felder = typ.felder();
                    self.klammern('{', '}', felder.len(), tiefe, |s, i| {
                        s.name(bytes(felder[i].name));
                        s.wert(wert.add(felder[i].stelle as usize), &*felder[i].typ, tiefe + 1);
                    });
                }
                ART_BELIEBIG => self.beliebig(wert, typ, tiefe),
                _ => self.aus.push_str("null"),
            }
        }
    }

    /// Ein `Json`-Wert aus der Bibliothek.
    unsafe fn beliebig(&mut self, wert: *const u8, typ: &JsonTyp, tiefe: usize) {
        self.zu_tief(tiefe);
        unsafe {
            let felder = typ.felder();
            let platz = |nummer: usize| wert.add(felder[nummer].stelle as usize);
            let liste = |nummer: usize| (platz(nummer) as *const *const ListenKopf).read_unaligned();
            let groesse = typ.groesse as usize;
            match (platz(0) as *const i64).read_unaligned() {
                OAT_JONA => self.aus.push_str(if platz(1).read() & 1 != 0 { "true" } else { "false" }),
                OAT_ZOI => self.aus.push_str(&(platz(2) as *const i64).read_unaligned().to_string()),
                OAT_KOMMAZOI => self.kommazahl((platz(3) as *const f64).read_unaligned()),
                OAT_TEXT => {
                    let text = (platz(4) as *const *const TextKopf).read_unaligned();
                    self.text(if text.is_null() { b"" } else { bytes(text) });
                }
                OAT_LISTN => {
                    let teile = liste(5);
                    let anzahl = if teile.is_null() { 0 } else { (*teile).laenge as usize };
                    self.klammern('[', ']', anzahl, tiefe, |s, i| {
                        s.beliebig((*teile).daten.add(i * groesse), typ, tiefe + 1);
                    });
                }
                OAT_OBJEKT => {
                    let (teile, namen) = (liste(5), liste(6));
                    // Zu jedem Namen gehört ein Wert; was allein dasteht, zählt nicht.
                    let anzahl =
                        if teile.is_null() || namen.is_null() { 0 } else { (*teile).laenge.min((*namen).laenge) as usize };
                    self.klammern('{', '}', anzahl, tiefe, |s, i| {
                        s.name(bytes(*((*namen).daten as *const *const TextKopf).add(i)));
                        s.beliebig((*teile).daten.add(i * groesse), typ, tiefe + 1);
                    });
                }
                _ => self.aus.push_str("null"),
            }
        }
    }
}

/// `OIS JSON(wert)`: der Wert als JSON, in einer Zeile oder (`schoen`) mit Zeilen und Einrückung.
///
/// # Safety
/// `wert` muss auf einen Wert zeigen, zu dem die Beschreibung `typ` passt.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_json_schreiben(wert: *const u8, typ: *const JsonTyp, schoen: bool, zeile: i64) -> *mut TextKopf {
    let mut schreiber = Schreiber { aus: String::new(), schoen, zeile };
    unsafe { schreiber.wert(wert, &*typ, 0) };
    neuer_text(schreiber.aus.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(inhalt: &str) -> *mut TextKopf {
        neuer_text(inhalt.as_bytes())
    }

    fn inhalt(text: *const TextKopf) -> String {
        String::from_utf8(unsafe { bytes(text) }.to_vec()).unwrap()
    }

    /// Eine Beschreibung, die für die Dauer der Tests bleibt.
    fn typ(art: i64, groesse: i64, anzahl: i64, innen: *const JsonTyp, felder: Vec<JsonFeld>) -> &'static JsonTyp {
        let felder = Box::leak(felder.into_boxed_slice());
        Box::leak(Box::new(JsonTyp { art, groesse, anzahl, innen, felder: felder.as_ptr() }))
    }

    fn einfach(art: i64) -> &'static JsonTyp {
        typ(art, if art == ART_JONA { 1 } else { 8 }, 0, std::ptr::null(), Vec::new())
    }

    fn feld(name: &str, stelle: usize, typ: &'static JsonTyp) -> JsonFeld {
        JsonFeld { name: text(name), stelle: stelle as i64, typ }
    }

    /// So liegt `STRUKTUR Spiela { nam KETTN, punkte ZOI, do JONA, tempo KOMMAZOI, tags LISTN VON KETTN, oita VIELLEICHT ZOI, spitznam VIELLEICHT KETTN }` im Speicher.
    #[repr(C)]
    struct Spieler {
        name: *mut TextKopf,
        punkte: i64,
        da: bool,
        tempo: f64,
        tags: *mut ListenKopf,
        alter: (bool, i64),
        spitzname: *mut TextKopf,
    }

    fn spieler_typ() -> &'static JsonTyp {
        let (zoi, kommazoi, jona, kettn) = (einfach(ART_ZOI), einfach(ART_KOMMAZOI), einfach(ART_JONA), einfach(ART_KETTN));
        let tags = typ(ART_LISTN, 8, 0, kettn, Vec::new());
        let alter = typ(ART_VIELLEICHT_WERT, 16, 8, zoi, Vec::new());
        let spitzname = typ(ART_VIELLEICHT_ZEIGER, 8, 0, kettn, Vec::new());
        typ(
            ART_STRUKTUR,
            std::mem::size_of::<Spieler>() as i64,
            7,
            std::ptr::null(),
            vec![
                feld("nam", std::mem::offset_of!(Spieler, name), kettn),
                feld("punkte", std::mem::offset_of!(Spieler, punkte), zoi),
                feld("do", std::mem::offset_of!(Spieler, da), jona),
                feld("tempo", std::mem::offset_of!(Spieler, tempo), kommazoi),
                feld("tags", std::mem::offset_of!(Spieler, tags), tags),
                feld("oita", std::mem::offset_of!(Spieler, alter), alter),
                feld("spitznam", std::mem::offset_of!(Spieler, spitzname), spitzname),
            ],
        )
    }

    fn lies_spieler(json: &str) -> Result<Spieler, String> {
        let mut ziel = std::mem::MaybeUninit::<Spieler>::zeroed();
        let fehler = unsafe { oida_json_lesen(text(json), spieler_typ(), ziel.as_mut_ptr().cast()) };
        if fehler.is_null() { Ok(unsafe { ziel.assume_init() }) } else { Err(inhalt(fehler)) }
    }

    fn schreib<T>(wert: &T, typ: &JsonTyp, schoen: bool) -> String {
        inhalt(unsafe { oida_json_schreiben((wert as *const T).cast(), typ, schoen, 1) })
    }

    #[test]
    fn struktur_lesen_und_wieder_schreiben() {
        let json = r#" { "punkte": 12, "nam": "Hansi \"da Wüde\"\n", "do": true, "tempo": 1.5,
                         "tags": ["a", "ö"], "oita": 17, "spitznam": null, "wos aundas": [1, {"x": 2}] } "#;
        let s = lies_spieler(json).unwrap();
        assert_eq!(inhalt(s.name), "Hansi \"da Wüde\"\n");
        assert_eq!((s.punkte, s.da, s.tempo, s.alter), (12, true, 1.5, (true, 17)));
        assert!(s.spitzname.is_null());
        let tags = unsafe { &*s.tags };
        assert_eq!((tags.laenge, tags.platz), (2, 2));
        let zeiger = unsafe { std::slice::from_raw_parts(tags.daten as *const *const TextKopf, 2) };
        assert_eq!((inhalt(zeiger[0]), inhalt(zeiger[1])), ("a".to_string(), "ö".to_string()));

        // Die Felder kommen in der Reihenfolge der Struktur, was fehlt als null.
        assert_eq!(
            schreib(&s, spieler_typ(), false),
            r#"{"nam":"Hansi \"da Wüde\"\n","punkte":12,"do":true,"tempo":1.5,"tags":["a","ö"],"oita":17,"spitznam":null}"#
        );
        let schoen = schreib(&s, spieler_typ(), true);
        assert!(schoen.starts_with("{\n  \"nam\": \"Hansi"), "{schoen}");
        assert!(schoen.contains("\n  \"tags\": [\n    \"a\",\n    \"ö\"\n  ],\n"), "{schoen}");
        assert!(schoen.ends_with("\n  \"spitznam\": null\n}"), "{schoen}");
        // Schön geschrieben lässt es sich genauso wieder lesen.
        assert_eq!(schreib(&lies_spieler(&schoen).unwrap(), spieler_typ(), false), schreib(&s, spieler_typ(), false));
    }

    #[test]
    fn was_fehlen_darf_und_was_nicht() {
        // VIELLEICHT-Felder dürfen fehlen; eine ganze Zahl darf als 3.0 dastehen, eine Kommazahl als 3.
        let s = lies_spieler(r#"{"nam":"","punkte":3.0,"do":false,"tempo":3,"tags":[]}"#).unwrap();
        assert_eq!((s.punkte, s.tempo, s.alter.0), (3, 3.0, false));
        assert!(s.spitzname.is_null());
        assert_eq!(unsafe { (*s.tags).laenge }, 0);
        assert_eq!(schreib(&s, spieler_typ(), true).lines().nth(5), Some("  \"tags\": [],"));

        let fehler = |json: &str| lies_spieler(json).err().unwrap();
        assert_eq!(fehler(r#"{"nam":"x","do":true,"tempo":1,"tags":[]}"#), "JSON: Im JSON fehlt 'punkte'.");
        assert_eq!(
            fehler(r#"{"nam":"x","punkte":"zwöf","do":true,"tempo":1,"tags":[]}"#),
            "JSON: Bei 'punkte': Do ghört a gaunze Zoi her, im JSON steht oba a Text."
        );
        assert_eq!(
            fehler(r#"{"nam":"x","punkte":1.5,"do":true,"tempo":1,"tags":[]}"#),
            "JSON: Bei 'punkte': Do ghört a gaunze Zoi her, im JSON steht oba a Kommazoi."
        );
        assert_eq!(
            fehler(r#"{"nam":"x","punkte":1,"do":true,"tempo":1,"tags":["a",7]}"#),
            "JSON: Bei 'tags[1]': Do ghört a Text her, im JSON steht oba a gaunze Zoi."
        );
        assert_eq!(
            fehler(r#"{"nam":null,"punkte":1,"do":true,"tempo":1,"tags":[]}"#),
            "JSON: Bei 'nam': Do ghört a Text her, im JSON steht oba null."
        );
        assert_eq!(
            fehler(r#"{"nam":"x","punkte":1,"do":true,"tempo":1,"tags":[],"oita":"oid"}"#),
            "JSON: Bei 'oita': Do ghört a gaunze Zoi her, im JSON steht oba a Text."
        );
        assert_eq!(fehler("[1, 2]"), "JSON: Do ghört a Objekt ({...}) her, im JSON steht oba a Listn.");
        // Steht ein Name doppelt da, gilt der spätere.
        let s = lies_spieler(r#"{"nam":"a","punkte":1,"do":true,"tempo":1,"tags":[],"punkte":2}"#).unwrap();
        assert_eq!(s.punkte, 2);
    }

    #[test]
    fn kaputtes_json_sagt_wo() {
        let fehler = |json: &str| lies(json.as_bytes()).err().unwrap();
        assert_eq!(fehler(""), "Zeile 1, Spoitn 1: Des JSON hört z'fria auf.");
        assert_eq!(fehler("{\n  \"a\": 1,\n  b: 2\n}"), "Zeile 3, Spoitn 3: Do ghört a Nom in Anführungszeichn her.");
        assert_eq!(fehler("{\"a\" 1}"), "Zeile 1, Spoitn 6: Do ghört a ':' her.");
        assert_eq!(fehler("{\"ä\": 1 \"b\": 2}"), "Zeile 1, Spoitn 9: Do ghört a ',' oda a '}' her.");
        assert_eq!(fehler("[1, 2"), "Zeile 1, Spoitn 6: Des JSON hört z'fria auf.");
        assert_eq!(fehler("[1 2]"), "Zeile 1, Spoitn 4: Do ghört a ',' oda a ']' her.");
        assert_eq!(fehler("[1,]"), "Zeile 1, Spoitn 4: Mit ']' kaun i do nix aufaunga.");
        assert_eq!(fehler("wos"), "Zeile 1, Spoitn 1: Mit 'w' kaun i do nix aufaunga.");
        assert_eq!(fehler("\"offn"), "Zeile 1, Spoitn 6: Der Text hört nimma auf: Do fehlt des \" am End.");
        assert_eq!(fehler("\"a\nb\""), "Zeile 1, Spoitn 3: In an Text derf ka Zeilenend und ka Steuerzeichn stehn (schreib \\n).");
        assert_eq!(fehler("\"a\\xb\""), "Zeile 1, Spoitn 4: '\\x' gibt's in JSON ned.");
        assert_eq!(fehler("\"\\u12g4\""), "Zeile 1, Spoitn 4: Noch \\u ghörn vier Ziffan (0-9, a-f).");
        assert_eq!(fehler("012"), "Zeile 1, Spoitn 1: A Zoi derf ned mit ana Null aufaunga (außa sie is 0 oda 0,irgendwos).");
        assert_eq!(fehler("-"), "Zeile 1, Spoitn 2: Do ghört a Ziffa her.");
        assert_eq!(fehler("1."), "Zeile 1, Spoitn 3: Noch'm Punkt ghört a Ziffa her.");
        assert_eq!(fehler("1e"), "Zeile 1, Spoitn 3: Noch'm e ghört a Ziffa her.");
        assert_eq!(fehler("1 2"), "Zeile 1, Spoitn 3: Do is des JSON scho aus, oba es kummt no wos.");
        assert_eq!(fehler("tru"), "Zeile 1, Spoitn 1: Mit 't' kaun i do nix aufaunga.");
        let tief = "[".repeat(500);
        assert_eq!(fehler(&tief), "Zeile 1, Spoitn 402: Des JSON is z'tiaf verschachtelt.");
    }

    #[test]
    fn zahlen_texte_und_sonderzeichen() {
        assert_eq!(lies(b"-0").unwrap(), Wert::Ganz(0));
        assert_eq!(lies(b"9223372036854775807").unwrap(), Wert::Ganz(i64::MAX));
        // Zu groß für 64 Bit: als Kommazahl.
        assert_eq!(lies(b"9223372036854775808").unwrap(), Wert::Komma(9223372036854775808.0));
        assert_eq!(lies(b"1e3").unwrap(), Wert::Komma(1000.0));
        assert_eq!(lies(b"-2.5E-1").unwrap(), Wert::Komma(-0.25));
        assert_eq!(lies(b" \t\r\n[ ] ").unwrap(), Wert::Liste(Vec::new()));
        assert_eq!(lies(b"{}").unwrap(), Wert::Objekt(Vec::new()));
        assert_eq!(
            lies(r#""\"\\\/\b\f\n\r\t \u00e4 \u20AC \uD83D\uDE00 ö""#.as_bytes()).unwrap(),
            Wert::Text("\"\\/\u{8}\u{c}\n\r\t ä € 😀 ö".into())
        );
        // Eine halbe Paarung wird zum Ersatzzeichen, das Zeichen danach bleibt.
        assert_eq!(lies(br#""\uD83Dx""#).unwrap(), Wert::Text("\u{FFFD}x".into()));
        assert_eq!(lies(br#""\uD83D\u0041""#).unwrap(), Wert::Text("\u{FFFD}A".into()));

        let mut s = Schreiber { aus: String::new(), schoen: false, zeile: 1 };
        s.text("a\"b\\c\n\r\t\u{8}\u{c}\u{1} ä€😀".as_bytes());
        assert_eq!(s.aus, r#""a\"b\\c\n\r\t\b\f\u0001 ä€😀""#);
        for (zahl, soll) in [(3.0, "3"), (0.1, "0.1"), (-2.5, "-2.5"), (f64::NAN, "null"), (f64::INFINITY, "null"), (1e21, "1000000000000000000000")] {
            let mut s = Schreiber { aus: String::new(), schoen: false, zeile: 1 };
            s.kommazahl(zahl);
            assert_eq!(s.aus, soll);
        }
    }

    #[test]
    fn listen_von_strukturen_und_verzeichnisse() {
        // LISTN VON Punkt mit Punkt { x ZOI, y ZOI }: Die Strukturen liegen direkt hintereinander in der Liste.
        let zoi = einfach(ART_ZOI);
        let punkt = typ(ART_STRUKTUR, 16, 2, std::ptr::null(), vec![feld("x", 0, zoi), feld("y", 8, zoi)]);
        let punkte = typ(ART_LISTN, 8, 0, punkt, Vec::new());
        let mut liste: *mut ListenKopf = std::ptr::null_mut();
        let ziel = (&mut liste as *mut *mut ListenKopf).cast();
        unsafe {
            assert!(oida_json_lesen(text(r#"[{"x":1,"y":2},{"y":4,"x":3}]"#), punkte, ziel).is_null());
            assert_eq!(std::slice::from_raw_parts((*liste).daten as *const i64, 4), [1, 2, 3, 4]);
        }
        assert_eq!(schreib(&liste, punkte, false), r#"[{"x":1,"y":2},{"x":3,"y":4}]"#);
        // Bei einem Fehler bleibt das Ziel, wie es war.
        let vorher = liste;
        let fehler = unsafe { oida_json_lesen(text(r#"[{"x":1,"y":2},{"x":3}]"#), punkte, ziel) };
        assert_eq!(inhalt(fehler), "JSON: Im JSON fehlt '[1].y'.");
        assert_eq!(liste, vorher);

        // VAZEICHNIS VON KETTN UND LISTN VON ZOI
        let zahlen = typ(ART_LISTN, 8, 0, zoi, Vec::new());
        let verzeichnis_typ = typ(ART_VAZEICHNIS, 8, 0, zahlen, Vec::new());
        let mut verzeichnis: *mut WbKopf = std::ptr::null_mut();
        let ziel = (&mut verzeichnis as *mut *mut WbKopf).cast();
        unsafe {
            assert!(oida_json_lesen(text(r#"{"ans":[1],"zwa":[2,2],"ans":[]}"#), verzeichnis_typ, ziel).is_null());
            assert_eq!((*verzeichnis).laenge, 2);
            // Die Schlüssel lassen sich finden, und das Verzeichnis zählt jeden genau einmal.
            let zwa = text("zwa");
            assert_eq!(super::super::oida_wb_finde(verzeichnis, zwa as i64), 0);
            assert_eq!((*(*(*verzeichnis).schluessel as *const TextKopf)).zaehler, 1);
        }
        assert_eq!(schreib(&verzeichnis, verzeichnis_typ, false), r#"{"zwa":[2,2],"ans":[]}"#);
        let fehler = unsafe { oida_json_lesen(text(r#"{"a":[1],"b":{"c":[1,"x"]}}"#), verzeichnis_typ, ziel) };
        assert_eq!(inhalt(fehler), "JSON: Bei 'b': Do ghört a Listn ([...]) her, im JSON steht oba a Objekt.");
    }

    /// So liegt `STRUKTUR Json` aus der Bibliothek im Speicher.
    #[repr(C)]
    struct Json {
        oat: i64,
        is_woahr: bool,
        gaunze: i64,
        komma: f64,
        inhalt: *mut TextKopf,
        teile: *mut ListenKopf,
        nama: *mut ListenKopf,
    }

    fn beliebig_typ() -> &'static JsonTyp {
        let nix = einfach(ART_ZOI);
        typ(
            ART_BELIEBIG,
            std::mem::size_of::<Json>() as i64,
            7,
            std::ptr::null(),
            vec![
                feld("oat", std::mem::offset_of!(Json, oat), nix),
                feld("is_woahr", std::mem::offset_of!(Json, is_woahr), nix),
                feld("gaunze", std::mem::offset_of!(Json, gaunze), nix),
                feld("komma", std::mem::offset_of!(Json, komma), nix),
                feld("inhalt", std::mem::offset_of!(Json, inhalt), nix),
                feld("teile", std::mem::offset_of!(Json, teile), nix),
                feld("nama", std::mem::offset_of!(Json, nama), nix),
            ],
        )
    }

    #[test]
    fn json_ohne_festen_aufbau() {
        let typ = beliebig_typ();
        let quelle = r#"{"nam":"Sepp","zoin":[1,2.5,-3,9007199254740993],"do":true,"nix":null,"tiaf":{"laar":{},"a":[]},"nam":"zwamoi"}"#;
        let mut wurzel = std::mem::MaybeUninit::<Json>::zeroed();
        let wurzel = unsafe {
            assert!(oida_json_lesen(text(quelle), typ, wurzel.as_mut_ptr().cast()).is_null());
            wurzel.assume_init()
        };
        assert_eq!(wurzel.oat, OAT_OBJEKT);
        let (teile, namen) = unsafe { (&*wurzel.teile, &*wurzel.nama) };
        assert_eq!((teile.laenge, namen.laenge), (6, 6));
        let kind = |i: usize| unsafe { &*(teile.daten as *const Json).add(i) };
        assert_eq!((kind(0).oat, inhalt(kind(0).inhalt)), (OAT_TEXT, "Sepp".to_string()));
        // Gleiche Namen teilen sich einen Text, und der zählt mit, an wie vielen Stellen er gebraucht wird.
        let name = |i: usize| unsafe { *(namen.daten as *const *mut TextKopf).add(i) };
        assert_eq!(name(0), name(5));
        assert_eq!(unsafe { ((*name(0)).zaehler, (*name(1)).zaehler) }, (2, 1));
        assert_eq!((kind(2).oat, kind(2).is_woahr, kind(3).oat), (OAT_JONA, true, OAT_NIX));
        let zahlen = unsafe { std::slice::from_raw_parts((*kind(1).teile).daten as *const Json, 4) };
        assert_eq!((zahlen[0].oat, zahlen[0].gaunze, zahlen[0].komma), (OAT_ZOI, 1, 1.0));
        assert_eq!((zahlen[1].oat, zahlen[1].komma), (OAT_KOMMAZOI, 2.5));
        // Große ganze Zahlen bleiben genau (als Kommazahl ginge die letzte Stelle verloren).
        assert_eq!(zahlen[3].gaunze, 9007199254740993);
        // Geschrieben kommt genau dasselbe heraus, auch der doppelte Name.
        assert_eq!(schreib(&wurzel, typ, false), quelle);
        let schoen = schreib(&wurzel, typ, true);
        assert!(schoen.contains("\n  \"tiaf\": {\n    \"laar\": {},\n    \"a\": []\n  },\n"), "{schoen}");

        // Ein Json-Wert als Feld einer Struktur: Der Teil bleibt, wie er im JSON steht, und darf fehlen.
        #[repr(C)]
        struct Nachricht {
            art: *mut TextKopf,
            daten: Json,
        }
        let nachricht_typ = super::tests::typ(
            ART_STRUKTUR,
            std::mem::size_of::<Nachricht>() as i64,
            2,
            std::ptr::null(),
            vec![feld("oat", 0, einfach(ART_KETTN)), feld("daten", std::mem::offset_of!(Nachricht, daten), typ)],
        );
        for (json, geschrieben) in [
            (r#"{"oat":"zug","daten":{"x":[1,"zwa"]}}"#, r#"{"oat":"zug","daten":{"x":[1,"zwa"]}}"#),
            (r#"{"oat":"nix"}"#, r#"{"oat":"nix","daten":null}"#),
        ] {
            let mut ziel = std::mem::MaybeUninit::<Nachricht>::zeroed();
            unsafe {
                assert!(oida_json_lesen(text(json), nachricht_typ, ziel.as_mut_ptr().cast()).is_null());
                assert_eq!(schreib(&ziel.assume_init(), nachricht_typ, false), geschrieben);
            }
        }
    }

    /// Zum Messen, nicht zum Prüfen: `cargo test --release -p oida-laufzeit tempo_messen -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn tempo_messen() {
        let mut json = String::from("[");
        for i in 0..200_000 {
            if i > 0 {
                json.push(',');
            }
            let _ = write!(
                json,
                r#"{{"nam":"Spiela {i}","punkte":{},"do":{},"tempo":{}.5,"tags":["Schwert","Schüd","Öpfe"],"oita":null}}"#,
                i * 7,
                i % 2 == 0,
                i
            );
        }
        json.push(']');
        let liste = typ(ART_LISTN, 8, 0, spieler_typ(), Vec::new());
        let uhr = std::time::Instant::now();
        let wert = lies(json.as_bytes()).unwrap();
        let gelesen = uhr.elapsed();
        assert!(unsafe { pruefe(&wert, liste) }.is_ok());
        let geprueft = uhr.elapsed();
        let mut ziel: *mut ListenKopf = std::ptr::null_mut();
        unsafe { baue(&wert, liste, (&raw mut ziel).cast()) };
        let gebaut = uhr.elapsed();
        drop(wert);
        let weg = uhr.elapsed();
        let zurueck = schreib(&ziel, liste, false);
        let geschrieben = uhr.elapsed();
        println!(
            "{} MB: lesen {:?}, prüfen {:?}, bauen {:?}, Baum wegräumen {:?}, schreiben {:?}",
            json.len() / 1_000_000,
            gelesen,
            geprueft - gelesen,
            gebaut - geprueft,
            weg - gebaut,
            geschrieben - weg
        );
        assert_eq!(zurueck.len(), json.len() + 200_000 * r#","spitznam":null"#.len());
    }
}
