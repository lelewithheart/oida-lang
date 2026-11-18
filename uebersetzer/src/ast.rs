//! Der Syntaxbaum (AST) eines OIDA-Programms.

use crate::fehler::Position;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typ {
    /// Ganze Zahl mit 64 Bit.
    Zoi,
    /// Kommazahl (64 Bit).
    Kommazoi,
    /// Wahrheitswert.
    Jona,
    /// Text.
    Kettn,
    /// Kein Wert (Funktion ohne Rückgabe; vor der Typprüfung auch "noch unbekannt").
    Nix,
    /// Hier ist schon ein Fehler gemeldet worden; verhindert Folgefehler.
    Fehler,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaerOp {
    Minus,
    Ned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaerOp {
    Plus,
    Minus,
    Mal,
    Durch,
    Rest,
    Gleich,
    Ungleich,
    Kleiner,
    KleinerGleich,
    Groesser,
    GroesserGleich,
    Und,
    Oda,
}

impl BinaerOp {
    /// So wie man es im Quelltext schreibt (für Meldungen). UND und ODA
    /// kommen aus dem Wörterbuch und werden woanders eingesetzt.
    pub fn zeichen(self) -> &'static str {
        match self {
            BinaerOp::Plus => "+",
            BinaerOp::Minus => "-",
            BinaerOp::Mal => "*",
            BinaerOp::Durch => "/",
            BinaerOp::Rest => "%",
            BinaerOp::Gleich => "==",
            BinaerOp::Ungleich => "!=",
            BinaerOp::Kleiner => "<",
            BinaerOp::KleinerGleich => "<=",
            BinaerOp::Groesser => ">",
            BinaerOp::GroesserGleich => ">=",
            BinaerOp::Und => "UND",
            BinaerOp::Oda => "ODA",
        }
    }

    pub fn ist_vergleich(self) -> bool {
        matches!(
            self,
            BinaerOp::Gleich
                | BinaerOp::Ungleich
                | BinaerOp::Kleiner
                | BinaerOp::KleinerGleich
                | BinaerOp::Groesser
                | BinaerOp::GroesserGleich
        )
    }
}

/// Die eingebauten Funktionen. Ihre Namen kommen wie alle Wörter aus dem Wörterbuch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eingebaut {
    /// `LIES()` oder `LIES("Frage: ")`: eine Zeile von der Tastatur
    Lies,
    /// `LÄNGN(text)`: Anzahl der Zeichen
    Laenge,
    /// `STICKL(text, von)` oder `STICKL(text, von, anzahl)`: ein Stück vom Text, gezählt ab 0
    Stueck,
    /// `SUACH(text, gesucht)`: Stelle des ersten Treffers ab 0, oder -1
    Suche,
    /// `SCHREI(text)`: in Großbuchstaben
    Gross,
    /// `FLÜSTER(text)`: in Kleinbuchstaben
    Klein,
    /// `STUTZ(text)`: ohne Leerraum am Anfang und Ende
    Stutz,
    /// `ERSETZ(text, alt, neu)`
    Ersetz,
    Wurzel,
    Hoch,
    Betrag,
    Min,
    Max,
    Sinus,
    Kosinus,
    /// `WINKL(y, x)`: der Winkel zum Punkt (x, y), wie atan2
    Winkel,
    /// `E_HOCH(x)`
    Exp,
    Abrunden,
    Aufrunden,
    Runden,
    /// `ZUFOI()`: Kommazahl von 0 bis unter 1. `ZUFOI(a, b)`: ganze Zahl von a bis b.
    Zufall,
    /// `SOMEN(n)`: Startwert für den Zufall festlegen (dann kommt immer dieselbe Folge)
    Samen,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ausdruck {
    pub art: AusdruckArt,
    pub pos: Position,
    /// Wird von der Typprüfung ausgefüllt.
    pub typ: Typ,
}

impl Ausdruck {
    pub fn neu(art: AusdruckArt, pos: Position) -> Ausdruck {
        Ausdruck { art, pos, typ: Typ::Nix }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AusdruckArt {
    Zoi(i64),
    Kommazoi(f64),
    Jona(bool),
    Text(String),
    /// `id` ist die Nummer der Variable, die Typprüfung trägt sie ein.
    Variable { name: String, id: usize },
    Unaer { op: UnaerOp, innen: Box<Ausdruck> },
    Binaer { op: BinaerOp, links: Box<Ausdruck>, rechts: Box<Ausdruck> },
    Aufruf { name: String, argumente: Vec<Ausdruck> },
    /// Aufruf einer eingebauten Funktion: `LÄNGN(text)`, `WURZL(2)` ...
    Eingebaut { funktion: Eingebaut, argumente: Vec<Ausdruck> },
    /// `ZOI(x)`, `KETTN(x)` ... oder von der Typprüfung eingefügt (ZOI -> KOMMAZOI).
    Umwandlung { ziel: Typ, innen: Box<Ausdruck> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Anweisung {
    pub art: AnweisungArt,
    pub pos: Position,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnweisungArt {
    /// `SOG AU name [TYP] = wert OIDA`
    Deklaration { name: String, typ: Option<Typ>, wert: Ausdruck, id: usize },
    /// `name = wert OIDA`
    Zuweisung { name: String, wert: Ausdruck, id: usize },
    /// `WENN bedingung DAUN ... [SUNST ...] PASST SCHO`
    Wenn { bedingung: Ausdruck, dann: Vec<Anweisung>, sunst: Vec<Anweisung> },
    /// `SOLONG bedingung TUA ... PASST SCHO`
    Solong { bedingung: Ausdruck, rumpf: Vec<Anweisung> },
    /// `FIA name VON von BIS bis [SCHRITT schritt] TUA ... PASST SCHO`
    /// Zählt von `von` bis einschließlich `bis`. `id` ist die Nummer der Zählvariable.
    Fia { name: String, id: usize, von: Ausdruck, bis: Ausdruck, schritt: Option<Ausdruck>, rumpf: Vec<Anweisung> },
    /// `HAU OB OIDA`: die innerste Schleife verlassen
    HauOb,
    /// `WEIDA OIDA`: gleich zum nächsten Durchlauf der innersten Schleife
    Weida,
    /// `GIB ZRUCK [wert] OIDA`
    GibZruck { wert: Option<Ausdruck> },
    /// `SCHREIB(a, b, ...) OIDA`
    Schreib { argumente: Vec<Ausdruck> },
    /// Ein Ausdruck als Anweisung, z.B. ein Funktionsaufruf.
    Ausdruck(Ausdruck),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub typ: Typ,
    pub pos: Position,
    /// Nummer der Variable, die Typprüfung trägt sie ein.
    pub id: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Funktion {
    pub name: String,
    pub parameter: Vec<Parameter>,
    pub rueckgabe: Typ,
    pub rumpf: Vec<Anweisung>,
    pub pos: Position,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Programm {
    pub funktionen: Vec<Funktion>,
    /// Die Anweisungen außerhalb von Funktionen: das Hauptprogramm.
    pub haupt: Vec<Anweisung>,
}
