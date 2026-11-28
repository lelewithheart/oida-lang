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
    /// Eine Klasse: Nummer in [`Typen::klassen`]. Ein Verweis auf ein Objekt am
    /// Heap; mehrere Variablen können dasselbe Objekt meinen.
    Klasse(u32),
    /// `VIELLEICHT <typ>`: ein Wert dieses Typs oder `NIX`. `nummer` zeigt in
    /// [`Typen::vielleicht`]. `zeiger` sagt, wie der Wert im Speicher aussieht:
    /// Ist der Typ darin ohnehin ein Zeiger (Text, Liste, Objekt), ist NIX der
    /// Nullzeiger; sonst steht ein Merker "ist da" neben dem Wert.
    Vielleicht { nummer: u32, zeiger: bool },
    /// `<typ> ODA FEHLA`: ein Wert oder ein Fehler. Nummer in [`Typen::ergebnisse`].
    Ergebnis(u32),
    /// Ein Fehler: im Speicher ein Text mit der Meldung.
    Fehla,
    /// Eine Funktion als Wert: Nummer in [`Typen::funktionen`]. Im Speicher
    /// ein Zeiger auf eine Hülle mit dem Code und den mitgenommenen Werten.
    Funktion(u32),
    /// `VAZEICHNIS VON <schlüssel> UND <wert>`: Nummer in [`Typen::verzeichnisse`].
    Verzeichnis(u32),
    /// Die leere Liste `[]`, solange noch nicht klar ist, wovon sie eine Liste ist.
    LeereListe,
    /// Das Wort `NIX`, solange noch nicht klar ist, was fehlen soll.
    NixWert,
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
    /// Nur bei einer Aufzählung: die Namen ihrer Werte. Im Speicher ist eine Aufzählung
    /// eine Struktur mit einem einzigen Feld, der Nummer des Werts (seiner Stelle hier).
    pub werte: Vec<String>,
}

/// Ein Feld einer Klasse.
#[derive(Debug, Clone, PartialEq)]
pub struct KlassenFeldInfo {
    pub name: String,
    pub typ: Typ,
    /// Die Klasse, in der das Feld angesagt ist, und seine Stelle dort
    /// (in [`KlassenDef::felder`]).
    pub klasse: u32,
    pub stelle: usize,
    /// Hat das Feld einen Wert dabeistehen (`lebn ZOI = 100`)? Dann gibt man
    /// es beim Anlegen eines Objekts nicht an.
    pub hat_standard: bool,
    /// `GHEIM`: Nur die Methoden der Klasse, in der es angesagt ist, kommen an das Feld.
    pub gheim: bool,
}

/// Eine Methode, wie man sie an einem Objekt aufrufen kann.
#[derive(Debug, Clone, PartialEq)]
pub struct MethodenInfo {
    pub name: String,
    pub parameter: Vec<Typ>,
    pub rueckgabe: Typ,
    /// Die Klasse, in der diese Fassung steht (bei geerbten: eine Elternklasse).
    pub klasse: u32,
    /// Die Stelle in der Sprungtabelle, wenn die Methode irgendwo in der
    /// Familie überschrieben wird. Sonst wird sie direkt aufgerufen.
    pub platz: Option<usize>,
    /// `GHEIM`: Nur die Methoden der Klasse, in der sie steht, können sie aufrufen.
    pub gheim: bool,
}

/// Der Typ einer Funktion als Wert: was sie nimmt und was sie zurückgibt.
#[derive(Debug, Clone, PartialEq)]
pub struct FunktionsTyp {
    pub parameter: Vec<Typ>,
    pub rueckgabe: Typ,
}

/// Eine Klasse, wie die Typprüfung sie kennt.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct KlassenInfo {
    pub name: String,
    /// Nur eine Vorlage (`KLASSE Stapel VON T`): Daraus entstehen erst beim
    /// Verwenden echte Klassen (`Stapel VON ZOI`). Für die Vorlage selbst wird kein Code erzeugt.
    pub vorlage: bool,
    pub eltern: Option<u32>,
    /// Alle Felder, die geerbten zuerst. So liegen sie auch im Objekt.
    pub felder: Vec<KlassenFeldInfo>,
    /// Alle Methoden, die man an einem Objekt dieser Klasse aufrufen kann, auch die geerbten.
    pub methoden: Vec<MethodenInfo>,
    /// Wie viele Plätze die Sprungtabelle hat (für alle Klassen einer Familie gleich viele).
    pub plaetze: usize,
    /// Eine Schnittstelle: Objekte davon gibt es nicht, nur Objekte von Klassen, die sie können.
    pub schnittstelle: bool,
    /// Die Schnittstellen, die diese Klasse selbst verspricht (die geerbten stehen bei den Eltern).
    pub kann: Vec<u32>,
}

