//! Typprüfung mit Typerkennung (Inferenz).
//!
//! Geht einmal durch den Syntaxbaum und
//!  - löst die Typen auf, die im Quelltext stehen (Strukturen, Listen),
//!  - trägt bei jedem Ausdruck den Typ ein,
//!  - gibt jeder Variable eine Nummer (`id`), damit die IR-Erzeugung nicht
//!    mehr nach Namen suchen muss,
//!  - fügt die Umwandlung ZOI -> KOMMAZOI ein, wo eine ganze Zahl auf eine
//!    Kommazahl trifft,
//!  - sammelt alle Fehler ein, statt beim ersten aufzuhören.

use std::collections::HashMap;

use crate::ast::*;
use crate::fehler::{Fehler, Position};
use crate::woerterbuch::{Schluessel, Woerterbuch};

struct Signatur {
    parameter: Vec<Typ>,
    rueckgabe: Typ,
}

struct Variable {
    id: usize,
    typ: Typ,
    pos: Position,
}

struct Pruefer<'a> {
    wb: &'a Woerterbuch,
    fehler: Vec<Fehler>,
    signaturen: HashMap<String, Signatur>,
    /// Sichtbare Variablen, der innerste Block steht hinten.
    bereiche: Vec<HashMap<String, Variable>>,
    naechste_id: usize,
    /// Name und Rückgabetyp der Funktion, in der wir gerade sind.
    funktion: Option<(String, Typ)>,
    /// In wie vielen Schleifen wir gerade stecken (für HAU OB und WEIDA).
    schleifen: usize,
    /// Alle Strukturen und Listentypen des Programms.
    typen: Typen,
    /// Name einer Struktur -> ihre Nummer in `typen.strukturen`.
    struktur_nummern: HashMap<String, u32>,
}

/// Das Wort, unter dem eine eingebaute Funktion im Wörterbuch steht.
fn wort_von(funktion: Eingebaut) -> Schluessel {
    match funktion {
        Eingebaut::Lies => Schluessel::Read,
        Eingebaut::Laenge => Schluessel::Length,
        Eingebaut::Stueck => Schluessel::Substring,
        Eingebaut::Suche => Schluessel::Find,
        Eingebaut::Gross => Schluessel::Upper,
        Eingebaut::Klein => Schluessel::Lower,
        Eingebaut::Stutz => Schluessel::Trim,
        Eingebaut::Ersetz => Schluessel::Replace,
        Eingebaut::Wurzel => Schluessel::Sqrt,
        Eingebaut::Hoch => Schluessel::Pow,
        Eingebaut::Betrag => Schluessel::Abs,
        Eingebaut::Min => Schluessel::Min,
        Eingebaut::Max => Schluessel::Max,
        Eingebaut::Sinus => Schluessel::Sin,
        Eingebaut::Kosinus => Schluessel::Cos,
        Eingebaut::Winkel => Schluessel::Atan2,
        Eingebaut::Exp => Schluessel::Exp,
        Eingebaut::Abrunden => Schluessel::Floor,
        Eingebaut::Aufrunden => Schluessel::Ceil,
        Eingebaut::Runden => Schluessel::Round,
        Eingebaut::Zufall => Schluessel::Random,
        Eingebaut::Samen => Schluessel::Seed,
        Eingebaut::Dazu => Schluessel::Add,
        Eingebaut::Raus => Schluessel::Remove,
    }
}

/// Welche Werte eine eingebaute Funktion nimmt und was sie zurückgibt.
/// Mehrere Zeilen heißt: Es gibt sie in mehreren Fassungen; die erste, die passt, gilt.
/// Wo KOMMAZOI verlangt ist, geht auch eine ZOI.
/// Die Funktionen für Listen stehen nicht hier, weil ihr Typ von der Liste abhängt.
fn fassungen(funktion: Eingebaut) -> &'static [(&'static [Typ], Typ)] {
    use Typ::{Kettn as T, Kommazoi as K, Nix, Zoi as Z};
    match funktion {
        Eingebaut::Lies => &[(&[], T), (&[T], T)],
        Eingebaut::Laenge => &[(&[T], Z)],
        Eingebaut::Stueck => &[(&[T, Z], T), (&[T, Z, Z], T)],
        Eingebaut::Suche => &[(&[T, T], Z)],
        Eingebaut::Gross | Eingebaut::Klein | Eingebaut::Stutz => &[(&[T], T)],
        Eingebaut::Ersetz => &[(&[T, T, T], T)],
        Eingebaut::Wurzel
        | Eingebaut::Sinus
        | Eingebaut::Kosinus
        | Eingebaut::Exp
        | Eingebaut::Abrunden
        | Eingebaut::Aufrunden
        | Eingebaut::Runden => &[(&[K], K)],
        Eingebaut::Hoch | Eingebaut::Winkel => &[(&[K, K], K)],
        Eingebaut::Betrag => &[(&[Z], Z), (&[K], K)],
        Eingebaut::Min | Eingebaut::Max => &[(&[Z, Z], Z), (&[K, K], K)],
        Eingebaut::Zufall => &[(&[], K), (&[Z, Z], Z)],
        Eingebaut::Samen => &[(&[Z], Nix)],
        Eingebaut::Dazu | Eingebaut::Raus => unreachable!("Listenfunktionen werden eigens geprüft"),
    }
}

/// Was beim Rechnen oder Vergleichen von `l` und `r` herauskommt, wenn es geht.
/// (Dass aus einer ZOI neben einer KOMMAZOI eine KOMMAZOI wird, ist hier schon eingerechnet.)
fn binaer_ergebnis(op: BinaerOp, l: Typ, r: Typ) -> Option<Typ> {
    let zahl = |t: Typ| t == Typ::Zoi || t == Typ::Kommazoi;
    let zahlen =
        (zahl(l) && zahl(r)).then(|| if l == Typ::Kommazoi || r == Typ::Kommazoi { Typ::Kommazoi } else { Typ::Zoi });
    let einfach = matches!(l, Typ::Zoi | Typ::Kommazoi | Typ::Jona | Typ::Kettn);
    match op {
        BinaerOp::Plus => zahlen.or((l == Typ::Kettn && r == Typ::Kettn).then_some(Typ::Kettn)),
        BinaerOp::Minus | BinaerOp::Mal | BinaerOp::Durch | BinaerOp::Rest => zahlen,
        BinaerOp::Gleich | BinaerOp::Ungleich => (zahlen.is_some() || (l == r && einfach)).then_some(Typ::Jona),
        BinaerOp::Kleiner | BinaerOp::KleinerGleich | BinaerOp::Groesser | BinaerOp::GroesserGleich => {
            (zahlen.is_some() || (l == Typ::Kettn && r == Typ::Kettn)).then_some(Typ::Jona)
        }
        BinaerOp::Und | BinaerOp::Oda => (l == Typ::Jona && r == Typ::Jona).then_some(Typ::Jona),
    }
}

/// Prüft das Programm und ergänzt den Syntaxbaum um Typen und Variablennummern.
pub fn pruefen(programm: &mut Programm, wb: &Woerterbuch) -> Result<(), Vec<Fehler>> {
    let mut p = Pruefer {
        wb,
        fehler: Vec::new(),
        signaturen: HashMap::new(),
        bereiche: Vec::new(),
        naechste_id: 0,
        funktion: None,
        schleifen: 0,
        typen: Typen::default(),
        struktur_nummern: HashMap::new(),
    };

    p.strukturen(&programm.strukturen);

    // Alle Funktionen kennenlernen, damit die Reihenfolge egal ist.
    for f in &mut programm.funktionen {
        for parameter in &mut f.parameter {
            parameter.typ = p.typ_aus(&parameter.angabe);
        }
        f.rueckgabe = f.rueckgabe_angabe.as_ref().map_or(Typ::Nix, |angabe| p.typ_aus(angabe));
        if p.struktur_nummern.contains_key(&f.name) {
            let meldung = format!(
                "'{}' gibt's scho ois {}, des kaun ned a no a {} sei.",
                f.name,
                p.wort(Schluessel::Struct),
                p.wort(Schluessel::Function)
            );
            p.melde(f.pos, meldung);
            continue;
        }
        if p.signaturen.contains_key(&f.name) {
            let meldung = format!("De {} '{}' gibt's scho. Zwamoi geht ned.", p.wort(Schluessel::Function), f.name);
            p.melde(f.pos, meldung);
            continue;
        }
        p.signaturen.insert(
            f.name.clone(),
            Signatur { parameter: f.parameter.iter().map(|x| x.typ).collect(), rueckgabe: f.rueckgabe },
        );
    }

    for f in &mut programm.funktionen {
        p.funktion(f);
    }

    p.bereiche = vec![HashMap::new()];
    p.funktion = None;
    p.schleifen = 0;
    for a in &mut programm.haupt {
        p.anweisung(a);
    }

    if p.fehler.is_empty() {
        programm.typen = p.typen;
        Ok(())
    } else {
        p.fehler.sort_by_key(|f| (f.pos.zeile, f.pos.spalte));
        Err(p.fehler)
    }
}

