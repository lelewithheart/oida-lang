//! Der Syntaxbaum (AST) eines OIDA-Programms.

use crate::fehler::Position;

/// Ein Typ. Strukturen und Listen tragen nur eine Nummer; was dahintersteckt,
/// steht in der Tabelle [`Typen`]. So bleibt ein Typ klein und kopierbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Typ {
    /// Ganze Zahl mit 64 Bit.
    Zoi,
    /// Kommazahl (64 Bit).
    Kommazoi,
    /// Wahrheitswert.
    Jona,
    /// Text.
    Kettn,
    /// Eine Struktur: Nummer in [`Typen::strukturen`]. Ein Wert, der direkt
    /// in der Variable oder in der Liste liegt.
    Struktur(u32),
    /// Eine Liste: Nummer in [`Typen::listen`], dort steht der Typ der Elemente.
    Liste(u32),
    /// Die leere Liste `[]`, solange noch nicht klar ist, wovon sie eine Liste ist.
    LeereListe,
    /// Kein Wert (Funktion ohne Rückgabe; vor der Typprüfung auch "noch unbekannt").
    Nix,
    /// Hier ist schon ein Fehler gemeldet worden; verhindert Folgefehler.
    Fehler,
}

/// Eine Struktur, wie die Typprüfung sie kennt.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StrukturInfo {
    pub name: String,
    /// Die Felder in der Reihenfolge, in der sie angesagt wurden.
    pub felder: Vec<(String, Typ)>,
}

/// Alle Strukturen und Listentypen eines Programms. Die Typprüfung füllt die
/// Tabelle, die IR-Erzeugung liest sie.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Typen {
    pub strukturen: Vec<StrukturInfo>,
    /// Der Typ der Elemente je Listentyp.
    pub listen: Vec<Typ>,
}

impl Typen {
    /// Der Typ "Liste von `element`". Jeden gibt es nur einmal.
    pub fn liste_von(&mut self, element: Typ) -> Typ {
        let nummer = match self.listen.iter().position(|t| *t == element) {
            Some(n) => n,
            None => {
                self.listen.push(element);
                self.listen.len() - 1
            }
        };
        Typ::Liste(nummer as u32)
    }

    /// Wovon `typ` eine Liste ist.
    pub fn element(&self, typ: Typ) -> Option<Typ> {
        match typ {
            Typ::Liste(n) => self.listen.get(n as usize).copied(),
            _ => None,
        }
    }

    pub fn struktur(&self, typ: Typ) -> Option<&StrukturInfo> {
        match typ {
            Typ::Struktur(n) => self.strukturen.get(n as usize),
            _ => None,
        }
    }

    /// Hängt an Werten dieses Typs Speicher, der gezählt und freigegeben
    /// werden muss? Das sind Texte, Listen und Strukturen, die so etwas enthalten.
    pub fn verwaltet(&self, typ: Typ) -> bool {
        match typ {
            Typ::Kettn | Typ::Liste(_) | Typ::LeereListe => true,
            Typ::Struktur(_) => self.struktur(typ).is_some_and(|s| s.felder.iter().any(|(_, t)| self.verwaltet(*t))),
            Typ::Zoi | Typ::Kommazoi | Typ::Jona | Typ::Nix | Typ::Fehler => false,
        }
    }
}