/// Alle Strukturen, Klassen und Listentypen eines Programms. Die Typprüfung
/// füllt die Tabelle, die IR-Erzeugung liest sie.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Typen {
    pub strukturen: Vec<StrukturInfo>,
    pub klassen: Vec<KlassenInfo>,
    /// Der Typ der Elemente je Listentyp.
    pub listen: Vec<Typ>,
    /// Was je `VIELLEICHT`-Typ da sein kann.
    pub vielleicht: Vec<Typ>,
    /// Der Typ des Werts je `... ODA FEHLA` (`Nix` bei `NIX ODA FEHLA`).
    pub ergebnisse: Vec<Typ>,
    /// Die Typen von Funktionen als Werte.
    pub funktionen: Vec<FunktionsTyp>,
    /// Schlüssel und Wert je Verzeichnis-Typ.
    pub verzeichnisse: Vec<(Typ, Typ)>,
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

    /// Der Typ "`innen` oder NIX". Jeden gibt es nur einmal.
    pub fn vielleicht_von(&mut self, innen: Typ) -> Typ {
        let nummer = match self.vielleicht.iter().position(|t| *t == innen) {
            Some(n) => n,
            None => {
                self.vielleicht.push(innen);
                self.vielleicht.len() - 1
            }
        };
        Typ::Vielleicht { nummer: nummer as u32, zeiger: self.zeiger(innen) }
    }

    /// Was bei einem `VIELLEICHT`-Typ da sein kann.
    pub fn innen(&self, typ: Typ) -> Option<Typ> {
        match typ {
            Typ::Vielleicht { nummer, .. } => self.vielleicht.get(nummer as usize).copied(),
            _ => None,
        }
    }

    /// Der Typ "`wert` oder ein Fehler". Jeden gibt es nur einmal.
    pub fn ergebnis_von(&mut self, wert: Typ) -> Typ {
        let nummer = match self.ergebnisse.iter().position(|t| *t == wert) {
            Some(n) => n,
            None => {
                self.ergebnisse.push(wert);
                self.ergebnisse.len() - 1
            }
        };
        Typ::Ergebnis(nummer as u32)
    }

    /// Der Typ des Werts bei `... ODA FEHLA`.
    pub fn wert_von(&self, typ: Typ) -> Option<Typ> {
        match typ {
            Typ::Ergebnis(n) => self.ergebnisse.get(n as usize).copied(),
            _ => None,
        }
    }

    /// Der Typ "Funktion von diesen Werten nach diesem". Jeden gibt es nur einmal.
    pub fn funktion_von(&mut self, parameter: Vec<Typ>, rueckgabe: Typ) -> Typ {
        let gesucht = FunktionsTyp { parameter, rueckgabe };
        let nummer = match self.funktionen.iter().position(|t| *t == gesucht) {
            Some(n) => n,
            None => {
                self.funktionen.push(gesucht);
                self.funktionen.len() - 1
            }
        };
        Typ::Funktion(nummer as u32)
    }

    pub fn funktions_typ(&self, typ: Typ) -> Option<&FunktionsTyp> {
        match typ {
            Typ::Funktion(n) => self.funktionen.get(n as usize),
            _ => None,
        }
    }

    /// Der Typ "Verzeichnis von `schluessel` nach `wert`". Jeden gibt es nur einmal.
    pub fn verzeichnis_von(&mut self, schluessel: Typ, wert: Typ) -> Typ {
        let nummer = match self.verzeichnisse.iter().position(|t| *t == (schluessel, wert)) {
            Some(n) => n,
            None => {
                self.verzeichnisse.push((schluessel, wert));
                self.verzeichnisse.len() - 1
            }
        };
        Typ::Verzeichnis(nummer as u32)
    }

    /// Schlüssel und Wert eines Verzeichnis-Typs.
    pub fn verzeichnis(&self, typ: Typ) -> Option<(Typ, Typ)> {
        match typ {
            Typ::Verzeichnis(n) => self.verzeichnisse.get(n as usize).copied(),
            _ => None,
        }
    }

    /// Ist ein Wert dieses Typs im Speicher ein einzelner Zeiger auf etwas am Heap?
    pub fn zeiger(&self, typ: Typ) -> bool {
        matches!(
            typ,
            Typ::Kettn
                | Typ::Liste(_)
                | Typ::LeereListe
                | Typ::Klasse(_)
                | Typ::Fehla
                | Typ::Funktion(_)
                | Typ::Verzeichnis(_)
        )
    }

    pub fn struktur(&self, typ: Typ) -> Option<&StrukturInfo> {
        match typ {
            Typ::Struktur(n) => self.strukturen.get(n as usize),
            _ => None,
        }
    }

    pub fn klasse(&self, typ: Typ) -> Option<&KlassenInfo> {
        match typ {
            Typ::Klasse(n) => self.klassen.get(n as usize),
            _ => None,
        }
    }

    /// Ist `kind` dieselbe Klasse wie `eltern` oder erbt (über wie viele Stufen auch immer) von ihr?
    /// Ist `kind` ein `eltern`: dieselbe Klasse, eine, die davon erbt, oder (wenn `eltern` eine
    /// Schnittstelle ist) eine Klasse, die sie kann oder von so einer erbt?
    pub fn erbt_von(&self, kind: u32, eltern: u32) -> bool {
        let mut aktuell = Some(kind);
        // Die Zählung schützt vor einem Kreis, falls der gerade erst als Fehler gemeldet wird.
        for _ in 0..=self.klassen.len() {
            match aktuell {
                Some(k) if k == eltern => return true,
                Some(k) => {
                    let Some(info) = self.klassen.get(k as usize) else { return false };
                    if info.kann.contains(&eltern) {
                        return true;
                    }
                    aktuell = info.eltern;
                }
                None => return false,
            }
        }
        false
    }

    /// Hängt an Werten dieses Typs Speicher, der gezählt und freigegeben
    /// werden muss? Das sind Texte, Listen und Strukturen, die so etwas enthalten.
    pub fn verwaltet(&self, typ: Typ) -> bool {
        match typ {
            Typ::Kettn
            | Typ::Liste(_)
            | Typ::LeereListe
            | Typ::Klasse(_)
            | Typ::Fehla
            | Typ::Ergebnis(_)
            | Typ::Funktion(_)
            | Typ::Verzeichnis(_) => true,
            Typ::Struktur(_) => self.struktur(typ).is_some_and(|s| s.felder.iter().any(|(_, t)| self.verwaltet(*t))),
            Typ::Vielleicht { .. } => self.innen(typ).is_some_and(|t| self.verwaltet(t)),
            Typ::Zoi | Typ::Kommazoi | Typ::Jona | Typ::Nix | Typ::NixWert | Typ::Fehler => false,
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
    /// `VIELLEICHT <typ>`
    Vielleicht { innen: Box<TypAngabe>, pos: Position },
    /// `<typ> ODA FEHLA`; ohne Typ davor: `NIX ODA FEHLA`
    OderFehler { wert: Option<Box<TypAngabe>>, pos: Position },
    /// `FUNKTIAUN(<typ>, ...) [GIBT ZRUCK <typ>]`: eine Funktion als Wert
    Funktion { parameter: Vec<TypAngabe>, rueckgabe: Option<Box<TypAngabe>>, pos: Position },
    /// `Stapel VON ZOI`, `Paar VON ZOI UND KETTN`: eine Vorlage mit ihren Typen
    Generisch { name: String, argumente: Vec<TypAngabe>, pos: Position },
    /// `VAZEICHNIS VON <schlüssel> UND <wert>`
    Verzeichnis { schluessel: Box<TypAngabe>, wert: Box<TypAngabe>, pos: Position },
}

impl TypAngabe {
    /// Kommt in dieser Angabe einer der Namen vor (die offenen Typen einer Vorlage)?
    pub fn erwaehnt(&self, namen: &[String]) -> bool {
        match self {
            TypAngabe::Einfach(_) => false,
            TypAngabe::Name { name, .. } => namen.contains(name),
            TypAngabe::Liste(innen) | TypAngabe::Vielleicht { innen, .. } => innen.erwaehnt(namen),
            TypAngabe::OderFehler { wert, .. } => wert.as_ref().is_some_and(|w| w.erwaehnt(namen)),
            TypAngabe::Funktion { parameter, rueckgabe, .. } => {
                parameter.iter().any(|p| p.erwaehnt(namen)) || rueckgabe.as_ref().is_some_and(|r| r.erwaehnt(namen))
            }
            TypAngabe::Generisch { argumente, .. } => argumente.iter().any(|a| a.erwaehnt(namen)),
            TypAngabe::Verzeichnis { schluessel, wert, .. } => schluessel.erwaehnt(namen) || wert.erwaehnt(namen),
        }
    }
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
    /// `ZOI AUS(text)`: die ganze Zahl, die im Text steht, oder ein Fehler
    ZoiAus,
    /// `KOMMAZOI AUS(text)`: die Kommazahl, die im Text steht, oder ein Fehler
    KommazoiAus,
    /// `AUF STÖN(zahl, stellen)`: die Zahl als Text mit genau so vielen Kommastellen
    AufStellen,
    /// `OIS JSON(wert)`: der Wert als JSON-Text in einer Zeile
    AlsJson,
    /// `OIS SCHEES JSON(wert)`: ... mit Zeilen und Einrückung
    AlsJsonSchoen,
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
    /// `SÖWA`: das Objekt, an dem die Methode gerade läuft. `id` ist seine
    /// Nummer wie bei einer Variable, die Typprüfung trägt sie ein.
    Selbst { id: usize },
    /// `objekt.name(argumente)`: Aufruf einer Methode. `nummer` ist ihre
    /// Stelle in [`KlassenInfo::methoden`] der Klasse von `objekt`.
    Methode { objekt: Box<Ausdruck>, name: String, argumente: Vec<Ausdruck>, nummer: usize },
    /// `DE OIDN.name(argumente)`: die Fassung der Elternklasse aufrufen.
    /// `klasse` ist die Klasse, in der diese Fassung steht.
    ElternAufruf { name: String, argumente: Vec<Ausdruck>, klasse: u32 },
    /// `Gegner("Hansi", 100)`: ein neues Objekt; die Felder ohne eigenen
    /// Wert der Reihe nach. Entsteht in der Typprüfung aus einem Aufruf.
    Neu { klasse: u32, argumente: Vec<Ausdruck> },
    /// `NIX`: kein Wert. Der Typ richtet sich danach, wo es hinkommt.
    Nix,
    /// `x == NIX` (`ist`) oder `x != NIX`. Entsteht in der Typprüfung aus dem Vergleich.
    IstNix { innen: Box<Ausdruck>, ist: bool },
    /// Eine `VIELLEICHT`-Variable an einer Stelle, an der feststeht, dass sie
    /// nicht NIX ist (nach `WENN x != NIX DAUN`). Der Typ ist der des Werts.
    /// Die Typprüfung setzt das um die Variable herum.
    Sicher { innen: Box<Ausdruck> },
    /// `links ODA HOID rechts`: der Wert von links; ist dort NIX oder ein
    /// Fehler, der von rechts (und nur dann wird rechts ausgerechnet).
    OdaHoid { links: Box<Ausdruck>, rechts: Box<Ausdruck> },
    /// `PROBIER ausdruck`: der Wert; ist es ein Fehler, hört die Funktion
    /// hier auf und gibt ihn weiter.
    Probier { innen: Box<Ausdruck> },
    /// Eine Funktion ohne Namen: `FUNKTIAUN(x ZOI) => x * 2` (`kurz`) oder
    /// mit Block bis `PASST SCHO`. `fang` sind die Werte von außen, die sie
    /// mitnimmt, `nummer` macht sie eindeutig; beides trägt die Typprüfung ein.
    Lambda { funktion: Box<Funktion>, kurz: bool, fang: Vec<Fang>, nummer: usize },
    /// Eine Funktion mit Namen als Wert: `SOG AU f = verdoppel OIDA`.
    /// Entsteht in der Typprüfung aus einer "Variable", die eine Funktion ist.
    FunktionsWert { name: String },
    /// Aufruf einer Funktion, die ein Wert ist: `f(3)`, `liste[0](x)`, `ding.rueckruf(x)`.
    WertAufruf { funktion: Box<Ausdruck>, argumente: Vec<Ausdruck> },
    /// Ein neuer Wert, dessen Typ dabeisteht: `Stapel VON ZOI()`, `VAZEICHNIS VON KETTN UND ZOI()`.
    Erzeug { angabe: TypAngabe, argumente: Vec<Ausdruck> },
    /// `AUS JSON VON Punkt(text)`: der Wert, der im JSON-Text steht, oder ein Fehler.
    /// Der Typ des Ausdrucks ist `<angabe> ODA FEHLA`.
    AusJson { angabe: TypAngabe, text: Box<Ausdruck> },
    /// `objekt OIS Klasse`: das Objekt als diese Klasse, wenn es wirklich eines davon ist, sonst
    /// `NIX`. `klasse` ist die Nummer der Klasse; die Typprüfung trägt sie ein.
    Als { objekt: Box<Ausdruck>, angabe: TypAngabe, klasse: u32 },
    /// `NEBNBEI f(a, b)`: Die Funktion läuft in einem eigenen Thread, mit Kopien der Werte.
    /// `aufruf` ist ein [`AusdruckArt::Aufruf`]; der Wert des Ganzen ist die Aufgabe
    /// (`Aufgab VON <was f zurückgibt>` aus der Bibliothek `nebnbei`).
    Nebenbei { aufruf: Box<Ausdruck> },
    /// `WOAT AUF aufgab`: wartet, bis die Funktion fertig ist; der Wert ist, was sie zurückgegeben hat.
    WartAuf { aufgabe: Box<Ausdruck> },
}

/// Ein Wert von außen, den eine Funktion ohne Namen mitnimmt. Er wird
/// kopiert, wenn die Funktion entsteht; drinnen kann man ihn nur lesen.
#[derive(Debug, Clone, PartialEq)]
pub struct Fang {
    pub name: String,
    /// Wie man außen an den Wert kommt (eine Variable oder SÖWA).
    pub wert: Ausdruck,
    /// Die Nummer, unter der die Funktion ihn drinnen findet.
    pub id: usize,
    pub typ: Typ,
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
    /// `JE NOCHDEM wert BEI ... DAUN ... [SUNST ...] PASST SCHO`. Die Typprüfung macht daraus
    /// eine [`AnweisungArt::Folge`]: den Wert in einer eigenen Variable und eine Kette aus `Wenn`.
    JeNachdem { wert: Ausdruck, faelle: Vec<Fall>, sunst: Option<Vec<Anweisung>> },
    /// Mehrere Anweisungen hintereinander, die zusammengehören (ohne eigenen Block für Variablen).
    Folge(Vec<Anweisung>),
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
    /// `PROBIER [name =] wert ... WONNS SCHIEFGEHT [fehler_name] ... PASST SCHO`
    /// Hat `wert` geklappt, läuft `dann` (mit dem Wert in `name`), sonst
    /// `sunst` (mit dem Fehler in `fehler_name`).
    Probier {
        name: Option<String>,
        id: usize,
        wert: Ausdruck,
        dann: Vec<Anweisung>,
        fehler_name: Option<String>,
        fehler_id: usize,
        sunst: Vec<Anweisung>,
    },
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
    /// Bei einer Methode: die Nummer von `SÖWA` (wie bei einer Variable).
    /// Die Typprüfung trägt sie ein.
    pub selbst_id: usize,
    /// `FUNKTIAUN erster VON T(...)`: die Namen der Typen, die erst beim Aufruf feststehen.
    pub typ_parameter: Vec<String>,
    /// `... VON DRAUSSN "oida_x" OIDA`: Die Funktion hat keinen Rumpf, sie
    /// steckt unter diesem Namen in der Laufzeit. Nur in der mitgelieferten Bibliothek.
    pub aussen: Option<String>,
    /// `GHEIM FUNKTIAUN ...`: gilt nur in ihrer Datei, als Methode nur in ihrer Klasse.
    pub gheim: bool,
    /// Nur bei der Fassung einer Vorlage `VON DRAUSSN`: für jeden Parameter und zuletzt für die
    /// Rückgabe, ob dort in der Vorlage ein offener Typ steht. So ein Wert reist als Zeiger
    /// zusammen mit der Beschreibung seines Typs, weil die Laufzeit ihn sonst nicht verstünde.
    /// Die Typprüfung trägt das ein; sonst ist die Liste leer.
    pub per_zeiger: Vec<bool>,
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
    /// `STRUKTUR Paar VON A UND B`: die Namen der Typen, die erst beim Verwenden feststehen.
    pub typ_parameter: Vec<String>,
    pub felder: Vec<FeldDef>,
    /// `AUFZÖHLUNG Foab ... PASST SCHO`: die Werte, wie sie dastehen (sonst leer).
    pub werte: Vec<(String, Position)>,
    pub pos: Position,
    /// `GHEIM STRUKTUR ...`: gilt nur in ihrer Datei.
    pub gheim: bool,
}

/// Ein Fall in `JE NOCHDEM`: `BEI wert, wert DAUN ...`.
#[derive(Debug, Clone, PartialEq)]
pub struct Fall {
    pub werte: Vec<FallWert>,
    pub block: Vec<Anweisung>,
    pub pos: Position,
}

/// Wofür ein Fall gilt: für genau einen Wert oder für alles `von BIS bis`.
#[derive(Debug, Clone, PartialEq)]
pub enum FallWert {
    Gleich(Ausdruck),
    Bereich(Ausdruck, Ausdruck),
}

/// Ein Feld in `KLASSE ... PASST SCHO`.
#[derive(Debug, Clone, PartialEq)]
pub struct KlassenFeldDef {
    pub name: String,
    pub angabe: TypAngabe,
    /// Der Wert, mit dem das Feld beginnt, wenn einer dabeisteht (`lebn ZOI = 100`).
    pub standard: Option<Ausdruck>,
    pub pos: Position,
    /// `GHEIM lebn ZOI`: nur die Methoden der Klasse selbst kommen an das Feld.
    pub gheim: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KlassenDef {
    pub name: String,
    /// `KLASSE Stapel VON T`: die Namen der Typen, die erst beim Verwenden feststehen.
    pub typ_parameter: Vec<String>,
    /// Der Name hinter `ERBT VON`.
    pub eltern: Option<(String, Position)>,
    pub felder: Vec<KlassenFeldDef>,
    pub methoden: Vec<Funktion>,
    pub pos: Position,
    /// `GHEIM KLASSE ...`: gilt nur in ihrer Datei.
    pub gheim: bool,
    /// `SCHNITTSTÖ Bella ... PASST SCHO`: keine Klasse, sondern die Liste der Methoden, die eine
    /// Klasse haben muss, um eine `Bella` zu sein. Die Methoden haben hier keinen Rumpf.
    pub schnittstelle: bool,
    /// `KLASSE Hund KAUN Bella, Laufa`: die Schnittstellen, die die Klasse verspricht.
    pub kann: Vec<(String, Position)>,
}

/// `BRAUCH listen OIDA` oder `BRAUCH "gegner.oida" OIDA`: Die Funktionen,
/// Klassen und Strukturen von dort gelten auch hier.
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    /// Der Name der mitgelieferten Bibliothek oder der Pfad der Datei.
    pub ziel: String,
    /// Ohne Anführungszeichen: eine mitgelieferte Bibliothek.
    pub bibliothek: bool,
    pub pos: Position,
    /// `BRAUCH "wüafl.oida" OIS w`: Was von dort kommt, heißt hier `w.name`.
    pub raum: Option<String>,
}

/// `FIX SCHWERKRAFT = 9.81 OIDA`: ein fester Wert, der überall gilt, auch in
/// Funktionen und in anderen Dateien. Die Typprüfung setzt ihn überall ein,
/// wo sein Name steht; danach gibt es ihn nicht mehr.
#[derive(Debug, Clone, PartialEq)]
pub struct Konstante {
    pub name: String,
    pub angabe: Option<TypAngabe>,
    pub wert: Ausdruck,
    pub pos: Position,
    /// `GHEIM FIX ...`: gilt nur in seiner Datei.
    pub gheim: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Programm {
    /// Was mit BRAUCH dazugeholt wird. Der Treiber lädt die Dateien und hängt
    /// ihren Inhalt hier an, bevor die Typprüfung beginnt.
    pub importe: Vec<Import>,
    pub konstanten: Vec<Konstante>,
    pub strukturen: Vec<StrukturDef>,
    /// Die Klassen. Ihre Stelle hier ist ihre Nummer in [`Typen::klassen`].
    pub klassen: Vec<KlassenDef>,
    pub funktionen: Vec<Funktion>,
    /// Die Anweisungen außerhalb von Funktionen: das Hauptprogramm.
    pub haupt: Vec<Anweisung>,
    /// Namen, die es gibt, aber nicht so oder nicht hier (GHEIM in einer anderen Datei, oder nur
    /// in einem Namensraum): der Name, wie er fälschlich dasteht, und was die Meldung dazu sagt.
    pub hinweise: std::collections::HashMap<String, String>,
    /// Alle Strukturen und Listentypen; die Typprüfung füllt die Tabelle.
    pub typen: Typen,
}