impl Pruefer<'_> {
    fn melde(&mut self, pos: Position, meldung: impl Into<String>) {
        self.fehler.push(Fehler::neu(pos, meldung));
    }

    fn wort(&self, schluessel: Schluessel) -> &str {
        self.wb.wort(schluessel)
    }

    /// Wie ein Typ heißt: "ZOI", "Punkt", "LISTN VON ZOI".
    fn typ_wort(&self, typ: Typ) -> String {
        match typ {
            Typ::Zoi => self.wort(Schluessel::Int).to_string(),
            Typ::Kommazoi => self.wort(Schluessel::Float).to_string(),
            Typ::Jona => self.wort(Schluessel::Boolean).to_string(),
            Typ::Kettn => self.wort(Schluessel::Text).to_string(),
            Typ::Struktur(_) => self.typen.struktur(typ).map_or_else(String::new, |s| s.name.clone()),
            Typ::Liste(_) => format!(
                "{} {} {}",
                self.wort(Schluessel::List),
                self.wort(Schluessel::From),
                self.typ_wort(self.typen.element(typ).unwrap_or(Typ::Fehler))
            ),
            Typ::LeereListe => format!("laare {}", self.wort(Schluessel::List)),
            Typ::Nix | Typ::Fehler => "nix".to_string(),
        }
    }

    /// Wie ein Typ in einer Meldung heißt, mit Artikel: "a ZOI".
    fn typ_name(&self, typ: Typ) -> String {
        match typ {
            Typ::Nix | Typ::Fehler => "nix".to_string(),
            _ => format!("a {}", self.typ_wort(typ)),
        }
    }

    /// Macht aus einem Typ, wie er im Quelltext steht, den Typ selbst.
    fn typ_aus(&mut self, angabe: &TypAngabe) -> Typ {
        match angabe {
            TypAngabe::Einfach(typ) => *typ,
            TypAngabe::Name { name, pos } => match self.struktur_nummern.get(name) {
                Some(nummer) => Typ::Struktur(*nummer),
                None => {
                    let meldung = match self.wb.festes_wort(name) {
                        Some(schluessel) => format!("Des haaßt {}, ned {}!", self.wort(schluessel), name),
                        None => format!(
                            "An Typ '{name}' kenn i ned. Gibt's a {} mit dem Nom?",
                            self.wort(Schluessel::Struct)
                        ),
                    };
                    self.melde(*pos, meldung);
                    Typ::Fehler
                }
            },
            TypAngabe::Liste(innen) => match self.typ_aus(innen) {
                Typ::Fehler => Typ::Fehler,
                element => self.typen.liste_von(element),
            },
        }
    }

    /// Steht der Typ ganz fest? Bei `[]` ohne Angabe ist offen, wovon es eine Liste ist.
    fn fertig(&self, typ: Typ) -> bool {
        match typ {
            Typ::LeereListe => false,
            Typ::Liste(_) => self.typen.element(typ).is_some_and(|e| self.fertig(e)),
            _ => true,
        }
    }

    /// Meldet eine Liste, von der nicht klar ist, was drin sein soll.
    fn fertig_verlangt(&mut self, ausdruck: &Ausdruck) -> bool {
        if self.fertig(ausdruck.typ) {
            return true;
        }
        let meldung = format!(
            "Bei [] waaß i ned, wos fia a {liste} des sei soll. Sog's dazua, zum Beispü: {} x {liste} {} {} = [] {}",
            self.wort(Schluessel::Declare),
            self.wort(Schluessel::From),
            self.wort(Schluessel::Int),
            self.wort(Schluessel::Oida),
            liste = self.wort(Schluessel::List),
        );
        self.melde(ausdruck.pos, meldung);
        false
    }

    // -------------------------------------------------------- Strukturen

    /// Lernt alle Strukturen kennen: erst die Namen (damit die Reihenfolge
    /// egal ist), dann die Felder.
    fn strukturen(&mut self, definitionen: &[StrukturDef]) {
        // Welche Definition zu welcher Nummer gehört (doppelte Namen fallen weg).
        let mut gueltig: Vec<&StrukturDef> = Vec::new();
        for s in definitionen {
            if self.struktur_nummern.contains_key(&s.name) {
                let meldung = format!("De {} '{}' gibt's scho. Zwamoi geht ned.", self.wort(Schluessel::Struct), s.name);
                self.melde(s.pos, meldung);
                continue;
            }
            self.struktur_nummern.insert(s.name.clone(), gueltig.len() as u32);
            self.typen.strukturen.push(StrukturInfo { name: s.name.clone(), felder: Vec::new() });
            gueltig.push(s);
        }
        for (nummer, s) in gueltig.iter().enumerate() {
            let mut felder: Vec<(String, Typ)> = Vec::new();
            for feld in &s.felder {
                let typ = self.typ_aus(&feld.angabe);
                if felder.iter().any(|(name, _)| *name == feld.name) {
                    let meldung = format!("Des Föd '{}' gibt's in '{}' scho.", feld.name, s.name);
                    self.melde(feld.pos, meldung);
                    continue;
                }
                felder.push((feld.name.clone(), typ));
            }
            self.typen.strukturen[nummer].felder = felder;
        }
        // Eine Struktur, die sich selbst als Wert enthält, wäre unendlich groß.
        for (nummer, s) in gueltig.iter().enumerate() {
            for i in 0..self.typen.strukturen[nummer].felder.len() {
                let (name, typ) = self.typen.strukturen[nummer].felder[i].clone();
                let Some(feld) = s.felder.iter().find(|f| f.name == name) else { continue };
                if self.enthaelt(typ, nummer as u32, 0) {
                    let meldung = format!(
                        "De {} '{}' kaun si ned söwa enthoitn, de warat unendlich groß. Nimm a {} dafia.",
                        self.wort(Schluessel::Struct),
                        s.name,
                        self.wort(Schluessel::List)
                    );
                    self.melde(feld.pos, meldung);
                    // Den Kreis auftrennen, damit später nichts endlos läuft.
                    self.typen.strukturen[nummer].felder[i].1 = Typ::Fehler;
                }
            }
        }
    }

    /// Steckt in einem Wert vom Typ `typ` (direkt, nicht über eine Liste) die Struktur `gesucht`?
    fn enthaelt(&self, typ: Typ, gesucht: u32, tiefe: usize) -> bool {
        match typ {
            Typ::Struktur(n) if n == gesucht => true,
            // Die Tiefe begrenzt die Suche, falls anderswo ein Kreis steckt, der gerade gemeldet wird.
            Typ::Struktur(n) if tiefe < self.typen.strukturen.len() => {
                self.typen.strukturen[n as usize].felder.iter().any(|(_, t)| self.enthaelt(*t, gesucht, tiefe + 1))
            }
            _ => false,
        }
    }

    // --------------------------------------------------------- Variablen

    fn neue_variable(&mut self, name: &str, typ: Typ, pos: Position) -> usize {
        let id = self.naechste_id;
        self.naechste_id += 1;
        let bereich = self.bereiche.last_mut().expect("es gibt immer einen Bereich");
        if let Some(alt) = bereich.get(name) {
            let zeile = alt.pos.zeile;
            self.melde(pos, format!("'{name}' gibt's do scho (Zeile {zeile}). Zwamoi ausogn geht ned."));
        } else {
            bereich.insert(name.to_string(), Variable { id, typ, pos });
        }
        id
    }

    fn finde_variable(&self, name: &str) -> Option<&Variable> {
        self.bereiche.iter().rev().find_map(|b| b.get(name))
    }

    fn unbekannte_variable(&mut self, name: &str, pos: Position) {
        let meldung =
            format!("'{name}' kenn i ned. Host des mit {} augsogt?", self.wort(Schluessel::Declare));
        self.melde(pos, meldung);
    }

    // -------------------------------------------------------- Funktionen

    fn funktion(&mut self, f: &mut Funktion) {
        self.bereiche = vec![HashMap::new()];
        self.schleifen = 0;
        self.funktion = Some((f.name.clone(), f.rueckgabe));
        for parameter in &mut f.parameter {
            parameter.id = self.neue_variable(&parameter.name, parameter.typ, parameter.pos);
        }
        for a in &mut f.rumpf {
            self.anweisung(a);
        }
        if f.rueckgabe != Typ::Nix && f.rueckgabe != Typ::Fehler && !gibt_sicher_zruck(&f.rumpf) {
            let meldung = format!(
                "De {} '{}' muass {} zruckgebn, oba ned jeda Weg hört mit {} auf.",
                self.wort(Schluessel::Function),
                f.name,
                self.typ_name(f.rueckgabe),
                self.wort(Schluessel::Return)
            );
            self.melde(f.pos, meldung);
        }
    }

    fn block(&mut self, anweisungen: &mut [Anweisung]) {
        self.bereiche.push(HashMap::new());
        for a in anweisungen {
            self.anweisung(a);
        }
        self.bereiche.pop();
    }

    // ------------------------------------------------------- Anweisungen

    fn anweisung(&mut self, anweisung: &mut Anweisung) {
        let pos = anweisung.pos;
        match &mut anweisung.art {
            AnweisungArt::Deklaration { name, angabe, wert, id } => {
                let wert_typ = self.ausdruck(wert);
                let verlangt = angabe.as_ref().map(|a| self.typ_aus(a));
                let mut var_typ = verlangt.unwrap_or(wert_typ);
                if wert_typ == Typ::Nix {
                    self.melde(wert.pos, "Des gibt nix zruck, des kaunst in ka Variable einetuan.");
                    var_typ = Typ::Fehler;
                } else if !self.anpassen(wert, var_typ) {
                    let meldung = format!(
                        "'{name}' soll {} sei, oba des do is {}.",
                        self.typ_name(var_typ),
                        self.typ_name(wert_typ)
                    );
                    self.melde(wert.pos, meldung);
                } else if !self.fertig_verlangt(wert) {
                    var_typ = Typ::Fehler;
                }
                // Erst jetzt eintragen: "SOG AU x = x" soll das äußere x meinen.
                *id = self.neue_variable(name, var_typ, pos);
            }
            AnweisungArt::Zuweisung { ziel, op, wert } => {
                let wert_typ = self.ausdruck(wert);
                let ziel_typ = self.ausdruck(ziel);
                if ziel_typ == Typ::Fehler || wert_typ == Typ::Fehler {
                    return;
                }
                if !ist_ziel(ziel) {
                    let meldung = format!(
                        "Do kaun ma nix zuaweisn. Links vom = ghört a Variable, a Föd oda a Stö in ana {}.",
                        self.wort(Schluessel::List)
                    );
                    self.melde(ziel.pos, meldung);
                    return;
                }
                // Was am Ende im Ziel landen soll: der Wert selbst oder das Ergebnis der Rechnung.
                let ergebnis = match op {
                    None => Some(wert_typ),
                    Some(op) => binaer_ergebnis(*op, ziel_typ, wert_typ),
                };
                let Some(ergebnis) = ergebnis else {
                    let meldung = format!(
                        "{} {}= {}? Des geht si ned aus.",
                        self.typ_name(ziel_typ).replacen("a ", "A ", 1),
                        op.map_or("", BinaerOp::zeichen),
                        self.typ_name(wert_typ)
                    );
                    self.melde(pos, meldung);
                    return;
                };
                let passt = match op {
                    None => self.anpassen(wert, ziel_typ),
                    // Bei der Rechnung muss wieder der Typ des Ziels herauskommen.
                    Some(_) if ergebnis == ziel_typ => {
                        self.anpassen(wert, ziel_typ);
                        true
                    }
                    Some(_) => false,
                };
                if !passt {
                    let meldung = format!(
                        "'{}' is {}, do kaunst {} einetuan.",
                        ziel_text(ziel),
                        self.typ_name(ziel_typ),
                        self.typ_name(ergebnis).replacen("a ", "ka ", 1)
                    );
                    self.melde(wert.pos, meldung);
                }
            }
            AnweisungArt::Wenn { bedingung, dann, sunst } => {
                self.bedingung(bedingung, Schluessel::If);
                self.block(dann);
                self.block(sunst);
            }
            AnweisungArt::Solong { bedingung, rumpf } => {
                self.bedingung(bedingung, Schluessel::While);
                self.schleifen += 1;
                self.block(rumpf);
                self.schleifen -= 1;
            }
            AnweisungArt::Fia { name, id, von, bis, schritt, rumpf } => {
                self.zoi_verlangt(von, Schluessel::From);
                self.zoi_verlangt(bis, Schluessel::To);
                if let Some(schritt) = schritt {
                    self.zoi_verlangt(schritt, Schluessel::Step);
                    if matches!(schritt.art, AusdruckArt::Zoi(0)) {
                        let meldung =
                            format!("Mit {} 0 kummt de Schleifn nie weida.", self.wort(Schluessel::Step));
                        self.melde(schritt.pos, meldung);
                    }
                }
                // Die Zählvariable gilt nur in der Schleife.
                self.bereiche.push(HashMap::new());
                *id = self.neue_variable(name, Typ::Zoi, pos);
                self.schleifen += 1;
                self.block(rumpf);
                self.schleifen -= 1;
                self.bereiche.pop();
            }
            AnweisungArt::FiaJede { name, id, liste, rumpf } => {
                let listen_typ = self.ausdruck(liste);
                let mut element = Typ::Fehler;
                if listen_typ != Typ::Fehler && self.fertig_verlangt(liste) {
                    match self.typen.element(listen_typ) {
                        Some(typ) => element = typ,
                        None => {
                            let meldung = format!(
                                "{} {} geht nur mit ana {}, des do is oba {}.",
                                self.wort(Schluessel::For),
                                self.wort(Schluessel::Each),
                                self.wort(Schluessel::List),
                                self.typ_name(listen_typ)
                            );
                            self.melde(liste.pos, meldung);
                        }
                    }
                }
                // Das Element gilt nur in der Schleife.
                self.bereiche.push(HashMap::new());
                *id = self.neue_variable(name, element, pos);
                self.schleifen += 1;
                self.block(rumpf);
                self.schleifen -= 1;
                self.bereiche.pop();
            }
            AnweisungArt::HauOb => self.nur_in_schleife(Schluessel::Break, pos),
            AnweisungArt::Weida => self.nur_in_schleife(Schluessel::Continue, pos),
            AnweisungArt::GibZruck { wert } => {
                let wert_typ = wert.as_mut().map(|w| self.ausdruck(w));
                let Some((fname, rueckgabe)) = self.funktion.clone() else {
                    let meldung = format!(
                        "{} geht nur in ana {}.",
                        self.wort(Schluessel::Return),
                        self.wort(Schluessel::Function)
                    );
                    self.melde(pos, meldung);
                    return;
                };
                match (wert, rueckgabe) {
                    (None, Typ::Nix) => {}
                    (None, _) => {
                        let meldung = format!(
                            "De {} '{fname}' muass {} zruckgebn. Noch {} fehlt da Wert.",
                            self.wort(Schluessel::Function),
                            self.typ_name(rueckgabe),
                            self.wort(Schluessel::Return)
                        );
                        self.melde(pos, meldung);
                    }
                    (Some(wert), Typ::Nix) => {
                        let meldung = format!(
                            "De {} '{fname}' gibt nix zruck. Schreib {} {} dazua, wennst wos zruckgebn wüst.",
                            self.wort(Schluessel::Function),
                            self.wort(Schluessel::Returns),
                            self.typ_wort(wert_typ.unwrap())
                        );
                        self.melde(wert.pos, meldung);
                    }
                    (Some(wert), _) => {
                        if !self.anpassen(wert, rueckgabe) {
                            let meldung = format!(
                                "De {} '{fname}' gibt {} zruck, ned {}.",
                                self.wort(Schluessel::Function),
                                self.typ_name(rueckgabe),
                                self.typ_name(wert_typ.unwrap())
                            );
                            self.melde(wert.pos, meldung);
                        }
                    }
                }
            }
            AnweisungArt::Schreib { argumente } => {
                for argument in argumente {
                    let typ = self.ausdruck(argument);
                    if typ == Typ::Nix {
                        let meldung = format!(
                            "Des gibt nix zruck, do gibt's nix zum {}.",
                            self.wort(Schluessel::Print)
                        );
                        self.melde(argument.pos, meldung);
                    } else if typ != Typ::Fehler {
                        self.fertig_verlangt(argument);
                    }
                }
            }
            AnweisungArt::Ausdruck(ausdruck) => {
                self.ausdruck(ausdruck);
                if !matches!(ausdruck.art, AusdruckArt::Aufruf { .. } | AusdruckArt::Eingebaut { .. }) {
                    self.melde(ausdruck.pos, "Des rechnet wos aus und schmeißt's glei wieda weg. Wos soll des?");
                }
            }
        }
    }

    /// HAU OB und WEIDA brauchen eine Schleife um sich herum.
    fn nur_in_schleife(&mut self, wort: Schluessel, pos: Position) {
        if self.schleifen == 0 {
            let meldung = format!("{} geht nur in ana Schleifn.", self.wort(wort));
            self.melde(pos, meldung);
        }
    }

    /// VON, BIS und SCHRITT einer Zählschleife müssen ganze Zahlen sein.
    fn zoi_verlangt(&mut self, ausdruck: &mut Ausdruck, wort: Schluessel) {
        let typ = self.ausdruck(ausdruck);
        if typ != Typ::Zoi && typ != Typ::Fehler {
            let meldung = format!(
                "Noch {} brauch i {}, des do is oba {}.",
                self.wort(wort),
                self.typ_name(Typ::Zoi),
                self.typ_name(typ)
            );
            self.melde(ausdruck.pos, meldung);
        }
    }

    fn bedingung(&mut self, bedingung: &mut Ausdruck, wort: Schluessel) {
        let typ = self.ausdruck(bedingung);
        if typ != Typ::Jona && typ != Typ::Fehler {
            let meldung = format!(
                "Noch {} brauch i {} ({} oda {}), des do is oba {}.",
                self.wort(wort),
                self.typ_name(Typ::Jona),
                self.wort(Schluessel::True),
                self.wort(Schluessel::False),
                self.typ_name(typ)
            );
            self.melde(bedingung.pos, meldung);
        }
    }

    // --------------------------------------------------------- Ausdrücke

    /// Passt der Ausdruck dorthin, wo `ziel` verlangt wird? Aus einer ZOI
    /// wird dabei von selbst eine KOMMAZOI, und eine Liste, die gerade
    /// hingeschrieben wird (`[]`, `[1, 2]`), richtet sich nach dem Ziel.
    fn anpassen(&mut self, ausdruck: &mut Ausdruck, ziel: Typ) -> bool {
        if ausdruck.typ == ziel || ausdruck.typ == Typ::Fehler || ziel == Typ::Fehler {
            return true;
        }
        if ausdruck.typ == Typ::Zoi && ziel == Typ::Kommazoi {
            einpacken(ausdruck, Typ::Kommazoi);
            return true;
        }
        if let Some(element) = self.typen.element(ziel)
            && let AusdruckArt::ListenWert { elemente } = &mut ausdruck.art
        {
            let mut alle = true;
            for e in elemente.iter_mut() {
                alle &= self.anpassen(e, element);
            }
            if alle {
                ausdruck.typ = ziel;
                return true;
            }
        }
        false
    }

    /// Bestimmt den Typ eines Ausdrucks und trägt ihn ein.
    fn ausdruck(&mut self, ausdruck: &mut Ausdruck) -> Typ {
        let pos = ausdruck.pos;
        // Heißt die "Funktion" wie eine Struktur, wird ein neuer Wert gebaut.
        if let AusdruckArt::Aufruf { name, .. } = &ausdruck.art
            && let Some(&nummer) = self.struktur_nummern.get(name)
        {
            return self.bau(ausdruck, nummer);
        }
        let typ = match &mut ausdruck.art {
            AusdruckArt::Zoi(_) => Typ::Zoi,
            AusdruckArt::Kommazoi(_) => Typ::Kommazoi,
            AusdruckArt::Jona(_) => Typ::Jona,
            AusdruckArt::Text(_) => Typ::Kettn,
            AusdruckArt::Variable { name, id } => match self.finde_variable(name).map(|v| (v.id, v.typ)) {
                Some((var_id, typ)) => {
                    *id = var_id;
                    typ
                }
                None => {
                    let name = name.clone();
                    self.unbekannte_variable(&name, pos);
                    Typ::Fehler
                }
            },
            AusdruckArt::Unaer { op, innen } => {
                let innen_typ = self.ausdruck(innen);
                match (*op, innen_typ) {
                    (_, Typ::Fehler) => Typ::Fehler,
                    (UnaerOp::Minus, Typ::Zoi | Typ::Kommazoi) => innen_typ,
                    (UnaerOp::Ned, Typ::Jona) => Typ::Jona,
                    (UnaerOp::Minus, _) => {
                        let meldung = format!("A Minus vua {}? Des geht nur bei Zoin.", self.typ_name(innen_typ));
                        self.melde(pos, meldung);
                        Typ::Fehler
                    }
                    (UnaerOp::Ned, _) => {
                        let meldung = format!(
                            "{} geht nur mit {}, des do is oba {}.",
                            self.wort(Schluessel::Not),
                            self.typ_name(Typ::Jona),
                            self.typ_name(innen_typ)
                        );
                        self.melde(pos, meldung);
                        Typ::Fehler
                    }
                }
            }
            AusdruckArt::Binaer { op, links, rechts } => {
                let op = *op;
                let l = self.ausdruck(links);
                let r = self.ausdruck(rechts);
                self.binaer(op, links, rechts, l, r, pos)
            }
            AusdruckArt::Aufruf { name, argumente } => {
                let typen: Vec<Typ> = argumente.iter_mut().map(|a| self.ausdruck(a)).collect();
                let name = name.clone();
                self.aufruf(&name, argumente, &typen, pos)
            }
            AusdruckArt::Eingebaut { funktion, argumente } => {
                let funktion = *funktion;
                self.eingebaut(funktion, argumente, pos)
            }
            AusdruckArt::Umwandlung { ziel, innen } => {
                let ziel = *ziel;
                let von = self.ausdruck(innen);
                let erlaubt = von == ziel
                    || matches!(
                        (von, ziel),
                        (Typ::Zoi, Typ::Kommazoi)
                            | (Typ::Kommazoi, Typ::Zoi)
                            | (Typ::Jona, Typ::Zoi)
                            | (Typ::Kettn, Typ::Zoi)
                            | (Typ::Kettn, Typ::Kommazoi)
                            | (Typ::Zoi, Typ::Kettn)
                            | (Typ::Kommazoi, Typ::Kettn)
                            | (Typ::Jona, Typ::Kettn)
                    );
                if von == Typ::Fehler {
                    Typ::Fehler
                } else if erlaubt {
                    ziel
                } else {
                    let meldung = format!(
                        "Aus {} kaun i {} mochn.",
                        self.typ_name(von).replacen("a ", "ana ", 1),
                        self.typ_name(ziel).replacen("a ", "ka ", 1)
                    );
                    self.melde(pos, meldung);
                    Typ::Fehler
                }
            }
            AusdruckArt::Feld { objekt, name, nummer } => {
                let objekt_typ = self.ausdruck(objekt);
                if objekt_typ == Typ::Fehler {
                    Typ::Fehler
                } else if let Some(struktur) = self.typen.struktur(objekt_typ).cloned() {
                    match struktur.felder.iter().position(|(feld, _)| feld == name) {
                        Some(stelle) => {
                            *nummer = stelle;
                            struktur.felder[stelle].1
                        }
                        None => {
                            let namen: Vec<&str> = struktur.felder.iter().map(|(feld, _)| feld.as_str()).collect();
                            let meldung = if namen.is_empty() {
                                format!("A Föd '{name}' gibt's in '{}' ned, de hot gor kane Föda.", struktur.name)
                            } else {
                                format!(
                                    "A Föd '{name}' gibt's in '{}' ned. Es gibt: {}.",
                                    struktur.name,
                                    namen.join(", ")
                                )
                            };
                            self.melde(pos, meldung);
                            Typ::Fehler
                        }
                    }
                } else {
                    let meldung = format!("{} hot kane Föda.", self.typ_name(objekt_typ).replacen("a ", "A ", 1));
                    self.melde(pos, meldung);
                    Typ::Fehler
                }
            }
            AusdruckArt::Stelle { liste, index } => {
                let listen_typ = self.ausdruck(liste);
                let index_typ = self.ausdruck(index);
                if index_typ != Typ::Zoi && index_typ != Typ::Fehler {
                    let meldung = format!(
                        "De Stö in ana {} muass {} sei, des do is oba {}.",
                        self.wort(Schluessel::List),
                        self.typ_name(Typ::Zoi),
                        self.typ_name(index_typ)
                    );
                    self.melde(index.pos, meldung);
                }
                if listen_typ == Typ::Fehler || !self.fertig_verlangt(liste) {
                    Typ::Fehler
                } else if let Some(element) = self.typen.element(listen_typ) {
                    element
                } else {
                    let mut meldung = format!(
                        "Mit [...] kummt ma nur in a {} eine, des do is oba {}.",
                        self.wort(Schluessel::List),
                        self.typ_name(listen_typ)
                    );
                    if listen_typ == Typ::Kettn {
                        meldung.push_str(&format!(" Fia Texte gibt's {}.", self.wort(Schluessel::Substring)));
                    }
                    self.melde(pos, meldung);
                    Typ::Fehler
                }
            }
            AusdruckArt::ListenWert { elemente } => self.listen_wert(elemente),
            AusdruckArt::Bau { struktur, .. } => Typ::Struktur(*struktur),
        };
        ausdruck.typ = typ;
        typ
    }

    /// `[a, b, c]`: Alle Elemente müssen denselben Typ haben.
    fn listen_wert(&mut self, elemente: &mut [Ausdruck]) -> Typ {
        let typen: Vec<Typ> = elemente.iter_mut().map(|e| self.ausdruck(e)).collect();
        if typen.contains(&Typ::Fehler) {
            return Typ::Fehler;
        }
        for (element, typ) in elemente.iter().zip(&typen) {
            if *typ == Typ::Nix {
                let meldung =
                    format!("Des gibt nix zruck, des kaunst in ka {} einetuan.", self.wort(Schluessel::List));
                self.melde(element.pos, meldung);
                return Typ::Fehler;
            }
        }
        // Der gemeinsame Typ: der erste, der feststeht; ZOI und KOMMAZOI gemischt ergibt KOMMAZOI.
        let Some(mut gemeinsam) = typen.iter().copied().find(|t| self.fertig(*t)) else {
            // Leer, oder nur Listen, von denen noch offen ist, was drin ist.
            return Typ::LeereListe;
        };
        if gemeinsam == Typ::Zoi && typen.contains(&Typ::Kommazoi) {
            gemeinsam = Typ::Kommazoi;
        }
        for (i, element) in elemente.iter_mut().enumerate() {
            if !self.anpassen(element, gemeinsam) {
                let meldung = format!(
                    "In ana {} muass ollas vom gleichn Typ sei: do steht {}, davua oba {}.",
                    self.wort(Schluessel::List),
                    self.typ_name(typen[i]),
                    self.typ_name(gemeinsam)
                );
                self.melde(element.pos, meldung);
                return Typ::Fehler;
            }
        }
        self.typen.liste_von(gemeinsam)
    }

    /// `Punkt(1.5, 2.0)`: ein neuer Wert der Struktur, die Felder der Reihe nach.
    fn bau(&mut self, ausdruck: &mut Ausdruck, nummer: u32) -> Typ {
        let AusdruckArt::Aufruf { name, argumente } = &mut ausdruck.art else { unreachable!() };
        let name = name.clone();
        let mut argumente = std::mem::take(argumente);
        let typen: Vec<Typ> = argumente.iter_mut().map(|a| self.ausdruck(a)).collect();
        let felder = self.typen.strukturen[nummer as usize].felder.clone();
        if felder.len() != argumente.len() {
            let hat = match felder.len() {
                0 => "kane Föda".to_string(),
                1 => "a Föd".to_string(),
                n => format!("{n} Föda"),
            };
            let meldung = format!("'{name}' hot {hat}, du gibst eam {}.", anzahl(argumente.len()));
            self.melde(ausdruck.pos, meldung);
        } else {
            for (i, (argument, (feld, ziel))) in argumente.iter_mut().zip(&felder).enumerate() {
                if !self.anpassen(argument, *ziel) {
                    let meldung = format!(
                        "'{name}' wü fia des Föd '{feld}' {}, du gibst eam {}.",
                        self.typ_name(*ziel),
                        self.typ_name(typen[i])
                    );
                    self.melde(argument.pos, meldung);
                }
            }
        }
        ausdruck.art = AusdruckArt::Bau { struktur: nummer, argumente };
        ausdruck.typ = Typ::Struktur(nummer);
        ausdruck.typ
    }

    fn binaer(
        &mut self,
        op: BinaerOp,
        links: &mut Ausdruck,
        rechts: &mut Ausdruck,
        l: Typ,
        r: Typ,
        pos: Position,
    ) -> Typ {
        if l == Typ::Fehler || r == Typ::Fehler {
            return Typ::Fehler;
        }
        if let Some(typ) = binaer_ergebnis(op, l, r) {
            // Zwei Zahlen: trifft ZOI auf KOMMAZOI, wird mit KOMMAZOI gerechnet.
            let zahl = |t: Typ| t == Typ::Zoi || t == Typ::Kommazoi;
            if zahl(l) && zahl(r) && (l == Typ::Kommazoi || r == Typ::Kommazoi) {
                self.anpassen(links, Typ::Kommazoi);
                self.anpassen(rechts, Typ::Kommazoi);
            }
            return typ;
        }

        let zeichen = match op {
            BinaerOp::Und => self.wort(Schluessel::And).to_string(),
            BinaerOp::Oda => self.wort(Schluessel::Or).to_string(),
            _ => op.zeichen().to_string(),
        };
        let mut meldung = format!(
            "{} {zeichen} {}? Des geht si ned aus.",
            self.typ_name(l).replacen("a ", "A ", 1),
            self.typ_name(r)
        );
        let text_und_wert = (l == Typ::Kettn) != (r == Typ::Kettn) && l != Typ::Nix && r != Typ::Nix;
        if op == BinaerOp::Plus && text_und_wert {
            meldung.push_str(&format!(" Nimm {}(...) zum Umwandln.", self.wort(Schluessel::Text)));
        }
        self.melde(pos, meldung);
        Typ::Fehler
    }

    /// Prüft den Aufruf einer eingebauten Funktion und sucht die passende Fassung.
    fn eingebaut(&mut self, funktion: Eingebaut, argumente: &mut [Ausdruck], pos: Position) -> Typ {
        let typen: Vec<Typ> = argumente.iter_mut().map(|a| self.ausdruck(a)).collect();
        if typen.contains(&Typ::Fehler) {
            return Typ::Fehler;
        }
        let name = self.wort(wort_von(funktion)).to_string();
        // Die Funktionen für Listen: Ihr Typ hängt davon ab, was in der Liste ist.
        match funktion {
            Eingebaut::Dazu | Eingebaut::Raus => return self.listen_funktion(funktion, &name, argumente, &typen, pos),
            Eingebaut::Laenge if typen.len() == 1 && matches!(typen[0], Typ::Liste(_) | Typ::LeereListe) => {
                self.fertig_verlangt(&argumente[0]);
                return Typ::Zoi;
            }
            _ => {}
        }
        let alle = fassungen(funktion);
        let passt = |von: Typ, ziel: Typ| von == ziel || (von == Typ::Zoi && ziel == Typ::Kommazoi);

        let mit_anzahl: Vec<&(&[Typ], Typ)> = alle.iter().filter(|(p, _)| p.len() == typen.len()).collect();
        if mit_anzahl.is_empty() {
            // "kan oda an Wert", "2 oda 3 Werte"
            let mut anzahlen: Vec<usize> = alle.iter().map(|(p, _)| p.len()).collect();
            anzahlen.dedup();
            let woerter: Vec<String> = anzahlen
                .iter()
                .map(|n| match n {
                    0 => "kan".to_string(),
                    1 => "an".to_string(),
                    n => n.to_string(),
                })
                .collect();
            let einheit = if anzahlen.last() == Some(&1) { "Wert" } else { "Werte" };
            let meldung = format!(
                "'{name}' wü {} {einheit} hom, du gibst eam {}.",
                woerter.join(" oda "),
                anzahl(typen.len())
            );
            self.melde(pos, meldung);
            return alle[0].1;
        }
        let gefunden =
            mit_anzahl.iter().find(|(parameter, _)| parameter.iter().zip(&typen).all(|(ziel, von)| passt(*von, *ziel)));
        if let Some((parameter, ergebnis)) = gefunden {
            for (argument, ziel) in argumente.iter_mut().zip(parameter.iter()) {
                self.anpassen(argument, *ziel);
            }
            return *ergebnis;
        }
        // Keine Fassung passt: den ersten falschen Wert melden.
        let (parameter, ergebnis) = mit_anzahl[0];
        for (i, (ziel, von)) in parameter.iter().zip(&typen).enumerate() {
            if !passt(*von, *ziel) {
                let meldung = format!(
                    "'{name}' wü ois {}. Wert {}, du gibst eam {}.",
                    i + 1,
                    self.typ_name(*ziel),
                    self.typ_name(*von)
                );
                self.melde(argumente[i].pos, meldung);
                break;
            }
        }
        *ergebnis
    }

    /// `HAU EINE(liste, wert)` und `HAU AUSSE(liste, stelle)`.
    fn listen_funktion(
        &mut self,
        funktion: Eingebaut,
        name: &str,
        argumente: &mut [Ausdruck],
        typen: &[Typ],
        pos: Position,
    ) -> Typ {
        if argumente.len() != 2 {
            let meldung = format!("'{name}' wü 2 Werte hom, du gibst eam {}.", anzahl(argumente.len()));
            self.melde(pos, meldung);
            return Typ::Nix;
        }
        if !self.fertig_verlangt(&argumente[0]) {
            return Typ::Nix;
        }
        let Some(element) = self.typen.element(typen[0]) else {
            let meldung = format!(
                "'{name}' wü ois 1. Wert a {}, du gibst eam {}.",
                self.wort(Schluessel::List),
                self.typ_name(typen[0])
            );
            self.melde(argumente[0].pos, meldung);
            return Typ::Nix;
        };
        if funktion == Eingebaut::Dazu {
            if !self.anpassen(&mut argumente[1], element) {
                let meldung = format!(
                    "In de {} ghört {} eine, du gibst oba {} her.",
                    self.wort(Schluessel::List),
                    self.typ_name(element),
                    self.typ_name(typen[1])
                );
                self.melde(argumente[1].pos, meldung);
            }
        } else if typen[1] != Typ::Zoi {
            let meldung = format!(
                "De Stö in ana {} muass {} sei, des do is oba {}.",
                self.wort(Schluessel::List),
                self.typ_name(Typ::Zoi),
                self.typ_name(typen[1])
            );
            self.melde(argumente[1].pos, meldung);
        }
        Typ::Nix
    }

    fn aufruf(&mut self, name: &str, argumente: &mut [Ausdruck], typen: &[Typ], pos: Position) -> Typ {
        let Some(signatur) = self.signaturen.get(name) else {
            let meldung = match self.wb.festes_wort(name) {
                Some(schluessel) => format!("Des haaßt {}, ned {}!", self.wort(schluessel), name),
                None => format!("De {} '{name}' kenn i ned.", self.wort(Schluessel::Function)),
            };
            self.melde(pos, meldung);
            return Typ::Fehler;
        };
        let parameter = signatur.parameter.clone();
        let rueckgabe = signatur.rueckgabe;
        if parameter.len() != argumente.len() {
            let meldung = format!(
                "'{name}' wü {} hom, du gibst eam {}.",
                anzahl(parameter.len()),
                anzahl(argumente.len())
            );
            self.melde(pos, meldung);
            return rueckgabe;
        }
        for (i, (argument, ziel)) in argumente.iter_mut().zip(parameter).enumerate() {
            if !self.anpassen(argument, ziel) {
                let meldung = format!(
                    "'{name}' wü ois {}. Wert {}, du gibst eam {}.",
                    i + 1,
                    self.typ_name(ziel),
                    self.typ_name(typen[i])
                );
                self.melde(argument.pos, meldung);
            }
        }
        rueckgabe
    }
}