/// Ein Typ, wie er im Quelltext steht. Die Typprüfung macht daraus einen [`Typ`].
#[derive(Debug, Clone, PartialEq)]
pub enum TypAngabe {
    /// `ZOI`, `KOMMAZOI`, `JONA`, `KETTN`
    Einfach(Typ),
    /// Der Name einer Struktur
    Name { name: String, pos: Position },
    /// `LISTN VON <typ>`
    Liste(Box<TypAngabe>),
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
    /// `LÄNGN(text)`: Anzahl der Zeichen. `LÄNGN(liste)`: Anzahl der Elemente.
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
    /// `HAU EINE(liste, wert)`: hinten an die Liste anhängen
    Dazu,
    /// `HAU AUSSE(liste, stelle)`: das Element an der Stelle entfernen
    Raus,
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
    /// Aufruf einer Funktion. Heißt so eine Struktur, macht die Typprüfung
    /// daraus [`AusdruckArt::Bau`].
    Aufruf { name: String, argumente: Vec<Ausdruck> },
    /// Aufruf einer eingebauten Funktion: `LÄNGN(text)`, `WURZL(2)` ...
    Eingebaut { funktion: Eingebaut, argumente: Vec<Ausdruck> },
    /// `ZOI(x)`, `KETTN(x)` ... oder von der Typprüfung eingefügt (ZOI -> KOMMAZOI).
    Umwandlung { ziel: Typ, innen: Box<Ausdruck> },
    /// `objekt.name`: ein Feld einer Struktur. `nummer` ist die Stelle des
    /// Feldes in der Struktur, die Typprüfung trägt sie ein.
    Feld { objekt: Box<Ausdruck>, name: String, nummer: usize },
    /// `liste[index]`: ein Element einer Liste, gezählt ab 0.
    Stelle { liste: Box<Ausdruck>, index: Box<Ausdruck> },
    /// `[a, b, c]`: eine neue Liste mit diesen Elementen.
    ListenWert { elemente: Vec<Ausdruck> },
    /// `Punkt(1.5, 2.0)`: ein neuer Wert einer Struktur, die Felder der Reihe nach.
    /// Entsteht in der Typprüfung aus einem Aufruf.
    Bau { struktur: u32, argumente: Vec<Ausdruck> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Anweisung {
    pub art: AnweisungArt,
    pub pos: Position,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnweisungArt {
    /// `SOG AU name [TYP] = wert OIDA`
    Deklaration { name: String, angabe: Option<TypAngabe>, wert: Ausdruck, id: usize },
    /// `ziel = wert OIDA` oder mit Rechnung `ziel += wert OIDA`.
    /// Das Ziel ist eine Variable, ein Feld (`p.x`) oder ein Element (`liste[i]`).
    Zuweisung { ziel: Ausdruck, op: Option<BinaerOp>, wert: Ausdruck },
    /// `WENN bedingung DAUN ... [SUNST ...] PASST SCHO`
    Wenn { bedingung: Ausdruck, dann: Vec<Anweisung>, sunst: Vec<Anweisung> },
    /// `SOLONG bedingung TUA ... PASST SCHO`
    Solong { bedingung: Ausdruck, rumpf: Vec<Anweisung> },
    /// `FIA name VON von BIS bis [SCHRITT schritt] TUA ... PASST SCHO`
    /// Zählt von `von` bis einschließlich `bis`. `id` ist die Nummer der Zählvariable.
    Fia { name: String, id: usize, von: Ausdruck, bis: Ausdruck, schritt: Option<Ausdruck>, rumpf: Vec<Anweisung> },
    /// `FIA JEDE name IN liste TUA ... PASST SCHO`
    /// `name` ist in jedem Durchlauf eine Kopie des nächsten Elements.
    FiaJede { name: String, id: usize, liste: Ausdruck, rumpf: Vec<Anweisung> },
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
    pub angabe: TypAngabe,
    /// Der Typ zur Angabe. Bei den vier einfachen Typen trägt ihn schon der
    /// Parser ein, sonst die Typprüfung.
    pub typ: Typ,
    pub pos: Position,
    /// Nummer der Variable, die Typprüfung trägt sie ein.
    pub id: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Funktion {
    pub name: String,
    pub parameter: Vec<Parameter>,
    /// Was hinter GIBT ZRUCK steht (fehlt bei Funktionen ohne Rückgabe).
    pub rueckgabe_angabe: Option<TypAngabe>,
    /// Der Typ dazu, wie bei [`Parameter::typ`].
    pub rueckgabe: Typ,
    pub rumpf: Vec<Anweisung>,
    pub pos: Position,
}

/// Ein Feld in `STRUKTUR ... PASST SCHO`.
#[derive(Debug, Clone, PartialEq)]
pub struct FeldDef {
    pub name: String,
    pub angabe: TypAngabe,
    pub pos: Position,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StrukturDef {
    pub name: String,
    pub felder: Vec<FeldDef>,
    pub pos: Position,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Programm {
    pub strukturen: Vec<StrukturDef>,
    pub funktionen: Vec<Funktion>,
    /// Die Anweisungen außerhalb von Funktionen: das Hauptprogramm.
    pub haupt: Vec<Anweisung>,
    /// Alle Strukturen und Listentypen; die Typprüfung füllt die Tabelle.
    pub typen: Typen,
}