fn anzahl(n: usize) -> String {
    match n {
        0 => "kan Wert".to_string(),
        1 => "an Wert".to_string(),
        n => format!("{n} Werte"),
    }
}

/// Kann man dem Ausdruck etwas zuweisen? Das geht bei einer Variable, bei
/// einem Feld von so etwas und bei jeder Stelle in einer Liste.
fn ist_ziel(ausdruck: &Ausdruck) -> bool {
    match &ausdruck.art {
        AusdruckArt::Variable { .. } | AusdruckArt::Stelle { .. } => true,
        AusdruckArt::Feld { objekt, .. } => ist_ziel(objekt),
        _ => false,
    }
}

/// Wie das Ziel einer Zuweisung in einer Meldung heißt: `x`, `p.x`, `liste[...]`.
fn ziel_text(ausdruck: &Ausdruck) -> String {
    match &ausdruck.art {
        AusdruckArt::Variable { name, .. } => name.clone(),
        AusdruckArt::Feld { objekt, name, .. } => format!("{}.{name}", ziel_text(objekt)),
        AusdruckArt::Stelle { liste, .. } => format!("{}[...]", ziel_text(liste)),
        _ => "des".to_string(),
    }
}

/// Packt einen Ausdruck in eine Umwandlung ein.
fn einpacken(ausdruck: &mut Ausdruck, ziel: Typ) {
    let pos = ausdruck.pos;
    let alt = std::mem::replace(ausdruck, Ausdruck::neu(AusdruckArt::Zoi(0), pos));
    *ausdruck = Ausdruck { art: AusdruckArt::Umwandlung { ziel, innen: Box::new(alt) }, pos, typ: ziel };
}

/// Hört dieser Block auf jedem Weg mit GIB ZRUCK auf?
fn gibt_sicher_zruck(block: &[Anweisung]) -> bool {
    block.iter().any(|a| match &a.art {
        AnweisungArt::GibZruck { .. } => true,
        AnweisungArt::Wenn { dann, sunst, .. } => gibt_sicher_zruck(dann) && gibt_sicher_zruck(sunst),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::zerlegen;
    use crate::parser::parsen;

    fn pruefe(quelltext: &str) -> Result<Programm, Vec<Fehler>> {
        let wb = Woerterbuch::eingebaut();
        let mut programm = parsen(zerlegen(quelltext, &wb).unwrap(), &wb).unwrap();
        pruefen(&mut programm, &wb).map(|_| programm)
    }

    fn ok(quelltext: &str) -> Programm {
        pruefe(quelltext).unwrap_or_else(|f| panic!("unerwartete Fehler: {f:?}"))
    }

    /// Genau ein Fehler wird erwartet.
    fn fehler(quelltext: &str) -> Fehler {
        let mut fehler = pruefe(quelltext).expect_err("hier sollte ein Fehler kommen");
        assert_eq!(fehler.len(), 1, "{fehler:?}");
        fehler.remove(0)
    }

    fn typ_von(quelltext: &str) -> Typ {
        let programm = ok(&format!("SOG AU ergebnis = {quelltext} OIDA"));
        match &programm.haupt.last().unwrap().art {
            AnweisungArt::Deklaration { wert, .. } => wert.typ,
            _ => unreachable!(),
        }
    }

    #[test]
    fn typen_werden_erkannt() {
        assert_eq!(typ_von("17"), Typ::Zoi);
        assert_eq!(typ_von("1.5"), Typ::Kommazoi);
        assert_eq!(typ_von("WOAHR"), Typ::Jona);
        assert_eq!(typ_von("\"Servas\""), Typ::Kettn);
        assert_eq!(typ_von("1 + 2 * 3"), Typ::Zoi);
        assert_eq!(typ_von("1 + 2.5"), Typ::Kommazoi);
        assert_eq!(typ_von("7 / 2"), Typ::Zoi);
        assert_eq!(typ_von("1 < 2 UND NED FOISCH"), Typ::Jona);
        assert_eq!(typ_von("\"a\" + \"b\""), Typ::Kettn);
        assert_eq!(typ_von("\"a\" < \"b\""), Typ::Jona);
        assert_eq!(typ_von("KETTN(3) + \"x\""), Typ::Kettn);
        assert_eq!(typ_von("ZOI(\"3\") + 1"), Typ::Zoi);
    }

    #[test]
    fn zoi_wird_von_selbst_zur_kommazoi() {
        let programm = ok("SOG AU x = 1 + 2.5 OIDA");
        let AnweisungArt::Deklaration { wert, .. } = &programm.haupt[0].art else { panic!() };
        let AusdruckArt::Binaer { links, rechts, .. } = &wert.art else { panic!() };
        assert!(matches!(links.art, AusdruckArt::Umwandlung { ziel: Typ::Kommazoi, .. }));
        assert!(matches!(rechts.art, AusdruckArt::Kommazoi(_)));
    }

    #[test]
    fn typ_angabe_bei_der_deklaration() {
        ok("SOG AU x KOMMAZOI = 3 OIDA x = x + 1 OIDA");
        let f = fehler("SOG AU x ZOI = 3.5 OIDA");
        assert_eq!(f.meldung, "'x' soll a ZOI sei, oba des do is a KOMMAZOI.");
    }

    #[test]
    fn variablen_bekommen_nummern() {
        let programm = ok("SOG AU a = 1 OIDA SOG AU b = a OIDA b = a + b OIDA");
        let AnweisungArt::Deklaration { id: a, .. } = programm.haupt[0].art else { panic!() };
        let AnweisungArt::Deklaration { id: b, .. } = programm.haupt[1].art else { panic!() };
        let AnweisungArt::Zuweisung { ziel, .. } = &programm.haupt[2].art else { panic!() };
        let AusdruckArt::Variable { id: ziel, .. } = ziel.art else { panic!() };
        assert_ne!(a, b);
        assert_eq!(ziel, b);
    }

    #[test]
    fn falscher_typ_bei_zuweisung() {
        let f = fehler("SOG AU alter = 17 OIDA\nalter = \"siebzehn\" OIDA");
        assert_eq!(f.meldung, "'alter' is a ZOI, do kaunst ka KETTN einetuan.");
        assert_eq!(f.pos, Position::neu(2, 9));
    }

    #[test]
    fn text_plus_zahl() {
        let f = fehler("SOG AU x = \"Alter: \" + 17 OIDA");
        assert_eq!(f.meldung, "A KETTN + a ZOI? Des geht si ned aus. Nimm KETTN(...) zum Umwandln.");
    }

    #[test]
    fn unbekannte_variable() {
        let f = fehler("SCHREIB(wos) OIDA");
        assert_eq!(f.meldung, "'wos' kenn i ned. Host des mit SOG AU augsogt?");
        assert_eq!(f.pos, Position::neu(1, 9));
    }

    #[test]
    fn doppelte_variable() {
        let f = fehler("SOG AU x = 1 OIDA\nSOG AU x = 2 OIDA");
        assert!(f.meldung.contains("gibt's do scho (Zeile 1)"), "{}", f.meldung);
    }

    #[test]
    fn variable_gilt_nur_im_block() {
        ok("WENN WOAHR DAUN SOG AU x = 1 OIDA PASST SCHO\nWENN WOAHR DAUN SOG AU x = 2 OIDA PASST SCHO");
        let f = fehler("WENN WOAHR DAUN SOG AU x = 1 OIDA PASST SCHO\nSCHREIB(x) OIDA");
        assert!(f.meldung.contains("'x' kenn i ned"), "{}", f.meldung);
    }

    #[test]
    fn bedingung_muss_jona_sein() {
        let f = fehler("WENN 1 DAUN PASST SCHO");
        assert_eq!(f.meldung, "Noch WENN brauch i a JONA (WOAHR oda FOISCH), des do is oba a ZOI.");
        let f = fehler("SOLONG \"jo\" TUA PASST SCHO");
        assert!(f.meldung.starts_with("Noch SOLONG brauch i a JONA"), "{}", f.meldung);
    }

    #[test]
    fn funktionsaufrufe() {
        let quelle = "FUNKTIAUN hoch2(x KOMMAZOI) GIBT ZRUCK KOMMAZOI\n GIB ZRUCK x * x OIDA\nPASST SCHO\n";
        assert_eq!(typ_von(&format!("hoch2(3)\nOIDA\n{quelle}SOG AU y = hoch2(1.5)")), Typ::Kommazoi);
        let f = fehler(&format!("{quelle}SCHREIB(hoch2(\"drei\")) OIDA"));
        assert_eq!(f.meldung, "'hoch2' wü ois 1. Wert a KOMMAZOI, du gibst eam a KETTN.");
        let f = fehler(&format!("{quelle}SCHREIB(hoch2(1, 2)) OIDA"));
        assert_eq!(f.meldung, "'hoch2' wü an Wert hom, du gibst eam 2 Werte.");
        let f = fehler("SCHREIB(gibtsned(1)) OIDA");
        assert_eq!(f.meldung, "De FUNKTIAUN 'gibtsned' kenn i ned.");
    }

    #[test]
    fn funktion_darf_vor_der_definition_verwendet_werden() {
        ok("SCHREIB(eins()) OIDA\nFUNKTIAUN eins() GIBT ZRUCK ZOI\n GIB ZRUCK 1 OIDA\nPASST SCHO");
    }

    #[test]
    fn rueckgabe_wird_geprueft() {
        let f = fehler("FUNKTIAUN f() GIBT ZRUCK ZOI\n GIB ZRUCK \"na\" OIDA\nPASST SCHO");
        assert_eq!(f.meldung, "De FUNKTIAUN 'f' gibt a ZOI zruck, ned a KETTN.");
        let f = fehler("FUNKTIAUN f(x ZOI) GIBT ZRUCK ZOI\n WENN x > 0 DAUN GIB ZRUCK 1 OIDA PASST SCHO\nPASST SCHO");
        assert!(f.meldung.contains("ned jeda Weg hört mit GIB ZRUCK auf"), "{}", f.meldung);
        ok("FUNKTIAUN f(x ZOI) GIBT ZRUCK ZOI\n WENN x > 0 DAUN GIB ZRUCK 1 OIDA SUNST GIB ZRUCK 2 OIDA PASST SCHO\nPASST SCHO");
        let f = fehler("GIB ZRUCK 1 OIDA");
        assert_eq!(f.meldung, "GIB ZRUCK geht nur in ana FUNKTIAUN.");
        let f = fehler("FUNKTIAUN f()\n GIB ZRUCK 1 OIDA\nPASST SCHO");
        assert!(f.meldung.contains("gibt nix zruck. Schreib GIBT ZRUCK ZOI dazua"), "{}", f.meldung);
    }

    #[test]
    fn funktion_sieht_keine_variablen_vom_hauptprogramm() {
        let f = fehler("SOG AU x = 1 OIDA\nFUNKTIAUN f() GIBT ZRUCK ZOI\n GIB ZRUCK x OIDA\nPASST SCHO");
        assert!(f.meldung.contains("'x' kenn i ned"), "{}", f.meldung);
    }

    #[test]
    fn mehrere_fehler_auf_einmal() {
        let fehler = pruefe("SOG AU a = 1 + WOAHR OIDA\nSCHREIB(b) OIDA\nc = 3 OIDA").unwrap_err();
        assert_eq!(fehler.len(), 3, "{fehler:?}");
        assert_eq!(fehler.iter().map(|f| f.pos.zeile).collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn zaehlschleife() {
        let programm = ok("FIA i VON 1 BIS 3 TUA SCHREIB(i) OIDA PASST SCHO");
        let AnweisungArt::Fia { id, rumpf, .. } = &programm.haupt[0].art else { panic!() };
        let AnweisungArt::Schreib { argumente } = &rumpf[0].art else { panic!() };
        assert!(matches!(argumente[0].art, AusdruckArt::Variable { id: innen, .. } if innen == *id));
        assert_eq!(argumente[0].typ, Typ::Zoi);

        // Die Zählvariable gilt nur in der Schleife.
        let f = fehler("FIA i VON 1 BIS 3 TUA PASST SCHO\nSCHREIB(i) OIDA");
        assert!(f.meldung.contains("'i' kenn i ned"), "{}", f.meldung);
        let f = fehler("FIA i VON 1.5 BIS 3 TUA PASST SCHO");
        assert_eq!(f.meldung, "Noch VON brauch i a ZOI, des do is oba a KOMMAZOI.");
        let f = fehler("FIA i VON 1 BIS 3 SCHRITT 0 TUA PASST SCHO");
        assert_eq!(f.meldung, "Mit SCHRITT 0 kummt de Schleifn nie weida.");
    }

    #[test]
    fn hau_ob_nur_in_schleifen() {
        ok("SOLONG WOAHR TUA WENN WOAHR DAUN HAU OB OIDA PASST SCHO WEIDA OIDA PASST SCHO");
        ok("FIA i VON 1 BIS 3 TUA HAU OB OIDA PASST SCHO");
        assert_eq!(fehler("HAU OB OIDA").meldung, "HAU OB geht nur in ana Schleifn.");
        assert_eq!(fehler("WENN WOAHR DAUN WEIDA OIDA PASST SCHO").meldung, "WEIDA geht nur in ana Schleifn.");
        // Eine Schleife außen herum gilt nicht für die Funktion darin.
        let f = fehler("FUNKTIAUN f()\n HAU OB OIDA\nPASST SCHO\nSOLONG WOAHR TUA f() OIDA PASST SCHO");
        assert_eq!(f.meldung, "HAU OB geht nur in ana Schleifn.");
    }

    #[test]
    fn eingebaute_funktionen() {
        assert_eq!(typ_von("LÄNGN(\"abc\")"), Typ::Zoi);
        assert_eq!(typ_von("STICKL(\"abc\", 1)"), Typ::Kettn);
        assert_eq!(typ_von("STICKL(\"abc\", 1, 2)"), Typ::Kettn);
        assert_eq!(typ_von("SUACH(\"abc\", \"c\")"), Typ::Zoi);
        assert_eq!(typ_von("SCHREI(FLÜSTER(STUTZ(\" a \")))"), Typ::Kettn);
        assert_eq!(typ_von("ERSETZ(\"a\", \"a\", \"b\")"), Typ::Kettn);
        assert_eq!(typ_von("WURZL(2)"), Typ::Kommazoi);
        assert_eq!(typ_von("HOCH(2, 10)"), Typ::Kommazoi);
        assert_eq!(typ_von("BETROG(-3)"), Typ::Zoi);
        assert_eq!(typ_von("BETROG(-3.5)"), Typ::Kommazoi);
        assert_eq!(typ_von("KLANSTA(1, 2)"), Typ::Zoi);
        assert_eq!(typ_von("GRÖSSTA(1, 2.5)"), Typ::Kommazoi);
        assert_eq!(typ_von("SINUS(PI) + KOSINUS(0) + WINKL(1, 1) + E_HOCH(1)"), Typ::Kommazoi);
        assert_eq!(typ_von("OBRUNDN(2.5) + AUFRUNDN(2.5) + RUNDN(2.5)"), Typ::Kommazoi);
        assert_eq!(typ_von("ZUFOI()"), Typ::Kommazoi);
        assert_eq!(typ_von("ZUFOI(1, 6)"), Typ::Zoi);
        assert_eq!(typ_von("LIES()"), Typ::Kettn);
        assert_eq!(typ_von("LIES(\"Nom: \")"), Typ::Kettn);
        ok("SOMEN(42) OIDA\nLIES() OIDA");
    }

    #[test]
    fn zoi_wird_in_eingebauten_funktionen_zur_kommazoi() {
        let programm = ok("SOG AU x = WURZL(16) OIDA");
        let AnweisungArt::Deklaration { wert, .. } = &programm.haupt[0].art else { panic!() };
        let AusdruckArt::Eingebaut { argumente, .. } = &wert.art else { panic!() };
        assert!(matches!(argumente[0].art, AusdruckArt::Umwandlung { ziel: Typ::Kommazoi, .. }));
    }

    #[test]
    fn eingebaute_funktionen_falsch_aufgerufen() {
        assert_eq!(fehler("SOG AU x = LÄNGN(5) OIDA").meldung, "'LÄNGN' wü ois 1. Wert a KETTN, du gibst eam a ZOI.");
        assert_eq!(fehler("SOG AU x = LÄNGN() OIDA").meldung, "'LÄNGN' wü an Wert hom, du gibst eam kan Wert.");
        assert_eq!(
            fehler("SOG AU x = STICKL(\"a\") OIDA").meldung,
            "'STICKL' wü 2 oda 3 Werte hom, du gibst eam an Wert."
        );
        assert_eq!(
            fehler("SOG AU x = ZUFOI(1) OIDA").meldung,
            "'ZUFOI' wü kan oda 2 Werte hom, du gibst eam an Wert."
        );
        assert_eq!(
            fehler("SOG AU x = WURZL(\"vier\") OIDA").meldung,
            "'WURZL' wü ois 1. Wert a KOMMAZOI, du gibst eam a KETTN."
        );
        assert_eq!(
            fehler("SOG AU x = SOMEN(1) OIDA").meldung,
            "Des gibt nix zruck, des kaunst in ka Variable einetuan."
        );
    }

    #[test]
    fn rechnen_und_zuweisen() {
        ok("SOG AU x = 1 OIDA x += 2 OIDA x *= 3 OIDA SOG AU s = \"a\" OIDA s += \"b\" OIDA");
        let f = fehler("SOG AU x = 1 OIDA\nx += 0.5 OIDA");
        assert_eq!(f.meldung, "'x' is a ZOI, do kaunst ka KOMMAZOI einetuan.");
    }

    #[test]
    fn sinnlose_anweisung() {
        let f = fehler("1 + 2 OIDA");
        assert!(f.meldung.contains("schmeißt's glei wieda weg"), "{}", f.meldung);
    }

    #[test]
    fn umwandlung_die_es_nicht_gibt() {
        let f = fehler("SOG AU x = JONA(3) OIDA");
        assert_eq!(f.meldung, "Aus ana ZOI kaun i ka JONA mochn.");
    }

    #[test]
    fn englisches_wort_als_funktion() {
        let f = fehler("PRINT(1) OIDA");
        assert_eq!(f.meldung, "Des haaßt SCHREIB, ned PRINT!");
    }

    // ------------------------------------------------ Strukturen und Listen

    const PUNKT: &str = "STRUKTUR Punkt\n x KOMMAZOI\n y KOMMAZOI\nPASST SCHO\n";

    #[test]
    fn struktur_bauen_und_felder_lesen() {
        let programm = ok(&format!("{PUNKT}SOG AU p = Punkt(1, 2.5) OIDA\nSOG AU summe = p.x + p.y OIDA"));
        let AnweisungArt::Deklaration { wert, .. } = &programm.haupt[0].art else { panic!() };
        assert_eq!(wert.typ, Typ::Struktur(0));
        // Aus dem Aufruf ist ein Bau geworden, und die 1 eine KOMMAZOI.
        let AusdruckArt::Bau { struktur: 0, argumente } = &wert.art else { panic!("{:?}", wert.art) };
        assert!(matches!(argumente[0].art, AusdruckArt::Umwandlung { ziel: Typ::Kommazoi, .. }));
        let AnweisungArt::Deklaration { wert, .. } = &programm.haupt[1].art else { panic!() };
        assert_eq!(wert.typ, Typ::Kommazoi);
        let AusdruckArt::Binaer { rechts, .. } = &wert.art else { panic!() };
        assert!(matches!(rechts.art, AusdruckArt::Feld { nummer: 1, .. }));
        assert_eq!(programm.typen.strukturen[0].name, "Punkt");
    }

    #[test]
    fn struktur_darf_weiter_unten_stehen_und_andere_enthalten() {
        ok("SOG AU g = Gegner(Punkt(0, 0), \"Hansi\") OIDA\nSCHREIB(g.pos.x, g.name) OIDA\n\
            STRUKTUR Gegner\n pos Punkt\n name KETTN\nPASST SCHO\nSTRUKTUR Punkt\n x KOMMAZOI\n y KOMMAZOI\nPASST SCHO");
    }

    #[test]
    fn struktur_falsch_gebaut() {
        let f = fehler(&format!("{PUNKT}SOG AU p = Punkt(1) OIDA"));
        assert_eq!(f.meldung, "'Punkt' hot 2 Föda, du gibst eam an Wert.");
        let f = fehler(&format!("{PUNKT}SOG AU p = Punkt(1, \"zwa\") OIDA"));
        assert_eq!(f.meldung, "'Punkt' wü fia des Föd 'y' a KOMMAZOI, du gibst eam a KETTN.");
    }

    #[test]
    fn felder_die_es_nicht_gibt() {
        let f = fehler(&format!("{PUNKT}SOG AU p = Punkt(1, 2) OIDA\nSCHREIB(p.z) OIDA"));
        assert_eq!(f.meldung, "A Föd 'z' gibt's in 'Punkt' ned. Es gibt: x, y.");
        assert_eq!(f.pos, Position::neu(6, 11));
        let f = fehler("SOG AU n = 5 OIDA\nSCHREIB(n.x) OIDA");
        assert_eq!(f.meldung, "A ZOI hot kane Föda.");
    }

    #[test]
    fn strukturen_mit_fehlern() {
        let f = fehler("STRUKTUR A\n x ZOI\n x ZOI\nPASST SCHO");
        assert_eq!(f.meldung, "Des Föd 'x' gibt's in 'A' scho.");
        let f = fehler("STRUKTUR A\nPASST SCHO\nSTRUKTUR A\nPASST SCHO");
        assert_eq!(f.meldung, "De STRUKTUR 'A' gibt's scho. Zwamoi geht ned.");
        let f = fehler("STRUKTUR A\n b Gibtsned\nPASST SCHO");
        assert_eq!(f.meldung, "An Typ 'Gibtsned' kenn i ned. Gibt's a STRUKTUR mit dem Nom?");
        let f = fehler("STRUKTUR A\nPASST SCHO\nFUNKTIAUN A()\nPASST SCHO");
        assert_eq!(f.meldung, "'A' gibt's scho ois STRUKTUR, des kaun ned a no a FUNKTIAUN sei.");
    }

    #[test]
    fn struktur_die_sich_selbst_enthaelt() {
        let f = fehler("STRUKTUR Knoten\n wert ZOI\n naechster Knoten\nPASST SCHO");
        assert!(f.meldung.contains("kaun si ned söwa enthoitn"), "{}", f.meldung);
        assert_eq!(f.pos, Position::neu(3, 2));
        // Im Kreis über eine zweite Struktur: Die erste Stelle wird gemeldet, damit ist der Kreis offen.
        let f = fehler("STRUKTUR A\n b B\nPASST SCHO\nSTRUKTUR B\n a A\nPASST SCHO");
        assert!(f.meldung.contains("'A' kaun si ned söwa enthoitn"), "{}", f.meldung);
        // Über eine Liste geht es: Die Liste liegt woanders.
        ok("STRUKTUR Knoten\n wert ZOI\n kinder LISTN VON Knoten\nPASST SCHO\nSOG AU k = Knoten(1, []) OIDA");
    }

    #[test]
    fn zuweisung_an_felder() {
        ok(&format!("{PUNKT}SOG AU p = Punkt(1, 2) OIDA\np.x = 5 OIDA\np.y += 0.5 OIDA\np = Punkt(0, 0) OIDA"));
        let f = fehler(&format!("{PUNKT}SOG AU p = Punkt(1, 2) OIDA\np.x = \"links\" OIDA"));
        assert_eq!(f.meldung, "'p.x' is a KOMMAZOI, do kaunst ka KETTN einetuan.");
        let f = fehler(&format!("{PUNKT}Punkt(1, 2).x = 5 OIDA"));
        assert!(f.meldung.starts_with("Do kaun ma nix zuaweisn."), "{}", f.meldung);
        let f = fehler("SOG AU s = \"a\" OIDA\ns -= \"b\" OIDA");
        assert_eq!(f.meldung, "A KETTN -= a KETTN? Des geht si ned aus.");
    }

    #[test]
    fn listen_werden_erkannt() {
        let programm = ok("SOG AU a = [1, 2, 3] OIDA\nSOG AU b = [1, 2.5] OIDA\nSOG AU c = [[\"x\"], []] OIDA\n\
                           SOG AU d = a[0] + LÄNGN(a) OIDA\nSOG AU e = c[0][0] OIDA");
        let typ = |n: usize| match &programm.haupt[n].art {
            AnweisungArt::Deklaration { wert, .. } => wert.typ,
            _ => unreachable!(),
        };
        let t = &programm.typen;
        assert_eq!(t.element(typ(0)), Some(Typ::Zoi));
        assert_eq!(t.element(typ(1)), Some(Typ::Kommazoi));
        assert_eq!(t.element(t.element(typ(2)).unwrap()), Some(Typ::Kettn));
        assert_eq!(typ(3), Typ::Zoi);
        assert_eq!(typ(4), Typ::Kettn);
    }

    #[test]
    fn leere_liste_braucht_einen_typ() {
        ok("SOG AU a LISTN VON ZOI = [] OIDA\nHAU EINE(a, 5) OIDA\na = [] OIDA");
        ok("FUNKTIAUN f(xs LISTN VON KOMMAZOI) GIBT ZRUCK LISTN VON KETTN\n GIB ZRUCK [] OIDA\nPASST SCHO\nSOG AU r = f([]) OIDA\nSOG AU s = f([1, 2]) OIDA");
        let f = fehler("SOG AU a = [] OIDA");
        assert!(f.meldung.starts_with("Bei [] waaß i ned, wos fia a LISTN des sei soll."), "{}", f.meldung);
        let f = fehler("SCHREIB(LÄNGN([])) OIDA");
        assert!(f.meldung.starts_with("Bei [] waaß i ned"), "{}", f.meldung);
    }

    #[test]
    fn listen_mit_fehlern() {
        let f = fehler("SOG AU a = [1, \"zwa\"] OIDA");
        assert_eq!(f.meldung, "In ana LISTN muass ollas vom gleichn Typ sei: do steht a KETTN, davua oba a ZOI.");
        let f = fehler("SOG AU a = [1, 2] OIDA\nSOG AU b = a[\"null\"] OIDA");
        assert_eq!(f.meldung, "De Stö in ana LISTN muass a ZOI sei, des do is oba a KETTN.");
        let f = fehler("SOG AU s = \"abc\" OIDA\nSOG AU b = s[0] OIDA");
        assert_eq!(f.meldung, "Mit [...] kummt ma nur in a LISTN eine, des do is oba a KETTN. Fia Texte gibt's STICKL.");
        let f = fehler("SOG AU a = [1, 2] OIDA\na[0] = \"x\" OIDA");
        assert_eq!(f.meldung, "'a[...]' is a ZOI, do kaunst ka KETTN einetuan.");
        let f = fehler("SOG AU a LISTN VON ZOI = [1.5] OIDA");
        assert_eq!(f.meldung, "'a' soll a LISTN VON ZOI sei, oba des do is a LISTN VON KOMMAZOI.");
    }

    #[test]
    fn listen_funktionen() {
        ok("SOG AU a = [1.5] OIDA\nHAU EINE(a, 2) OIDA\nDAZUA(a, 3.5) OIDA\nHAU AUSSE(a, 0) OIDA");
        let f = fehler("SOG AU a = [1] OIDA\nHAU EINE(a, \"zwa\") OIDA");
        assert_eq!(f.meldung, "In de LISTN ghört a ZOI eine, du gibst oba a KETTN her.");
        let f = fehler("HAU EINE(5, 1) OIDA");
        assert_eq!(f.meldung, "'HAU EINE' wü ois 1. Wert a LISTN, du gibst eam a ZOI.");
        let f = fehler("SOG AU a = [1] OIDA\nHAU AUSSE(a, 0.5) OIDA");
        assert_eq!(f.meldung, "De Stö in ana LISTN muass a ZOI sei, des do is oba a KOMMAZOI.");
        let f = fehler("SOG AU a = [1] OIDA\nHAU AUSSE(a) OIDA");
        assert_eq!(f.meldung, "'HAU AUSSE' wü 2 Werte hom, du gibst eam an Wert.");
    }

    #[test]
    fn fia_jede() {
        let programm = ok(&format!(
            "{PUNKT}SOG AU ps = [Punkt(1, 2)] OIDA\nFIA JEDE p IN ps TUA\n SCHREIB(p.x) OIDA\nPASST SCHO"
        ));
        let AnweisungArt::FiaJede { id, rumpf, .. } = &programm.haupt[1].art else { panic!() };
        let AnweisungArt::Schreib { argumente } = &rumpf[0].art else { panic!() };
        let AusdruckArt::Feld { objekt, .. } = &argumente[0].art else { panic!() };
        assert!(matches!(objekt.art, AusdruckArt::Variable { id: innen, .. } if innen == *id));
        assert_eq!(objekt.typ, Typ::Struktur(0));

        let f = fehler("FIA JEDE x IN 5 TUA PASST SCHO");
        assert_eq!(f.meldung, "FIA JEDE geht nur mit ana LISTN, des do is oba a ZOI.");
        let f = fehler("FIA JEDE x IN [1] TUA PASST SCHO\nSCHREIB(x) OIDA");
        assert!(f.meldung.contains("'x' kenn i ned"), "{}", f.meldung);
        ok("FIA JEDE x IN [1, 2] TUA WENN x = 1 DAUN WEIDA OIDA PASST SCHO HAU OB OIDA PASST SCHO");
    }

    #[test]
    fn strukturen_und_listen_kann_man_nicht_vergleichen() {
        let f = fehler(&format!("{PUNKT}SOG AU a = Punkt(1, 2) = Punkt(1, 2) OIDA"));
        assert_eq!(f.meldung, "A Punkt == a Punkt? Des geht si ned aus.");
        let f = fehler("SOG AU a = [1] = [1] OIDA");
        assert_eq!(f.meldung, "A LISTN VON ZOI == a LISTN VON ZOI? Des geht si ned aus.");
    }
}
