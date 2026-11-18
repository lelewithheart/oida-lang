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
//!
//! # Vorlagen (Generics)
//!
//! `FUNKTIAUN erster VON T(...)`, `KLASSE Stapel VON T` und `STRUKTUR Paar
//! VON A UND B` sind Vorlagen: Für sie selbst entsteht kein Code. Erst wenn
//! feststeht, was `T` ist (`Stapel VON ZOI`, ein Aufruf mit einer Liste von
//! Texten ...), wird die Vorlage kopiert und mit diesem Typ geprüft wie eine
//! gewöhnliche Funktion oder Klasse ([`Pruefer::typ_umgebung`] sagt dabei,
//! wofür `T` steht). Jede Fassung gibt es nur einmal. Ob der Rumpf mit einem
//! Typ funktioniert (etwa `a < b`), zeigt sich also erst beim Verwenden.
//!
//! # Was NIX sein kann
//!
//! Ein Wert vom Typ `VIELLEICHT T` kann fehlen. Verwenden kann man ihn erst,
//! wenn feststeht, dass er da ist. Bei einer Variable verfolgt die
//! Typprüfung das mit ([`Pruefer::sicher`]): Nach `WENN x != NIX DAUN` gilt
//! `x` im Block als `T`, ebenso nach `WENN x == NIX DAUN GIB ZRUCK ...`,
//! rechts von `x != NIX UND` und nach einer Zuweisung mit einem echten Wert.
//! Eine Zuweisung, die wieder NIX bringen kann, hebt das auf. Bei Feldern
//! und Listenstellen geht das nicht (jemand anderer könnte sie inzwischen
//! ändern); die holt man sich zuerst in eine Variable.

use std::collections::{HashMap, HashSet};

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

/// Was die Typprüfung über eine Funktion ohne Namen weiß, in der sie gerade steckt.
struct LambdaKontext {
    /// So viele Bereiche waren offen, als sie begonnen hat: Alles darunter liegt außerhalb.
    grenze: usize,
    /// Was sie von außen mitnimmt.
    fang: Vec<Fang>,
    /// Die Nummer, unter der sie SÖWA mitgenommen hat.
    selbst: Option<usize>,
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
    /// Name einer Klasse -> ihre Nummer in `typen.klassen`.
    klassen_nummern: HashMap<String, u32>,
    /// Die Klasse, deren Methoden gerade geprüft werden.
    methoden_klasse: Option<u32>,
    /// In einer Methode: die Klasse und die Nummer von SÖWA.
    selbst: Option<(u32, usize)>,
    /// Die `VIELLEICHT`-Variablen (ihre Nummern), von denen an dieser Stelle
    /// feststeht, dass sie nicht NIX sind.
    sicher: HashSet<usize>,
    /// Darf der Ausdruck, der als nächster geprüft wird, mit PROBIER beginnen?
    /// Das geht nur ganz vorne in einer Anweisung: Dort hängt noch kein
    /// Zwischenwert in der Luft, wenn die Funktion mittendrin aufhört.
    probier_erlaubt: bool,
    /// Die Funktionen ohne Namen, in denen wir gerade stecken, die innerste hinten.
    lambdas: Vec<LambdaKontext>,
    naechste_lambda: usize,
    /// Die Vorlagen, wie sie im Programm stehen.
    struktur_vorlagen: HashMap<String, StrukturDef>,
    klassen_vorlagen: HashMap<String, KlassenDef>,
    funktions_vorlagen: HashMap<String, Funktion>,
    /// Wofür die Typnamen einer Vorlage gerade stehen (`T` -> `ZOI`).
    typ_umgebung: HashMap<String, Typ>,
    /// Die Fassungen, die es schon gibt: Vorlage und Typen -> Nummer oder Name.
    struktur_fassungen: HashMap<(String, Vec<Typ>), u32>,
    klassen_fassungen: HashMap<(String, Vec<Typ>), u32>,
    funktions_fassungen: HashMap<(String, Vec<Typ>), String>,
    /// Umgekehrt: aus welcher Vorlage und mit welchen Typen eine Struktur oder Klasse entstanden ist.
    struktur_aus: HashMap<u32, (String, Vec<Typ>)>,
    klasse_aus: HashMap<u32, (String, Vec<Typ>)>,
    /// Fassungen, deren Inhalt noch geprüft werden muss.
    offene_funktionen: Vec<(Funktion, HashMap<String, Typ>)>,
    offene_klassen: Vec<(u32, KlassenDef, HashMap<String, Typ>)>,
    fertige_funktionen: Vec<Funktion>,
    fertige_klassen: Vec<(u32, KlassenDef)>,
    /// Die Fassung, die gerade geprüft wird (für Meldungen).
    fassung: Option<String>,
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
        Eingebaut::ZoiAus => Schluessel::ParseInt,
        Eingebaut::KommazoiAus => Schluessel::ParseFloat,
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
        Eingebaut::Dazu | Eingebaut::Raus | Eingebaut::ZoiAus | Eingebaut::KommazoiAus => {
            unreachable!("werden eigens geprüft, weil ihr Typ nicht in dieser Tabelle stehen kann")
        }
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
        klassen_nummern: HashMap::new(),
        methoden_klasse: None,
        selbst: None,
        sicher: HashSet::new(),
        probier_erlaubt: false,
        lambdas: Vec::new(),
        naechste_lambda: 0,
        struktur_vorlagen: HashMap::new(),
        klassen_vorlagen: HashMap::new(),
        funktions_vorlagen: HashMap::new(),
        typ_umgebung: HashMap::new(),
        struktur_fassungen: HashMap::new(),
        klassen_fassungen: HashMap::new(),
        funktions_fassungen: HashMap::new(),
        struktur_aus: HashMap::new(),
        klasse_aus: HashMap::new(),
        offene_funktionen: Vec::new(),
        offene_klassen: Vec::new(),
        fertige_funktionen: Vec::new(),
        fertige_klassen: Vec::new(),
        fassung: None,
    };

    p.strukturen(&programm.strukturen, &programm.klassen);
    p.klassen(&mut programm.klassen);

    // Alle Funktionen kennenlernen, damit die Reihenfolge egal ist.
    for f in &mut programm.funktionen {
        let vorlage = !f.typ_parameter.is_empty();
        if !vorlage {
            for parameter in &mut f.parameter {
                parameter.typ = p.typ_aus(&parameter.angabe);
            }
            f.rueckgabe = f.rueckgabe_angabe.as_ref().map_or(Typ::Nix, |angabe| p.typ_aus(angabe));
        }
        let schon = if p.struktur_nummern.contains_key(&f.name) || p.struktur_vorlagen.contains_key(&f.name) {
            Some(Schluessel::Struct)
        } else if p.klassen_nummern.contains_key(&f.name) || p.klassen_vorlagen.contains_key(&f.name) {
            Some(Schluessel::Class)
        } else {
            None
        };
        if let Some(schon) = schon {
            let meldung = format!(
                "'{}' gibt's scho ois {}, des kaun ned a no a {} sei.",
                f.name,
                p.wort(schon),
                p.wort(Schluessel::Function)
            );
            p.melde(f.pos, meldung);
            continue;
        }
        if p.signaturen.contains_key(&f.name) || p.funktions_vorlagen.contains_key(&f.name) {
            let meldung = format!("De {} '{}' gibt's scho. Zwamoi geht ned.", p.wort(Schluessel::Function), f.name);
            p.melde(f.pos, meldung);
            // Damit die zweite nicht als gewöhnliche Funktion geprüft wird.
            f.typ_parameter.clear();
            continue;
        }
        if vorlage {
            p.funktions_vorlagen.insert(f.name.clone(), f.clone());
            continue;
        }
        p.signaturen.insert(
            f.name.clone(),
            Signatur { parameter: f.parameter.iter().map(|x| x.typ).collect(), rueckgabe: f.rueckgabe },
        );
    }

    for f in &mut programm.funktionen {
        if !p.funktions_vorlagen.contains_key(&f.name) {
            p.funktion(f);
        }
    }
    for (nummer, k) in programm.klassen.iter_mut().enumerate() {
        // Eine doppelte Klasse ist schon gemeldet; ihr Inhalt wird nicht geprüft.
        if p.klassen_nummern.get(&k.name) == Some(&(nummer as u32)) {
            p.klassen_inhalt(nummer as u32, k);
        }
    }

    p.bereiche = vec![HashMap::new()];
    p.funktion = None;
    p.selbst = None;
    p.schleifen = 0;
    p.sicher.clear();
    for a in &mut programm.haupt {
        p.anweisung(a);
    }

    // Jetzt die Fassungen der Vorlagen, die dabei gebraucht wurden. Beim
    // Prüfen können weitere dazukommen.
    loop {
        if let Some((mut f, umgebung)) = p.offene_funktionen.pop() {
            p.typ_umgebung = umgebung;
            p.fassung = Some(f.name.clone());
            p.methoden_klasse = None;
            p.selbst = None;
            p.funktion(&mut f);
            p.fertige_funktionen.push(f);
        } else if let Some((nummer, mut k, umgebung)) = p.offene_klassen.pop() {
            p.typ_umgebung = umgebung;
            p.fassung = Some(p.typen.klassen[nummer as usize].name.clone());
            p.klassen_inhalt(nummer, &mut k);
            p.fertige_klassen.push((nummer, k));
        } else {
            break;
        }
    }
    p.fassung = None;
    p.typ_umgebung.clear();

    // Die Vorlagen selbst fallen weg, ihre Fassungen kommen dazu. Die Klassen
    // müssen in der Reihenfolge ihrer Nummern stehen.
    programm.funktionen.retain(|f| f.typ_parameter.is_empty());
    programm.funktionen.append(&mut p.fertige_funktionen);
    p.fertige_klassen.sort_by_key(|(nummer, _)| *nummer);
    programm.klassen.extend(std::mem::take(&mut p.fertige_klassen).into_iter().map(|(_, k)| k));

    if p.fehler.is_empty() {
        programm.typen = p.typen;
        Ok(())
    } else {
        p.fehler.sort_by_key(|f| (f.pos.datei, f.pos.zeile, f.pos.spalte));
        // Derselbe Fehler in mehreren Fassungen einer Vorlage wird nur einmal gezeigt.
        p.fehler.dedup_by(|a, b| a.pos == b.pos && a.meldung == b.meldung);
        Err(p.fehler)
    }
}

impl Pruefer<'_> {
    fn melde(&mut self, pos: Position, meldung: impl Into<String>) {
        let mut meldung: String = meldung.into();
        // In einer Vorlage dazusagen, mit welchen Typen es nicht geht.
        if let Some(fassung) = &self.fassung {
            meldung.push_str(&format!(" (bei '{fassung}')"));
        }
        self.fehler.push(Fehler::neu(pos, meldung));
    }

    /// Wie die Fassung einer Vorlage heißt: `Stapel VON ZOI`, `Paar VON ZOI UND KETTN`.
    fn fassungs_name(&self, name: &str, typen: &[Typ]) -> String {
        let typen: Vec<String> = typen.iter().map(|t| self.typ_wort(*t)).collect();
        let und = format!(" {} ", self.wort(Schluessel::And));
        format!("{name} {} {}", self.wort(Schluessel::From), typen.join(&und))
    }

    /// Passen so viele Typen zur Vorlage?
    fn typen_anzahl_passt(&mut self, name: &str, offen: &[String], gegeben: usize, pos: Position) -> bool {
        if offen.len() == gegeben {
            return true;
        }
        let meldung = format!(
            "'{name}' braucht {} ({}), du gibst {}.",
            if offen.len() == 1 { "an Typ".to_string() } else { format!("{} Typen", offen.len()) },
            offen.join(", "),
            if gegeben == 1 { "an".to_string() } else { gegeben.to_string() }
        );
        self.melde(pos, meldung);
        false
    }

    /// Die Klasse, die aus einer Vorlage mit diesen Typen entsteht.
    fn klassen_fassung(&mut self, name: &str, typen: Vec<Typ>, pos: Position) -> Typ {
        let schluessel = (name.to_string(), typen.clone());
        if let Some(nummer) = self.klassen_fassungen.get(&schluessel) {
            return Typ::Klasse(*nummer);
        }
        let mut k = self.klassen_vorlagen[name].clone();
        if !self.typen_anzahl_passt(name, &k.typ_parameter, typen.len(), pos) {
            return Typ::Fehler;
        }
        // Erst die Nummer vergeben: Die Klasse darf in ihren eigenen Feldern vorkommen.
        let nummer = self.typen.klassen.len() as u32;
        let voller_name = self.fassungs_name(name, &typen);
        self.typen.klassen.push(KlassenInfo { name: voller_name.clone(), ..KlassenInfo::default() });
        self.klassen_fassungen.insert(schluessel, nummer);
        self.klasse_aus.insert(nummer, (name.to_string(), typen.clone()));

        let umgebung: HashMap<String, Typ> = k.typ_parameter.iter().cloned().zip(typen).collect();
        let aussen = std::mem::replace(&mut self.typ_umgebung, umgebung.clone());
        let aussen_fassung = self.fassung.replace(voller_name.clone());
        let mut felder: Vec<KlassenFeldInfo> = Vec::new();
        for (stelle, feld) in k.felder.iter().enumerate() {
            let typ = self.typ_aus(&feld.angabe);
            if felder.iter().any(|f| f.name == feld.name) {
                let meldung = format!("Des Föd '{}' gibt's in '{name}' scho.", feld.name);
                self.melde(feld.pos, meldung);
                continue;
            }
            felder.push(KlassenFeldInfo {
                name: feld.name.clone(),
                typ,
                klasse: nummer,
                stelle,
                hat_standard: feld.standard.is_some(),
            });
        }
        let mut methoden: Vec<MethodenInfo> = Vec::new();
        for m in &mut k.methoden {
            for parameter in &mut m.parameter {
                parameter.typ = self.typ_aus(&parameter.angabe);
            }
            m.rueckgabe = m.rueckgabe_angabe.as_ref().map_or(Typ::Nix, |angabe| self.typ_aus(angabe));
            if methoden.iter().any(|alt| alt.name == m.name) {
                let meldung = format!("De Methode '{}' gibt's in '{name}' scho. Zwamoi geht ned.", m.name);
                self.melde(m.pos, meldung);
                continue;
            }
            if felder.iter().any(|f| f.name == m.name) {
                let meldung = format!("'{}' is in '{name}' scho a Föd, des kaun ned a no a Methode sei.", m.name);
                self.melde(m.pos, meldung);
                continue;
            }
            methoden.push(MethodenInfo {
                name: m.name.clone(),
                parameter: m.parameter.iter().map(|p| p.typ).collect(),
                rueckgabe: m.rueckgabe,
                klasse: nummer,
                platz: None,
            });
        }
        self.typ_umgebung = aussen;
        self.fassung = aussen_fassung;
        self.typen.klassen[nummer as usize].felder = felder;
        self.typen.klassen[nummer as usize].methoden = methoden;
        k.name = voller_name;
        k.typ_parameter.clear();
        self.offene_klassen.push((nummer, k, umgebung));
        Typ::Klasse(nummer)
    }

    /// Die Struktur, die aus einer Vorlage mit diesen Typen entsteht.
    fn struktur_fassung(&mut self, name: &str, typen: Vec<Typ>, pos: Position) -> Typ {
        let schluessel = (name.to_string(), typen.clone());
        if let Some(nummer) = self.struktur_fassungen.get(&schluessel) {
            return Typ::Struktur(*nummer);
        }
        let s = self.struktur_vorlagen[name].clone();
        if !self.typen_anzahl_passt(name, &s.typ_parameter, typen.len(), pos) {
            return Typ::Fehler;
        }
        let nummer = self.typen.strukturen.len() as u32;
        let voller_name = self.fassungs_name(name, &typen);
        self.typen.strukturen.push(StrukturInfo { name: voller_name.clone(), felder: Vec::new() });
        self.struktur_fassungen.insert(schluessel, nummer);
        self.struktur_aus.insert(nummer, (name.to_string(), typen.clone()));

        let umgebung: HashMap<String, Typ> = s.typ_parameter.iter().cloned().zip(typen).collect();
        let aussen = std::mem::replace(&mut self.typ_umgebung, umgebung);
        let aussen_fassung = self.fassung.replace(voller_name.clone());
        let mut felder: Vec<(String, Typ)> = Vec::new();
        for feld in &s.felder {
            let mut typ = self.typ_aus(&feld.angabe);
            if felder.iter().any(|(alt, _)| *alt == feld.name) {
                let meldung = format!("Des Föd '{}' gibt's in '{name}' scho.", feld.name);
                self.melde(feld.pos, meldung);
                continue;
            }
            if self.enthaelt(typ, nummer, 0) {
                let meldung = format!(
                    "De {} '{voller_name}' kaun si ned söwa enthoitn, de warat unendlich groß. Nimm a {} dafia.",
                    self.wort(Schluessel::Struct),
                    self.wort(Schluessel::List)
                );
                self.melde(feld.pos, meldung);
                typ = Typ::Fehler;
            }
            felder.push((feld.name.clone(), typ));
        }
        self.typ_umgebung = aussen;
        self.fassung = aussen_fassung;
        self.typen.strukturen[nummer as usize].felder = felder;
        Typ::Struktur(nummer)
    }

    /// Die Funktion, die aus einer Vorlage mit diesen Typen entsteht. Gibt ihren Namen zurück.
    fn funktions_fassung(&mut self, name: &str, typen: Vec<Typ>) -> String {
        let schluessel = (name.to_string(), typen.clone());
        if let Some(fertig) = self.funktions_fassungen.get(&schluessel) {
            return fertig.clone();
        }
        let mut f = self.funktions_vorlagen[name].clone();
        let voller_name = self.fassungs_name(name, &typen);
        self.funktions_fassungen.insert(schluessel, voller_name.clone());
        let umgebung: HashMap<String, Typ> = f.typ_parameter.iter().cloned().zip(typen).collect();
        let aussen = std::mem::replace(&mut self.typ_umgebung, umgebung.clone());
        let aussen_fassung = self.fassung.replace(voller_name.clone());
        for parameter in &mut f.parameter {
            parameter.typ = self.typ_aus(&parameter.angabe);
        }
        f.rueckgabe = f.rueckgabe_angabe.as_ref().map_or(Typ::Nix, |angabe| self.typ_aus(angabe));
        self.typ_umgebung = aussen;
        self.fassung = aussen_fassung;
        f.name = voller_name.clone();
        f.typ_parameter.clear();
        self.signaturen.insert(
            voller_name.clone(),
            Signatur { parameter: f.parameter.iter().map(|x| x.typ).collect(), rueckgabe: f.rueckgabe },
        );
        self.offene_funktionen.push((f, umgebung));
        voller_name
    }

    /// Liest aus einem Typ, wie er hingeschrieben ist, und dem Typ eines
    /// Werts ab, wofür die offenen Namen stehen müssen: `LISTN VON T` und
    /// eine `LISTN VON ZOI` ergibt `T` = `ZOI`. Wer zuerst kommt, gilt.
    fn gleiche_ab(&self, angabe: &TypAngabe, typ: Typ, offen: &[String], gefunden: &mut HashMap<String, Typ>) {
        match angabe {
            TypAngabe::Name { name, .. } if offen.contains(name) => {
                if !matches!(typ, Typ::Nix | Typ::Fehler) && self.fertig(typ) {
                    gefunden.entry(name.clone()).or_insert(typ);
                }
            }
            TypAngabe::Liste(innen) => {
                if let Some(element) = self.typen.element(typ) {
                    self.gleiche_ab(innen, element, offen, gefunden);
                }
            }
            // Wo VIELLEICHT T verlangt ist, passt auch ein T selbst.
            TypAngabe::Vielleicht { innen, .. } => {
                self.gleiche_ab(innen, self.typen.innen(typ).unwrap_or(typ), offen, gefunden);
            }
            TypAngabe::OderFehler { wert: Some(wert), .. } => {
                self.gleiche_ab(wert, self.typen.wert_von(typ).unwrap_or(typ), offen, gefunden);
            }
            TypAngabe::Funktion { parameter, rueckgabe, .. } => {
                if let Some(f) = self.typen.funktions_typ(typ) {
                    for (a, t) in parameter.iter().zip(&f.parameter) {
                        self.gleiche_ab(a, *t, offen, gefunden);
                    }
                    if let Some(rueckgabe) = rueckgabe {
                        self.gleiche_ab(rueckgabe, f.rueckgabe, offen, gefunden);
                    }
                }
            }
            TypAngabe::Generisch { name, argumente, .. } => {
                let aus = match typ {
                    Typ::Klasse(n) => self.klasse_aus.get(&n),
                    Typ::Struktur(n) => self.struktur_aus.get(&n),
                    _ => None,
                };
                if let Some((vorlage, typen)) = aus
                    && vorlage == name
                {
                    for (a, t) in argumente.iter().zip(typen) {
                        self.gleiche_ab(a, *t, offen, gefunden);
                    }
                }
            }
            TypAngabe::Verzeichnis { schluessel, wert, .. } => {
                if let Some((k, v)) = self.typen.verzeichnis(typ) {
                    self.gleiche_ab(schluessel, k, offen, gefunden);
                    self.gleiche_ab(wert, v, offen, gefunden);
                }
            }
            _ => {}
        }
    }

    /// Bestimmt aus den Werten eines Aufrufs, wofür die offenen Typen einer
    /// Vorlage stehen. Meldet, wenn einer offen bleibt.
    fn typen_ablesen(
        &mut self,
        name: &str,
        offen: &[String],
        angaben: &[TypAngabe],
        typen: &[Typ],
        pos: Position,
    ) -> Option<Vec<Typ>> {
        let mut gefunden = HashMap::new();
        for (angabe, typ) in angaben.iter().zip(typen) {
            self.gleiche_ab(angabe, *typ, offen, &mut gefunden);
        }
        if let Some(fehlt) = offen.iter().find(|o| !gefunden.contains_key(*o)) {
            let meldung = format!(
                "Bei '{name}' waaß i ned, wos '{fehlt}' sei soll: Aus de Werte in da Klammer geht's ned hervua."
            );
            self.melde(pos, meldung);
            return None;
        }
        Some(offen.iter().map(|o| gefunden[o]).collect())
    }

    /// `Stapel()` oder `Paar(1, "a")`, wo der Name eine Vorlage ist: Die Typen
    /// werden aus den Werten abgelesen.
    fn vorlage_bauen(&mut self, ausdruck: &mut Ausdruck) -> Typ {
        let pos = ausdruck.pos;
        let AusdruckArt::Aufruf { name, argumente } = &mut ausdruck.art else { unreachable!() };
        let name = name.clone();
        let typen: Vec<Typ> = argumente.iter_mut().map(|a| self.ausdruck(a)).collect();
        if typen.contains(&Typ::Fehler) {
            return Typ::Fehler;
        }
        // Was in der Klammer steht, sind die Felder der Reihe nach (bei Klassen die ohne eigenen Wert).
        let (offen, angaben) = match (self.klassen_vorlagen.get(&name), self.struktur_vorlagen.get(&name)) {
            (Some(k), _) => (
                k.typ_parameter.clone(),
                k.felder.iter().filter(|f| f.standard.is_none()).map(|f| f.angabe.clone()).collect::<Vec<_>>(),
            ),
            (_, Some(s)) => (s.typ_parameter.clone(), s.felder.iter().map(|f| f.angabe.clone()).collect()),
            _ => unreachable!("nur für Vorlagen aufgerufen"),
        };
        let mut gefunden = HashMap::new();
        for (angabe, typ) in angaben.iter().zip(&typen) {
            self.gleiche_ab(angabe, *typ, &offen, &mut gefunden);
        }
        if let Some(fehlt) = offen.iter().find(|o| !gefunden.contains_key(*o)) {
            let meldung = format!(
                "Bei '{name}' waaß i ned, wos '{fehlt}' sei soll. Schreib's dazua: {name} {} ...(...)",
                self.wort(Schluessel::From)
            );
            self.melde(pos, meldung);
            return Typ::Fehler;
        }
        let gewaehlt: Vec<Typ> = offen.iter().map(|o| gefunden[o]).collect();
        if self.klassen_vorlagen.contains_key(&name) {
            match self.klassen_fassung(&name, gewaehlt, pos) {
                Typ::Klasse(nummer) => self.neu(ausdruck, nummer, Some(typen)),
                _ => Typ::Fehler,
            }
        } else {
            match self.struktur_fassung(&name, gewaehlt, pos) {
                Typ::Struktur(nummer) => self.bau(ausdruck, nummer, Some(typen)),
                _ => Typ::Fehler,
            }
        }
    }

    /// `Stapel VON ZOI(...)`: ein neuer Wert, dessen Typ dabeisteht.
    fn erzeug(&mut self, ausdruck: &mut Ausdruck) -> Typ {
        let pos = ausdruck.pos;
        let AusdruckArt::Erzeug { angabe, mut argumente } = std::mem::replace(&mut ausdruck.art, AusdruckArt::Nix)
        else {
            unreachable!()
        };
        let typ = self.typ_aus(&angabe);
        let typen: Vec<Typ> = argumente.iter_mut().map(|a| self.ausdruck(a)).collect();
        let name = self.typ_wort(typ);
        match typ {
            Typ::Klasse(nummer) => {
                ausdruck.art = AusdruckArt::Aufruf { name, argumente };
                self.neu(ausdruck, nummer, Some(typen))
            }
            Typ::Struktur(nummer) => {
                ausdruck.art = AusdruckArt::Aufruf { name, argumente };
                self.bau(ausdruck, nummer, Some(typen))
            }
            Typ::Verzeichnis(_) => {
                if !argumente.is_empty() {
                    let meldung = format!("A neichs {} is laa: In de Klammer ghört nix.", self.wort(Schluessel::Map));
                    self.melde(pos, meldung);
                }
                ausdruck.art = AusdruckArt::Erzeug { angabe, argumente: Vec::new() };
                ausdruck.typ = typ;
                typ
            }
            Typ::Fehler => Typ::Fehler,
            _ => {
                let meldung = format!("{} kaun ma so ned aulegn.", self.typ_name(typ).replacen("a ", "A ", 1));
                self.melde(pos, meldung);
                Typ::Fehler
            }
        }
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
            Typ::Klasse(_) => self.typen.klasse(typ).map_or_else(String::new, |k| k.name.clone()),
            Typ::Liste(_) => format!(
                "{} {} {}",
                self.wort(Schluessel::List),
                self.wort(Schluessel::From),
                self.typ_wort(self.typen.element(typ).unwrap_or(Typ::Fehler))
            ),
            Typ::LeereListe => format!("laare {}", self.wort(Schluessel::List)),
            Typ::Vielleicht { .. } => format!(
                "{} {}",
                self.wort(Schluessel::Maybe),
                self.typ_wort(self.typen.innen(typ).unwrap_or(Typ::Fehler))
            ),
            Typ::Ergebnis(_) => {
                let vorne = match self.typen.wert_von(typ) {
                    Some(Typ::Nix) | None => self.wort(Schluessel::Null).to_string(),
                    Some(wert) => self.typ_wort(wert),
                };
                format!("{vorne} {} {}", self.wort(Schluessel::Or), self.wort(Schluessel::Error))
            }
            Typ::Fehla => self.wort(Schluessel::Error).to_string(),
            Typ::NixWert => self.wort(Schluessel::Null).to_string(),
            Typ::Funktion(_) => {
                let Some(f) = self.typen.funktions_typ(typ) else { return self.wort(Schluessel::Function).to_string() };
                let parameter: Vec<String> = f.parameter.iter().map(|t| self.typ_wort(*t)).collect();
                let mut text = format!("{}({})", self.wort(Schluessel::Function), parameter.join(", "));
                if f.rueckgabe != Typ::Nix {
                    text.push_str(&format!(" {} {}", self.wort(Schluessel::Returns), self.typ_wort(f.rueckgabe)));
                }
                text
            }
            Typ::Verzeichnis(_) => {
                let (schluessel, wert) = self.typen.verzeichnis(typ).unwrap_or((Typ::Fehler, Typ::Fehler));
                format!(
                    "{} {} {} {} {}",
                    self.wort(Schluessel::Map),
                    self.wort(Schluessel::From),
                    self.typ_wort(schluessel),
                    self.wort(Schluessel::And),
                    self.typ_wort(wert)
                )
            }
            Typ::Nix | Typ::Fehler => "nix".to_string(),
        }
    }

    /// Wie ein Typ in einer Meldung heißt, mit Artikel: "a ZOI".
    fn typ_name(&self, typ: Typ) -> String {
        match typ {
            Typ::Nix | Typ::Fehler => "nix".to_string(),
            Typ::NixWert => self.wort(Schluessel::Null).to_string(),
            _ => format!("a {}", self.typ_wort(typ)),
        }
    }

    /// Macht aus einem Typ, wie er im Quelltext steht, den Typ selbst.
    fn typ_aus(&mut self, angabe: &TypAngabe) -> Typ {
        match angabe {
            TypAngabe::Einfach(typ) => *typ,
            TypAngabe::Name { name, pos } => {
                // In einer Vorlage: wofür der Name gerade steht.
                if let Some(typ) = self.typ_umgebung.get(name) {
                    return *typ;
                }
                if let Some(nummer) = self.struktur_nummern.get(name) {
                    return Typ::Struktur(*nummer);
                }
                if let Some(nummer) = self.klassen_nummern.get(name) {
                    return Typ::Klasse(*nummer);
                }
                let vorlage = self.klassen_vorlagen.contains_key(name) || self.struktur_vorlagen.contains_key(name);
                let meldung = match self.wb.festes_wort(name) {
                    Some(schluessel) => format!("Des haaßt {}, ned {}!", self.wort(schluessel), name),
                    None if vorlage => format!(
                        "'{name}' is a Vorlog, do ghört dazua, wovon: {name} {} ...",
                        self.wort(Schluessel::From)
                    ),
                    None => format!(
                        "An Typ '{name}' kenn i ned. Gibt's a {} oda a {} mit dem Nom?",
                        self.wort(Schluessel::Struct),
                        self.wort(Schluessel::Class)
                    ),
                };
                self.melde(*pos, meldung);
                Typ::Fehler
            }
            TypAngabe::Liste(innen) => match self.typ_aus(innen) {
                Typ::Fehler => Typ::Fehler,
                element => self.typen.liste_von(element),
            },
            TypAngabe::Vielleicht { innen, pos } => match self.typ_aus(innen) {
                Typ::Fehler => Typ::Fehler,
                Typ::Vielleicht { .. } => {
                    let meldung = format!("Zwamoi {} hintaranaund bringt nix.", self.wort(Schluessel::Maybe));
                    self.melde(*pos, meldung);
                    Typ::Fehler
                }
                wert => self.typen.vielleicht_von(wert),
            },
            TypAngabe::OderFehler { wert: None, .. } => self.typen.ergebnis_von(Typ::Nix),
            TypAngabe::OderFehler { wert: Some(innen), pos } => match self.typ_aus(innen) {
                Typ::Fehler => Typ::Fehler,
                Typ::Fehla => {
                    let meldung = format!(
                        "{f} {} {f}? Wos ka Wert hot, gibt {} {} {f} zruck.",
                        self.wort(Schluessel::Or),
                        self.wort(Schluessel::Null),
                        self.wort(Schluessel::Or),
                        f = self.wort(Schluessel::Error)
                    );
                    self.melde(*pos, meldung);
                    Typ::Fehler
                }
                wert => self.typen.ergebnis_von(wert),
            },
            TypAngabe::Funktion { parameter, rueckgabe, .. } => {
                let parameter: Vec<Typ> = parameter.iter().map(|a| self.typ_aus(a)).collect();
                let rueckgabe = rueckgabe.as_ref().map_or(Typ::Nix, |a| self.typ_aus(a));
                if parameter.contains(&Typ::Fehler) || rueckgabe == Typ::Fehler {
                    Typ::Fehler
                } else {
                    self.typen.funktion_von(parameter, rueckgabe)
                }
            }
            TypAngabe::Generisch { name, argumente, pos } => {
                let typen: Vec<Typ> = argumente.iter().map(|a| self.typ_aus(a)).collect();
                if typen.contains(&Typ::Fehler) {
                    Typ::Fehler
                } else if self.klassen_vorlagen.contains_key(name) {
                    self.klassen_fassung(name, typen, *pos)
                } else if self.struktur_vorlagen.contains_key(name) {
                    self.struktur_fassung(name, typen, *pos)
                } else {
                    let bekannt = self.struktur_nummern.contains_key(name) || self.klassen_nummern.contains_key(name);
                    let meldung = if bekannt {
                        format!(
                            "'{name}' is ka Vorlog, do ghört ka {} dahinta. (A Vorlog schaut so aus: {} {name} {} T)",
                            self.wort(Schluessel::From),
                            self.wort(Schluessel::Class),
                            self.wort(Schluessel::From)
                        )
                    } else {
                        format!("A Vorlog '{name}' kenn i ned.")
                    };
                    self.melde(*pos, meldung);
                    Typ::Fehler
                }
            }
            TypAngabe::Verzeichnis { schluessel, wert, pos } => {
                let k = self.typ_aus(schluessel);
                let w = self.typ_aus(wert);
                if k == Typ::Fehler || w == Typ::Fehler {
                    Typ::Fehler
                } else if !matches!(k, Typ::Zoi | Typ::Kettn) {
                    let meldung = format!(
                        "Ois Schlissl in an {} geht nur {} oda {}, ned {}.",
                        self.wort(Schluessel::Map),
                        self.typ_name(Typ::Zoi),
                        self.typ_name(Typ::Kettn),
                        self.typ_name(k)
                    );
                    self.melde(*pos, meldung);
                    Typ::Fehler
                } else if matches!(w, Typ::Vielleicht { .. }) {
                    let meldung = format!(
                        "Da Wert in an {m} kaun ned {} sei: Wos fehlt, steht afoch ned drin.",
                        self.wort(Schluessel::Maybe),
                        m = self.wort(Schluessel::Map)
                    );
                    self.melde(*pos, meldung);
                    Typ::Fehler
                } else {
                    self.typen.verzeichnis_von(k, w)
                }
            }
        }
    }

    /// Steht der Typ ganz fest? Bei `[]` ohne Angabe ist offen, wovon es eine
    /// Liste ist, bei `NIX` allein, was fehlen soll.
    fn fertig(&self, typ: Typ) -> bool {
        match typ {
            Typ::LeereListe | Typ::NixWert => false,
            Typ::Liste(_) => self.typen.element(typ).is_some_and(|e| self.fertig(e)),
            Typ::Vielleicht { .. } => self.typen.innen(typ).is_some_and(|t| self.fertig(t)),
            _ => true,
        }
    }

    /// Meldet eine Liste, von der nicht klar ist, was drin sein soll.
    fn fertig_verlangt(&mut self, ausdruck: &Ausdruck) -> bool {
        if self.fertig(ausdruck.typ) {
            return true;
        }
        if ausdruck.typ == Typ::NixWert {
            let meldung = format!(
                "Bei {nix} waaß i ned, wos do fehln soll. Sog's dazua, zum Beispü: {} x {} {} = {nix} {}",
                self.wort(Schluessel::Declare),
                self.wort(Schluessel::Maybe),
                self.wort(Schluessel::Int),
                self.wort(Schluessel::Oida),
                nix = self.wort(Schluessel::Null),
            );
            self.melde(ausdruck.pos, meldung);
            return false;
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
    fn strukturen(&mut self, definitionen: &[StrukturDef], klassen: &[KlassenDef]) {
        // Welche Definition zu welcher Nummer gehört (doppelte Namen fallen weg).
        let mut gueltig: Vec<&StrukturDef> = Vec::new();
        for s in definitionen {
            if self.struktur_nummern.contains_key(&s.name) || self.struktur_vorlagen.contains_key(&s.name) {
                let meldung = format!("De {} '{}' gibt's scho. Zwamoi geht ned.", self.wort(Schluessel::Struct), s.name);
                self.melde(s.pos, meldung);
                continue;
            }
            // Eine Vorlage bekommt selbst keine Nummer; erst ihre Fassungen.
            if !s.typ_parameter.is_empty() {
                self.struktur_vorlagen.insert(s.name.clone(), s.clone());
                continue;
            }
            self.struktur_nummern.insert(s.name.clone(), gueltig.len() as u32);
            self.typen.strukturen.push(StrukturInfo { name: s.name.clone(), felder: Vec::new() });
            gueltig.push(s);
        }
        // Auch die Namen der Klassen müssen bekannt sein, bevor Felder aufgelöst werden.
        self.klassen_namen(klassen);
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
            // Auch VIELLEICHT Punkt und Punkt ODA FEHLA tragen den Punkt direkt in sich.
            Typ::Vielleicht { .. } => self.typen.innen(typ).is_some_and(|t| self.enthaelt(t, gesucht, tiefe)),
            Typ::Ergebnis(_) => self.typen.wert_von(typ).is_some_and(|t| self.enthaelt(t, gesucht, tiefe)),
            _ => false,
        }
    }

    // ----------------------------------------------------------- Klassen

    /// Trägt die Namen aller Klassen ein. Jede Klasse bekommt die Nummer, an
    /// der sie im Programm steht; eine doppelte bleibt leer und wird gemeldet.
    fn klassen_namen(&mut self, definitionen: &[KlassenDef]) {
        for (nummer, k) in definitionen.iter().enumerate() {
            let vorlage = !k.typ_parameter.is_empty();
            self.typen.klassen.push(KlassenInfo { name: k.name.clone(), vorlage, ..KlassenInfo::default() });
            if self.klassen_nummern.contains_key(&k.name) || self.klassen_vorlagen.contains_key(&k.name) {
                let meldung = format!("De {} '{}' gibt's scho. Zwamoi geht ned.", self.wort(Schluessel::Class), k.name);
                self.melde(k.pos, meldung);
            } else if self.struktur_nummern.contains_key(&k.name) || self.struktur_vorlagen.contains_key(&k.name) {
                let meldung = format!(
                    "'{}' gibt's scho ois {}, des kaun ned a no a {} sei.",
                    k.name,
                    self.wort(Schluessel::Struct),
                    self.wort(Schluessel::Class)
                );
                self.melde(k.pos, meldung);
            } else if vorlage {
                // Eine Vorlage erbt nicht und vererbt nicht: Jede Fassung steht für sich.
                if let Some((_, pos)) = &k.eltern {
                    let meldung = format!(
                        "A Vorlog ({} ... {} ...) kaun ned erbn.",
                        self.wort(Schluessel::Class),
                        self.wort(Schluessel::From)
                    );
                    self.melde(*pos, meldung);
                }
                self.klassen_vorlagen.insert(k.name.clone(), k.clone());
            } else {
                self.klassen_nummern.insert(k.name.clone(), nummer as u32);
            }
        }
    }

    /// Die oberste Klasse der Familie.
    fn wurzel(&self, klasse: u32) -> u32 {
        let mut aktuell = klasse;
        for _ in 0..self.typen.klassen.len() {
            match self.typen.klassen[aktuell as usize].eltern {
                Some(eltern) => aktuell = eltern,
                None => break,
            }
        }
        aktuell
    }

    /// Löst auf, wer von wem erbt, und baut für jede Klasse die Liste ihrer
    /// Felder und Methoden (die geerbten zuerst).
    fn klassen(&mut self, definitionen: &mut [KlassenDef]) {
        let gueltig: Vec<usize> = (0..definitionen.len())
            .filter(|n| self.klassen_nummern.get(&definitionen[*n].name) == Some(&(*n as u32)))
            .collect();

        // Wer erbt von wem?
        for &n in &gueltig {
            let k = &definitionen[n];
            let Some((eltern_name, pos)) = &k.eltern else { continue };
            match self.klassen_nummern.get(eltern_name).copied() {
                Some(eltern) if eltern as usize == n => {
                    let meldung =
                        format!("De {} '{}' kaun ned von si söwa erbn.", self.wort(Schluessel::Class), k.name);
                    self.melde(*pos, meldung);
                }
                Some(eltern) => self.typen.klassen[n].eltern = Some(eltern),
                None => {
                    let meldung = if self.klassen_vorlagen.contains_key(eltern_name) {
                        format!("Von ana Vorlog wia '{eltern_name}' kaun ma ned erbn.")
                    } else if self.struktur_nummern.contains_key(eltern_name) {
                        format!(
                            "'{eltern_name}' is a {}, erbn kaun ma nur von ana {}.",
                            self.wort(Schluessel::Struct),
                            self.wort(Schluessel::Class)
                        )
                    } else {
                        format!("De {} '{eltern_name}' kenn i ned.", self.wort(Schluessel::Class))
                    };
                    self.melde(*pos, meldung);
                }
            }
        }
        // Im Kreis erben geht nicht: A von B und B von A.
        for &n in &gueltig {
            let mut aktuell = self.typen.klassen[n].eltern;
            for _ in 0..definitionen.len() {
                match aktuell {
                    Some(e) if e as usize == n => {
                        let meldung = format!(
                            "De {} '{}' erbt im Kreis von si söwa.",
                            self.wort(Schluessel::Class),
                            definitionen[n].name
                        );
                        self.melde(definitionen[n].pos, meldung);
                        self.typen.klassen[n].eltern = None;
                        break;
                    }
                    Some(e) => aktuell = self.typen.klassen[e as usize].eltern,
                    None => break,
                }
            }
        }

        // Eltern vor den Kindern drannehmen, damit die geerbten Felder und Methoden schon feststehen.
        let tiefe = |typen: &Typen, n: usize| {
            let mut stufen = 0;
            let mut aktuell = typen.klassen[n].eltern;
            while let Some(e) = aktuell {
                stufen += 1;
                aktuell = typen.klassen[e as usize].eltern;
            }
            stufen
        };
        let mut reihenfolge = gueltig.clone();
        reihenfolge.sort_by_key(|n| tiefe(&self.typen, *n));

        // Methoden, die eine geerbte ersetzen: (Klasse, Name)
        let mut ueberschrieben: Vec<(u32, String)> = Vec::new();
        for n in reihenfolge {
            let klassen_name = definitionen[n].name.clone();
            let (mut felder, mut methoden) = match self.typen.klassen[n].eltern {
                Some(e) => {
                    let eltern = &self.typen.klassen[e as usize];
                    (eltern.felder.clone(), eltern.methoden.clone())
                }
                None => (Vec::new(), Vec::new()),
            };
            for (stelle, feld) in definitionen[n].felder.iter().enumerate() {
                let typ = self.typ_aus(&feld.angabe);
                if felder.iter().any(|f| f.name == feld.name) {
                    let meldung = format!("Des Föd '{}' gibt's in '{klassen_name}' scho.", feld.name);
                    self.melde(feld.pos, meldung);
                    continue;
                }
                felder.push(KlassenFeldInfo {
                    name: feld.name.clone(),
                    typ,
                    klasse: n as u32,
                    stelle,
                    hat_standard: feld.standard.is_some(),
                });
            }
            let mut eigene: Vec<String> = Vec::new();
            for m in &mut definitionen[n].methoden {
                for parameter in &mut m.parameter {
                    parameter.typ = self.typ_aus(&parameter.angabe);
                }
                m.rueckgabe = m.rueckgabe_angabe.as_ref().map_or(Typ::Nix, |angabe| self.typ_aus(angabe));
                if eigene.contains(&m.name) {
                    let meldung = format!("De Methode '{}' gibt's in '{klassen_name}' scho. Zwamoi geht ned.", m.name);
                    self.melde(m.pos, meldung);
                    continue;
                }
                eigene.push(m.name.clone());
                if felder.iter().any(|f| f.name == m.name) {
                    let meldung = format!(
                        "'{}' is in '{klassen_name}' scho a Föd, des kaun ned a no a Methode sei.",
                        m.name
                    );
                    self.melde(m.pos, meldung);
                    continue;
                }
                let info = MethodenInfo {
                    name: m.name.clone(),
                    parameter: m.parameter.iter().map(|p| p.typ).collect(),
                    rueckgabe: m.rueckgabe,
                    klasse: n as u32,
                    platz: None,
                };
                match methoden.iter_mut().find(|alt| alt.name == m.name) {
                    Some(alt) => {
                        // Überschreiben: Die neue Fassung muss genauso aufzurufen sein wie die alte.
                        if alt.parameter != info.parameter || alt.rueckgabe != info.rueckgabe {
                            let meldung = format!(
                                "De Methode '{}' in '{klassen_name}' passt ned zu der in '{}': \
                                 Sie muass de gleichn Werte nehma und des Gleiche zruckgebn.",
                                m.name, self.typen.klassen[alt.klasse as usize].name
                            );
                            self.fehler.push(Fehler::neu(m.pos, meldung));
                        }
                        ueberschrieben.push((n as u32, m.name.clone()));
                        *alt = info;
                    }
                    None => methoden.push(info),
                }
            }
            self.typen.klassen[n].felder = felder;
            self.typen.klassen[n].methoden = methoden;
        }

        // Sprungtabellen: Eine Methode, die irgendwo in der Familie überschrieben
        // wird, bekommt einen Platz; alle anderen werden direkt aufgerufen.
        let mut plaetze: HashMap<(u32, String), usize> = HashMap::new();
        let mut anzahl: HashMap<u32, usize> = HashMap::new();
        for (klasse, name) in ueberschrieben {
            let wurzel = self.wurzel(klasse);
            plaetze.entry((wurzel, name)).or_insert_with(|| {
                let naechster = anzahl.entry(wurzel).or_default();
                *naechster += 1;
                *naechster - 1
            });
        }
        for &n in &gueltig {
            let wurzel = self.wurzel(n as u32);
            for m in &mut self.typen.klassen[n].methoden {
                m.platz = plaetze.get(&(wurzel, m.name.clone())).copied();
            }
            self.typen.klassen[n].plaetze = anzahl.get(&wurzel).copied().unwrap_or(0);
        }
    }

    /// Prüft, was in einer Klasse steht: die Werte, mit denen Felder beginnen, und die Methoden.
    fn klassen_inhalt(&mut self, nummer: u32, k: &mut KlassenDef) {
        for feld in &mut k.felder {
            let Some(standard) = &mut feld.standard else { continue };
            // Der Wert wird beim Anlegen des Objekts ausgerechnet: Da gibt es noch kein SÖWA.
            self.bereiche = vec![HashMap::new()];
            self.funktion = None;
            self.selbst = None;
            self.schleifen = 0;
            let typ = self.ausdruck(standard);
            let ziel = self.typen.klassen[nummer as usize]
                .felder
                .iter()
                .find(|f| f.klasse == nummer && f.name == feld.name)
                .map(|f| f.typ);
            let Some(ziel) = ziel else { continue };
            if typ == Typ::Nix {
                self.melde(standard.pos, "Des gibt nix zruck, des kaunst in ka Föd einetuan.");
            } else if !self.anpassen(standard, ziel) {
                let meldung = format!(
                    "Des Föd '{}' is {}, da Wert dahinta is oba {}.",
                    feld.name,
                    self.typ_name(ziel),
                    self.typ_name(typ)
                );
                self.melde(standard.pos, meldung);
            } else {
                self.fertig_verlangt(standard);
            }
        }
        self.methoden_klasse = Some(nummer);
        for m in &mut k.methoden {
            self.funktion(m);
        }
        self.methoden_klasse = None;
    }

    /// `Gegner("Hansi", 5)`: ein neues Objekt. Angegeben werden die Felder,
    /// die keinen eigenen Wert haben, der Reihe nach (die geerbten zuerst).
    fn neu(&mut self, ausdruck: &mut Ausdruck, nummer: u32, geprueft: Option<Vec<Typ>>) -> Typ {
        let AusdruckArt::Aufruf { name, argumente } = &mut ausdruck.art else { unreachable!() };
        let name = name.clone();
        let mut argumente = std::mem::take(argumente);
        // Sind die Werte schon geprüft (bei Vorlagen), kein zweites Mal.
        let typen: Vec<Typ> = match geprueft {
            Some(typen) => typen,
            None => argumente.iter_mut().map(|a| self.ausdruck(a)).collect(),
        };
        let felder: Vec<KlassenFeldInfo> =
            self.typen.klassen[nummer as usize].felder.iter().filter(|f| !f.hat_standard).cloned().collect();
        if felder.len() != argumente.len() {
            let namen: Vec<&str> = felder.iter().map(|f| f.name.as_str()).collect();
            let braucht = match felder.len() {
                0 => "kane Werte".to_string(),
                1 => format!("an Wert ({})", namen[0]),
                n => format!("{n} Werte ({})", namen.join(", ")),
            };
            let meldung = format!("'{name}' braucht {braucht}, du gibst eam {}.", anzahl(argumente.len()));
            self.melde(ausdruck.pos, meldung);
        } else {
            for (i, (argument, feld)) in argumente.iter_mut().zip(&felder).enumerate() {
                if !self.anpassen(argument, feld.typ) {
                    let meldung = format!(
                        "'{name}' wü fia des Föd '{}' {}, du gibst eam {}.",
                        feld.name,
                        self.typ_name(feld.typ),
                        self.typ_name(typen[i])
                    );
                    self.melde(argument.pos, meldung);
                }
            }
        }
        ausdruck.art = AusdruckArt::Neu { klasse: nummer, argumente };
        ausdruck.typ = Typ::Klasse(nummer);
        ausdruck.typ
    }

    /// Prüft den Aufruf `objekt.name(argumente)`. Gibt die Stelle der
    /// Methode in der Klasse und ihren Rückgabetyp zurück.
    fn methode(
        &mut self,
        objekt_typ: Typ,
        name: &str,
        argumente: &mut [Ausdruck],
        typen: &[Typ],
        pos: Position,
    ) -> Option<(usize, Typ)> {
        if objekt_typ == Typ::Fehler {
            return None;
        }
        let Some(klasse) = self.typen.klasse(objekt_typ).cloned() else {
            let meldung = format!("{} hot kane Methodn.", self.typ_name(objekt_typ).replacen("a ", "A ", 1));
            self.melde(pos, meldung);
            return None;
        };
        let Some(stelle) = klasse.methoden.iter().position(|m| m.name == name) else {
            let meldung = if klasse.felder.iter().any(|f| f.name == name) {
                format!("'{name}' is in '{}' a Föd, ka Methode. Loss de Klammern weg.", klasse.name)
            } else if klasse.methoden.is_empty() {
                format!("A Methode '{name}' gibt's in '{}' ned, de hot gor kane.", klasse.name)
            } else {
                let namen: Vec<&str> = klasse.methoden.iter().map(|m| m.name.as_str()).collect();
                format!("A Methode '{name}' gibt's in '{}' ned. Es gibt: {}.", klasse.name, namen.join(", "))
            };
            self.melde(pos, meldung);
            return None;
        };
        let methode = &klasse.methoden[stelle];
        if typen.contains(&Typ::Fehler) {
            return Some((stelle, methode.rueckgabe));
        }
        if methode.parameter.len() != argumente.len() {
            let meldung = format!(
                "'{name}' wü {} hom, du gibst eam {}.",
                anzahl(methode.parameter.len()),
                anzahl(argumente.len())
            );
            self.melde(pos, meldung);
            return Some((stelle, methode.rueckgabe));
        }
        for (i, (argument, ziel)) in argumente.iter_mut().zip(&methode.parameter).enumerate() {
            if !self.anpassen(argument, *ziel) {
                let meldung = format!(
                    "'{name}' wü ois {}. Wert {}, du gibst eam {}.",
                    i + 1,
                    self.typ_name(*ziel),
                    self.typ_name(typen[i])
                );
                self.melde(argument.pos, meldung);
            }
        }
        Some((stelle, methode.rueckgabe))
    }

    /// Prüft `DE OIDN.name(argumente)`. Gibt die Klasse zurück, in der die
    /// aufgerufene Fassung steht, und ihren Rückgabetyp.
    fn eltern_aufruf(&mut self, name: &str, argumente: &mut [Ausdruck], pos: Position) -> Option<(u32, Typ)> {
        let typen: Vec<Typ> = argumente.iter_mut().map(|a| self.ausdruck(a)).collect();
        let Some((klasse, _)) = self.selbst else {
            let meldung = format!("{} gibt's nur in ana Methode.", self.wort(Schluessel::Super));
            self.melde(pos, meldung);
            return None;
        };
        if !self.lambdas.is_empty() {
            let meldung = format!(
                "{} geht ned in ana {} ohne Nom. Ruaf de Methode vorher auf und nimm des Ergebnis mit.",
                self.wort(Schluessel::Super),
                self.wort(Schluessel::Function)
            );
            self.melde(pos, meldung);
            return None;
        }
        let Some(eltern) = self.typen.klassen[klasse as usize].eltern else {
            let meldung = format!(
                "'{}' erbt von kana {}, do gibt's kane {}.",
                self.typen.klassen[klasse as usize].name,
                self.wort(Schluessel::Class),
                self.wort(Schluessel::Super)
            );
            self.melde(pos, meldung);
            return None;
        };
        let (stelle, rueckgabe) = self.methode(Typ::Klasse(eltern), name, argumente, &typen, pos)?;
        Some((self.typen.klassen[eltern as usize].methoden[stelle].klasse, rueckgabe))
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

    /// Sucht eine Variable von der Stelle aus, an der `ebene` Funktionen ohne
    /// Namen offen sind. Liegt sie außerhalb der innersten, wird sie
    /// mitgenommen: Die Funktion bekommt eine eigene Kopie mit eigener Nummer.
    fn hol_variable(&mut self, name: &str, ebene: usize) -> Option<(usize, Typ)> {
        let oben = if ebene < self.lambdas.len() { self.lambdas[ebene].grenze } else { self.bereiche.len() };
        let unten = if ebene > 0 { self.lambdas[ebene - 1].grenze } else { 0 };
        if let Some(v) = self.bereiche[unten..oben].iter().rev().find_map(|b| b.get(name)) {
            return Some((v.id, v.typ));
        }
        if ebene == 0 {
            return None;
        }
        if let Some(f) = self.lambdas[ebene - 1].fang.iter().find(|f| f.name == name) {
            return Some((f.id, f.typ));
        }
        let (aussen, typ) = self.hol_variable(name, ebene - 1)?;
        let id = self.naechste_id;
        self.naechste_id += 1;
        let pos = self.lambdas[ebene - 1].pos;
        let wert = Ausdruck { art: AusdruckArt::Variable { name: name.to_string(), id: aussen }, pos, typ };
        self.lambdas[ebene - 1].fang.push(Fang { name: name.to_string(), wert, id, typ });
        Some((id, typ))
    }

    /// Wie [`Self::hol_variable`], für SÖWA.
    fn hol_selbst(&mut self, ebene: usize) -> Option<(u32, usize)> {
        let (klasse, basis) = self.selbst?;
        if ebene == 0 {
            return Some((klasse, basis));
        }
        if let Some(id) = self.lambdas[ebene - 1].selbst {
            return Some((klasse, id));
        }
        let (_, aussen) = self.hol_selbst(ebene - 1)?;
        let id = self.naechste_id;
        self.naechste_id += 1;
        let pos = self.lambdas[ebene - 1].pos;
        let typ = Typ::Klasse(klasse);
        let wert = Ausdruck { art: AusdruckArt::Selbst { id: aussen }, pos, typ };
        let name = self.wort(Schluessel::This).to_string();
        self.lambdas[ebene - 1].fang.push(Fang { name, wert, id, typ });
        self.lambdas[ebene - 1].selbst = Some(id);
        Some((klasse, id))
    }

    /// Würde die Zuweisung an `ziel` einen Wert ändern, den die Funktion ohne
    /// Namen von außen mitgenommen hat? Das ist nur eine Kopie; die Änderung ginge verloren.
    fn aendert_fang(&self, ziel: &Ausdruck) -> Option<String> {
        let kontext = self.lambdas.last()?;
        match &ziel.art {
            AusdruckArt::Variable { name, id } => kontext.fang.iter().any(|f| f.id == *id).then(|| name.clone()),
            AusdruckArt::Feld { objekt, .. } if matches!(objekt.typ, Typ::Struktur(_)) => self.aendert_fang(objekt),
            AusdruckArt::Sicher { innen } => self.aendert_fang(innen),
            _ => None,
        }
    }

    /// Prüft eine Funktion ohne Namen. Gibt ihren Typ zurück und was sie von außen mitnimmt.
    fn lambda(&mut self, f: &mut Funktion, kurz: bool, pos: Position) -> (Typ, Vec<Fang>) {
        for parameter in &mut f.parameter {
            parameter.typ = self.typ_aus(&parameter.angabe);
        }
        f.rueckgabe = f.rueckgabe_angabe.as_ref().map_or(Typ::Nix, |angabe| self.typ_aus(angabe));
        let name = "ohne Nom".to_string();

        // Was für die Funktion rundherum gilt, hat drinnen nichts verloren.
        let alt_funktion = self.funktion.take();
        let alt_schleifen = std::mem::replace(&mut self.schleifen, 0);
        let alt_sicher = std::mem::take(&mut self.sicher);
        self.lambdas.push(LambdaKontext { grenze: self.bereiche.len(), fang: Vec::new(), selbst: None, pos });
        self.bereiche.push(HashMap::new());
        for parameter in &mut f.parameter {
            parameter.id = self.neue_variable(&parameter.name, parameter.typ, parameter.pos);
        }
        if kurz {
            // FUNKTIAUN(x ZOI) => x * 2: Was der Ausdruck ergibt, gibt die Funktion zurück.
            let AnweisungArt::GibZruck { wert } = &mut f.rumpf[0].art else { unreachable!("so baut es der Parser") };
            let mut ausdruck = wert.take().expect("so baut es der Parser");
            self.funktion = Some((name, Typ::Fehler));
            let mut typ = self.ausdruck(&mut ausdruck);
            if typ == Typ::Nix {
                f.rumpf[0].art = AnweisungArt::Ausdruck(ausdruck);
            } else {
                if typ != Typ::Fehler && !self.fertig_verlangt(&ausdruck) {
                    typ = Typ::Fehler;
                }
                f.rumpf[0].art = AnweisungArt::GibZruck { wert: Some(ausdruck) };
            }
            f.rueckgabe = typ;
        } else {
            self.funktion = Some((name, f.rueckgabe));
            for a in &mut f.rumpf {
                self.anweisung(a);
            }
            let ohne_wert = f.rueckgabe == Typ::Nix || self.typen.wert_von(f.rueckgabe) == Some(Typ::Nix);
            if !ohne_wert && f.rueckgabe != Typ::Fehler && !gibt_sicher_zruck(&f.rumpf) {
                let meldung = format!(
                    "De {} muass {} zruckgebn, oba ned jeda Weg hört mit {} auf.",
                    self.wort(Schluessel::Function),
                    self.typ_name(f.rueckgabe),
                    self.wort(Schluessel::Return)
                );
                self.melde(pos, meldung);
            }
        }
        self.bereiche.pop();
        let kontext = self.lambdas.pop().expect("gerade angelegt");
        self.funktion = alt_funktion;
        self.schleifen = alt_schleifen;
        self.sicher = alt_sicher;

        let parameter: Vec<Typ> = f.parameter.iter().map(|p| p.typ).collect();
        let typ = if parameter.contains(&Typ::Fehler) || f.rueckgabe == Typ::Fehler {
            Typ::Fehler
        } else {
            self.typen.funktion_von(parameter, f.rueckgabe)
        };
        (typ, kontext.fang)
    }

    /// Prüft den Aufruf einer Funktion, die ein Wert ist.
    fn wert_aufruf(
        &mut self,
        funktion: &Ausdruck,
        f_typ: Typ,
        argumente: &mut [Ausdruck],
        typen: &[Typ],
        pos: Position,
    ) -> Typ {
        if f_typ == Typ::Fehler {
            return Typ::Fehler;
        }
        let Some(f) = self.typen.funktions_typ(f_typ).cloned() else {
            let meldung = self.nix_hinweis(funktion, f_typ).unwrap_or_else(|| {
                format!(
                    "{} kaun ma ned aufruafn, des is ka {}.",
                    self.typ_name(f_typ).replacen("a ", "A ", 1),
                    self.wort(Schluessel::Function)
                )
            });
            self.melde(funktion.pos, meldung);
            return Typ::Fehler;
        };
        let wer = match ziel_text(funktion).as_str() {
            "des" => format!("De {}", self.wort(Schluessel::Function)),
            name => format!("'{name}'"),
        };
        if typen.contains(&Typ::Fehler) {
            return f.rueckgabe;
        }
        if f.parameter.len() != argumente.len() {
            let meldung =
                format!("{wer} wü {} hom, du gibst eam {}.", anzahl(f.parameter.len()), anzahl(argumente.len()));
            self.melde(pos, meldung);
            return f.rueckgabe;
        }
        for (i, (argument, ziel)) in argumente.iter_mut().zip(&f.parameter).enumerate() {
            if !self.anpassen(argument, *ziel) {
                let meldung = format!(
                    "{wer} wü ois {}. Wert {}, du gibst eam {}.",
                    i + 1,
                    self.typ_name(*ziel),
                    self.typ_name(typen[i])
                );
                self.melde(argument.pos, meldung);
            }
        }
        f.rueckgabe
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
        self.sicher.clear();
        self.funktion = Some((f.name.clone(), f.rueckgabe));
        // In einer Methode gibt es SÖWA; es bekommt eine Nummer wie eine Variable.
        self.selbst = self.methoden_klasse.map(|klasse| {
            f.selbst_id = self.naechste_id;
            self.naechste_id += 1;
            (klasse, f.selbst_id)
        });
        for parameter in &mut f.parameter {
            parameter.id = self.neue_variable(&parameter.name, parameter.typ, parameter.pos);
        }
        for a in &mut f.rumpf {
            self.anweisung(a);
        }
        // NIX ODA FEHLA: Wer ohne GIB ZRUCK ans Ende kommt, hat es geschafft.
        let ohne_wert = f.rueckgabe == Typ::Nix || self.typen.wert_von(f.rueckgabe) == Some(Typ::Nix);
        if !ohne_wert && f.rueckgabe != Typ::Fehler && !gibt_sicher_zruck(&f.rumpf) {
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
                self.probier_erlaubt = true;
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
                } else if var_typ != Typ::Fehler && !self.fertig_verlangt(wert) {
                    var_typ = Typ::Fehler;
                }
                // Erst jetzt eintragen: "SOG AU x = x" soll das äußere x meinen.
                *id = self.neue_variable(name, var_typ, pos);
                // Steht gleich ein echter Wert drin, ist die Variable fürs Erste nicht NIX.
                let echter_wert = !matches!(wert_typ, Typ::Vielleicht { .. } | Typ::NixWert | Typ::Fehler);
                if matches!(var_typ, Typ::Vielleicht { .. }) && echter_wert {
                    self.sicher.insert(*id);
                }
            }
            AnweisungArt::Zuweisung { ziel, op, wert } => {
                self.probier_erlaubt = true;
                let wert_typ = self.ausdruck(wert);
                let mut ziel_typ = self.ausdruck(ziel);
                // Zuweisen darf man auch NIX: Hier zählt die Variable, nicht ihr sicherer Wert.
                if op.is_none() && matches!(ziel.art, AusdruckArt::Sicher { .. }) {
                    sicher_auspacken(ziel);
                    ziel_typ = ziel.typ;
                }
                // verzeichnis[schlüssel] = wert: Hinein kommt ein Wert, auch wenn beim
                // Lesen VIELLEICHT herauskommt (der Schlüssel könnte ja fehlen).
                if let AusdruckArt::Stelle { liste, .. } = &ziel.art
                    && matches!(liste.typ, Typ::Verzeichnis(_))
                    && ziel_typ != Typ::Fehler
                {
                    if op.is_some() {
                        let meldung = format!(
                            "Bei an {} geht nur '=': Da Schlissl kunnt jo fehln. Schreib zum Beispü: \
                             v[k] = (v[k] {} 0) + 1 {}",
                            self.wort(Schluessel::Map),
                            self.wort(Schluessel::OrElse),
                            self.wort(Schluessel::Oida)
                        );
                        self.melde(pos, meldung);
                        return;
                    }
                    ziel_typ = self.typen.innen(ziel_typ).unwrap_or(ziel_typ);
                }
                // Danach gilt für die Variable, was hineingekommen ist.
                if op.is_none()
                    && matches!(ziel_typ, Typ::Vielleicht { .. })
                    && let AusdruckArt::Variable { id, .. } = &ziel.art
                {
                    if matches!(wert_typ, Typ::Vielleicht { .. } | Typ::NixWert | Typ::Fehler) {
                        self.sicher.remove(id);
                    } else {
                        self.sicher.insert(*id);
                    }
                }
                if ziel_typ == Typ::Fehler || wert_typ == Typ::Fehler {
                    return;
                }
                if let Some(name) = self.aendert_fang(ziel) {
                    let meldung = format!(
                        "'{name}' kummt von außn: In ana {} ohne Nom host nur a Kopie davon, de kaunst lesn, oba ned ändern. \
                         (Wos in an Objekt oda ana {} steht, auf des '{name}' zeigt, kaunst scho ändern.)",
                        self.wort(Schluessel::Function),
                        self.wort(Schluessel::List)
                    );
                    self.melde(ziel.pos, meldung);
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
                    let wos = match ergebnis {
                        Typ::NixWert => format!("ka {}", self.wort(Schluessel::Null)),
                        _ => self.typ_name(ergebnis).replacen("a ", "ka ", 1),
                    };
                    let meldung = format!(
                        "'{}' is {}, do kaunst {wos} einetuan.",
                        ziel_text(ziel),
                        self.typ_name(ziel_typ)
                    );
                    self.melde(wert.pos, meldung);
                }
            }
            AnweisungArt::Wenn { bedingung, dann, sunst } => {
                self.bedingung(bedingung, Schluessel::If);
                // Was die Bedingung über NIX sagt, gilt im jeweiligen Zweig.
                let (wahr, falsch) = fakten(bedingung);
                let vorher = self.sicher.clone();
                self.sicher.extend(wahr);
                self.block(dann);
                let nach_dann = std::mem::replace(&mut self.sicher, vorher);
                self.sicher.extend(falsch);
                self.block(sunst);
                let nach_sunst = std::mem::take(&mut self.sicher);
                self.sicher = nach_zweigen(nach_dann, verlaesst(dann), nach_sunst, verlaesst(sunst));
            }
            AnweisungArt::Solong { bedingung, rumpf } => {
                // Was im Rumpf neu zugewiesen wird, ist beim nächsten Durchlauf nicht mehr sicher.
                self.vergiss_zugewiesene(rumpf);
                self.bedingung(bedingung, Schluessel::While);
                let (wahr, _) = fakten(bedingung);
                let vorher = self.sicher.clone();
                self.sicher.extend(wahr);
                self.schleifen += 1;
                self.block(rumpf);
                self.schleifen -= 1;
                self.sicher = vorher;
            }
            AnweisungArt::Probier { name, id, wert, dann, fehler_name, fehler_id, sunst } => {
                let typ = self.ausdruck(wert);
                let mut wert_typ = Typ::Fehler;
                if typ != Typ::Fehler {
                    match self.typen.wert_von(typ) {
                        Some(t) => wert_typ = t,
                        None => {
                            let meldung = format!(
                                "Noch {} brauch i wos, des schiefgehn kaun (a {} mit ... {} {}), des do is oba {}.",
                                self.wort(Schluessel::Try),
                                self.wort(Schluessel::Function),
                                self.wort(Schluessel::Or),
                                self.wort(Schluessel::Error),
                                self.typ_name(typ)
                            );
                            self.melde(wert.pos, meldung);
                        }
                    }
                }
                if let (Some(name), Typ::Nix) = (&name, wert_typ) {
                    let meldung = format!("Do kummt ka Wert zruck. Loss '{name} =' weg.");
                    self.melde(pos, meldung);
                    wert_typ = Typ::Fehler;
                }
                let vorher = self.sicher.clone();
                // Hat es geklappt: der Wert, sicher da.
                self.bereiche.push(HashMap::new());
                if let Some(name) = name {
                    *id = self.neue_variable(name, wert_typ, pos);
                }
                self.block(dann);
                self.bereiche.pop();
                let nach_dann = std::mem::replace(&mut self.sicher, vorher);
                // Ist es schiefgegangen: der Fehler.
                self.bereiche.push(HashMap::new());
                if let Some(fehler_name) = fehler_name {
                    *fehler_id = self.neue_variable(fehler_name, Typ::Fehla, pos);
                }
                self.block(sunst);
                self.bereiche.pop();
                let nach_sunst = std::mem::take(&mut self.sicher);
                self.sicher = nach_zweigen(nach_dann, verlaesst(dann), nach_sunst, verlaesst(sunst));
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
                self.vergiss_zugewiesene(rumpf);
                let vorher = self.sicher.clone();
                self.bereiche.push(HashMap::new());
                *id = self.neue_variable(name, Typ::Zoi, pos);
                self.schleifen += 1;
                self.block(rumpf);
                self.schleifen -= 1;
                self.bereiche.pop();
                self.sicher = vorher;
            }
            AnweisungArt::FiaJede { name, id, liste, rumpf } => {
                let listen_typ = self.ausdruck(liste);
                let mut element = Typ::Fehler;
                if listen_typ != Typ::Fehler && self.fertig_verlangt(liste) {
                    // Bei einem Verzeichnis geht die Schleife über die Schlüssel.
                    let schluessel = self.typen.verzeichnis(listen_typ).map(|(k, _)| k);
                    if let Some(k) = schluessel {
                        // Die IR-Erzeugung holt die Schlüssel als Liste; den Typ dafür muss es geben.
                        self.typen.liste_von(k);
                    }
                    match self.typen.element(listen_typ).or(schluessel) {
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
                self.vergiss_zugewiesene(rumpf);
                let vorher = self.sicher.clone();
                self.bereiche.push(HashMap::new());
                *id = self.neue_variable(name, element, pos);
                self.schleifen += 1;
                self.block(rumpf);
                self.schleifen -= 1;
                self.bereiche.pop();
                self.sicher = vorher;
            }
            AnweisungArt::HauOb => self.nur_in_schleife(Schluessel::Break, pos),
            AnweisungArt::Weida => self.nur_in_schleife(Schluessel::Continue, pos),
            AnweisungArt::GibZruck { wert } => {
                self.probier_erlaubt = wert.is_some();
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
                    // NIX ODA FEHLA: ohne Wert heißt "hat geklappt".
                    (None, _) if self.typen.wert_von(rueckgabe) == Some(Typ::Nix) => {}
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
                self.probier_erlaubt = true;
                let typ = self.ausdruck(ausdruck);
                let aufruf = matches!(
                    ausdruck.art,
                    AusdruckArt::Aufruf { .. }
                        | AusdruckArt::Eingebaut { .. }
                        | AusdruckArt::Methode { .. }
                        | AusdruckArt::ElternAufruf { .. }
                        | AusdruckArt::Probier { .. }
                        | AusdruckArt::WertAufruf { .. }
                );
                if matches!(typ, Typ::Ergebnis(_)) {
                    // Einen Fehler darf man nicht einfach übersehen.
                    let meldung = format!(
                        "Do kaun wos schiefgehn, des derfst ned afoch wegschmeißn. \
                         Schreib {p} davua (daun geht da {} weida) oda fang eam mit {p} ... {} auf.",
                        self.wort(Schluessel::Error),
                        self.wort(Schluessel::Catch),
                        p = self.wort(Schluessel::Try),
                    );
                    self.melde(ausdruck.pos, meldung);
                } else if !aufruf {
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

    /// Vor einer Schleife: Variablen, denen im Rumpf etwas zugewiesen wird,
    /// gelten nicht mehr als sicher. Beim zweiten Durchlauf könnte NIX drinstehen.
    fn vergiss_zugewiesene(&mut self, rumpf: &[Anweisung]) {
        let mut namen = Vec::new();
        sammle_zugewiesene(rumpf, &mut namen);
        for name in namen {
            if let Some(id) = self.finde_variable(&name).map(|v| v.id) {
                self.sicher.remove(&id);
            }
        }
    }

    /// Der Hinweis, wenn jemand etwas verwenden will, das NIX oder ein Fehler sein könnte.
    fn nix_hinweis(&self, ausdruck: &Ausdruck, typ: Typ) -> Option<String> {
        let nix = self.wort(Schluessel::Null);
        match typ {
            Typ::Vielleicht { .. } => Some(match &ausdruck.art {
                AusdruckArt::Variable { name, .. } => format!(
                    "'{name}' kunnt {nix} sei. Schau z'erst noch: {} {name} != {nix} {} ... (oda nimm {}).",
                    self.wort(Schluessel::If),
                    self.wort(Schluessel::Then),
                    self.wort(Schluessel::OrElse)
                ),
                _ => format!(
                    "Des kunnt {nix} sei. Tua's z'erst in a Variable und schau noch: \
                     {} x = ... {}  {} x != {nix} {} ...",
                    self.wort(Schluessel::Declare),
                    self.wort(Schluessel::Oida),
                    self.wort(Schluessel::If),
                    self.wort(Schluessel::Then)
                ),
            }),
            Typ::Ergebnis(_) => Some(format!(
                "Do kunnt a {} drin sei. Nimm z'erst {} oda {}.",
                self.wort(Schluessel::Error),
                self.wort(Schluessel::Try),
                self.wort(Schluessel::OrElse)
            )),
            _ => None,
        }
    }

    /// `x == NIX` und `x != NIX`: Aus dem Vergleich wird die Frage, ob etwas da ist.
    fn ist_nix(&mut self, ausdruck: &mut Ausdruck) -> Typ {
        let pos = ausdruck.pos;
        let AusdruckArt::Binaer { op, links, rechts } = &mut ausdruck.art else { unreachable!() };
        let ist = *op == BinaerOp::Gleich;
        let anderes = if matches!(links.art, AusdruckArt::Nix) { rechts } else { links };
        let mut innen = std::mem::replace(&mut **anderes, Ausdruck::neu(AusdruckArt::Nix, pos));
        let mut typ = self.ausdruck(&mut innen);
        // Nachschauen darf man auch, wo es schon feststeht.
        if matches!(innen.art, AusdruckArt::Sicher { .. }) {
            sicher_auspacken(&mut innen);
            typ = innen.typ;
        }
        let ergebnis = match typ {
            Typ::Fehler => Typ::Fehler,
            Typ::Vielleicht { .. } => Typ::Jona,
            Typ::NixWert => {
                let meldung = format!("{nix} mit {nix} vergleichn bringt nix.", nix = self.wort(Schluessel::Null));
                self.melde(pos, meldung);
                Typ::Fehler
            }
            _ => {
                let meldung = format!(
                    "{} kaun ned {nix} sei, do gibt's nix zum Nochschaun. Des geht nur bei {} ...",
                    self.typ_name(typ).replacen("a ", "A ", 1),
                    self.wort(Schluessel::Maybe),
                    nix = self.wort(Schluessel::Null)
                );
                self.melde(innen.pos, meldung);
                Typ::Fehler
            }
        };
        ausdruck.art = AusdruckArt::IstNix { innen: Box::new(innen), ist };
        ausdruck.typ = ergebnis;
        ergebnis
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
        // Wo etwas fehlen darf, passt NIX und jeder Wert, der dort sein darf.
        if let Some(innen) = self.typen.innen(ziel) {
            if ausdruck.typ == Typ::NixWert {
                ausdruck.typ = ziel;
                return true;
            }
            if let Some(von) = self.typen.innen(ausdruck.typ) {
                // VIELLEICHT Gegner passt, wo VIELLEICHT Wesen verlangt ist.
                if let (Typ::Klasse(a), Typ::Klasse(b)) = (von, innen)
                    && self.typen.erbt_von(a, b)
                {
                    einpacken(ausdruck, ziel);
                    return true;
                }
                return false;
            }
            if matches!(ausdruck.typ, Typ::Nix | Typ::Ergebnis(_)) || !self.anpassen(ausdruck, innen) {
                return false;
            }
            einpacken(ausdruck, ziel);
            return true;
        }
        // Wo ein Wert oder ein Fehler verlangt ist, passt beides.
        if let Some(wert) = self.typen.wert_von(ziel) {
            if ausdruck.typ == Typ::Fehla {
                einpacken(ausdruck, ziel);
                return true;
            }
            if matches!(ausdruck.typ, Typ::Nix | Typ::Ergebnis(_)) || wert == Typ::Nix || !self.anpassen(ausdruck, wert) {
                return false;
            }
            einpacken(ausdruck, ziel);
            return true;
        }
        // Ein Gegner ist auch ein Wesen: Wo die Elternklasse verlangt ist, passt jedes Kind.
        if let (Typ::Klasse(von), Typ::Klasse(nach)) = (ausdruck.typ, ziel)
            && self.typen.erbt_von(von, nach)
        {
            // Im Baum steht danach überall der Typ, als der der Wert verwendet wird.
            einpacken(ausdruck, ziel);
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
        // Gilt nur für diesen einen Ausdruck, nicht für die darin.
        let probier_erlaubt = std::mem::take(&mut self.probier_erlaubt);
        if let AusdruckArt::Binaer { op: BinaerOp::Gleich | BinaerOp::Ungleich, links, rechts } = &ausdruck.art
            && (matches!(links.art, AusdruckArt::Nix) || matches!(rechts.art, AusdruckArt::Nix))
        {
            return self.ist_nix(ausdruck);
        }
        // Heißt die "Funktion" wie eine Struktur oder Klasse, wird ein neuer Wert gebaut.
        if let AusdruckArt::Aufruf { name, .. } = &ausdruck.art {
            if let Some(&nummer) = self.struktur_nummern.get(name) {
                return self.bau(ausdruck, nummer, None);
            }
            if let Some(&nummer) = self.klassen_nummern.get(name) {
                return self.neu(ausdruck, nummer, None);
            }
            if self.klassen_vorlagen.contains_key(name) || self.struktur_vorlagen.contains_key(name) {
                return self.vorlage_bauen(ausdruck);
            }
            // Heißt so eine Variable, in der eine Funktion steckt, wird die aufgerufen.
            let name = name.clone();
            let variable = self.hol_variable(&name, self.lambdas.len());
            let ist_funktion = variable.is_some_and(|(_, typ)| {
                matches!(typ, Typ::Funktion(_)) || matches!(self.typen.innen(typ), Some(Typ::Funktion(_)))
            });
            let funktion_mit_nom = self.signaturen.contains_key(&name) || self.funktions_vorlagen.contains_key(&name);
            if ist_funktion || (variable.is_some() && !funktion_mit_nom) {
                let AusdruckArt::Aufruf { name, argumente } = std::mem::replace(&mut ausdruck.art, AusdruckArt::Nix)
                else {
                    unreachable!()
                };
                let funktion = Box::new(Ausdruck::neu(AusdruckArt::Variable { name, id: 0 }, pos));
                ausdruck.art = AusdruckArt::WertAufruf { funktion, argumente };
            }
        }
        if matches!(ausdruck.art, AusdruckArt::Erzeug { .. }) {
            return self.erzeug(ausdruck);
        }
        // Was erst nach dem Blick in den Ausdruck umgebaut werden kann.
        let mut ist_funktions_wert = false;
        let mut feld_aufruf: Option<(usize, Typ)> = None;
        let typ = match &mut ausdruck.art {
            AusdruckArt::Zoi(_) => Typ::Zoi,
            AusdruckArt::Kommazoi(_) => Typ::Kommazoi,
            AusdruckArt::Jona(_) => Typ::Jona,
            AusdruckArt::Text(_) => Typ::Kettn,
            AusdruckArt::Variable { name, id } => {
                let name = name.clone();
                match self.hol_variable(&name, self.lambdas.len()) {
                    Some((var_id, typ)) => {
                        *id = var_id;
                        typ
                    }
                    // Keine Variable, aber eine Funktion mit dem Namen: die Funktion als Wert.
                    None if self.signaturen.contains_key(&name) => {
                        ist_funktions_wert = true;
                        let signatur = &self.signaturen[&name];
                        let (parameter, rueckgabe) = (signatur.parameter.clone(), signatur.rueckgabe);
                        if parameter.contains(&Typ::Fehler) || rueckgabe == Typ::Fehler {
                            Typ::Fehler
                        } else {
                            self.typen.funktion_von(parameter, rueckgabe)
                        }
                    }
                    None if self.funktions_vorlagen.contains_key(&name) => {
                        let meldung = format!(
                            "'{name}' is a Vorlog: Ois Wert geht de ned, weu ned feststeht, mit wöchn Typen. \
                             Pack den Aufruf in a {} ohne Nom.",
                            self.wort(Schluessel::Function)
                        );
                        self.melde(pos, meldung);
                        Typ::Fehler
                    }
                    None => {
                        self.unbekannte_variable(&name, pos);
                        Typ::Fehler
                    }
                }
            }
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
                // Rechts von UND gilt, was links nachgeschaut wurde (x != NIX UND x.lebt());
                // rechts von ODA das Gegenteil (x == NIX ODA x.lebt()).
                let (wahr, falsch) = fakten(links);
                let fakten = match op {
                    BinaerOp::Und => wahr,
                    BinaerOp::Oda => falsch,
                    _ => Vec::new(),
                };
                let dazu: Vec<usize> = fakten.into_iter().filter(|id| self.sicher.insert(*id)).collect();
                let r = self.ausdruck(rechts);
                for id in dazu {
                    self.sicher.remove(&id);
                }
                self.binaer(op, links, rechts, l, r, pos)
            }
            AusdruckArt::Aufruf { name, argumente } => {
                let typen: Vec<Typ> = argumente.iter_mut().map(|a| self.ausdruck(a)).collect();
                // Eine Vorlage: Aus den Werten ergibt sich, welche Fassung gemeint ist.
                if let Some(vorlage) = self.funktions_vorlagen.get(name.as_str()) {
                    let offen = vorlage.typ_parameter.clone();
                    let angaben: Vec<TypAngabe> = vorlage.parameter.iter().map(|p| p.angabe.clone()).collect();
                    let vorlagen_name = name.clone();
                    if typen.contains(&Typ::Fehler) {
                        Typ::Fehler
                    } else if angaben.len() != typen.len() {
                        let meldung = format!(
                            "'{vorlagen_name}' wü {} hom, du gibst eam {}.",
                            anzahl(angaben.len()),
                            anzahl(typen.len())
                        );
                        self.melde(pos, meldung);
                        Typ::Fehler
                    } else {
                        match self.typen_ablesen(&vorlagen_name, &offen, &angaben, &typen, pos) {
                            Some(gewaehlt) => {
                                let fassung = self.funktions_fassung(&vorlagen_name, gewaehlt);
                                *name = fassung.clone();
                                self.aufruf(&fassung, argumente, &typen, pos)
                            }
                            None => Typ::Fehler,
                        }
                    }
                } else {
                    let name = name.clone();
                    self.aufruf(&name, argumente, &typen, pos)
                }
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
                            // FEHLA("..."): Ein Fehler ist ein Text mit der Meldung.
                            | (Typ::Kettn, Typ::Fehla)
                            | (Typ::Fehla, Typ::Kettn)
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
                } else if let Some(klasse) = self.typen.klasse(objekt_typ).cloned() {
                    match klasse.felder.iter().position(|feld| feld.name == *name) {
                        Some(stelle) => {
                            *nummer = stelle;
                            klasse.felder[stelle].typ
                        }
                        None => {
                            let meldung = if klasse.methoden.iter().any(|m| m.name == *name) {
                                format!("'{name}' is in '{}' a Methode: Schreib {name}(...).", klasse.name)
                            } else if klasse.felder.is_empty() {
                                format!("A Föd '{name}' gibt's in '{}' ned, de hot gor kane Föda.", klasse.name)
                            } else {
                                let namen: Vec<&str> = klasse.felder.iter().map(|f| f.name.as_str()).collect();
                                format!("A Föd '{name}' gibt's in '{}' ned. Es gibt: {}.", klasse.name, namen.join(", "))
                            };
                            self.melde(pos, meldung);
                            Typ::Fehler
                        }
                    }
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
                } else if objekt_typ == Typ::Fehla {
                    // Ein Fehler hat genau ein Feld: seine Meldung.
                    if name == "meldung" {
                        *nummer = 0;
                        Typ::Kettn
                    } else {
                        let meldung = format!(
                            "A {} hot nur des Föd 'meldung', a Föd '{name}' gibt's do ned.",
                            self.wort(Schluessel::Error)
                        );
                        self.melde(pos, meldung);
                        Typ::Fehler
                    }
                } else {
                    let meldung = self.nix_hinweis(objekt, objekt_typ).unwrap_or_else(|| {
                        format!("{} hot kane Föda.", self.typ_name(objekt_typ).replacen("a ", "A ", 1))
                    });
                    self.melde(pos, meldung);
                    Typ::Fehler
                }
            }
            AusdruckArt::Stelle { liste, index } => {
                let listen_typ = self.ausdruck(liste);
                let index_typ = self.ausdruck(index);
                if let Some((k, w)) = self.typen.verzeichnis(listen_typ) {
                    // verzeichnis[schlüssel]: der Wert, oder NIX, wenn es den Schlüssel nicht gibt.
                    if index_typ != Typ::Fehler && !self.anpassen(index, k) {
                        let meldung = format!(
                            "Da Schlissl von dem {} is {}, des do is oba {}.",
                            self.wort(Schluessel::Map),
                            self.typ_name(k),
                            self.typ_name(index_typ)
                        );
                        self.melde(index.pos, meldung);
                    }
                    self.typen.vielleicht_von(w)
                } else if index_typ != Typ::Zoi && index_typ != Typ::Fehler {
                    let meldung = format!(
                        "De Stö in ana {} muass {} sei, des do is oba {}.",
                        self.wort(Schluessel::List),
                        self.typ_name(Typ::Zoi),
                        self.typ_name(index_typ)
                    );
                    self.melde(index.pos, meldung);
                    Typ::Fehler
                } else if listen_typ == Typ::Fehler || !self.fertig_verlangt(liste) {
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
            AusdruckArt::Neu { klasse, .. } => Typ::Klasse(*klasse),
            AusdruckArt::Selbst { id } => match self.hol_selbst(self.lambdas.len()) {
                Some((klasse, selbst_id)) => {
                    *id = selbst_id;
                    Typ::Klasse(klasse)
                }
                None => {
                    let meldung = format!("{} gibt's nur in ana Methode.", self.wort(Schluessel::This));
                    self.melde(pos, meldung);
                    Typ::Fehler
                }
            },
            AusdruckArt::Methode { objekt, name, argumente, nummer } => {
                let mut objekt_typ = self.ausdruck(objekt);
                let typen: Vec<Typ> = argumente.iter_mut().map(|a| self.ausdruck(a)).collect();
                let name = name.clone();
                if let Some(meldung) = self.nix_hinweis(objekt, objekt_typ) {
                    self.melde(pos, meldung);
                    objekt_typ = Typ::Fehler;
                }
                // Keine Methode, aber ein Feld, in dem eine Funktion steckt: die wird aufgerufen.
                if let Some(klasse) = self.typen.klasse(objekt_typ)
                    && !klasse.methoden.iter().any(|m| m.name == name)
                    && let Some(stelle) = klasse.felder.iter().position(|f| f.name == name)
                    && matches!(klasse.felder[stelle].typ, Typ::Funktion(_))
                {
                    feld_aufruf = Some((stelle, klasse.felder[stelle].typ));
                }
                if let Some(struktur) = self.typen.struktur(objekt_typ)
                    && let Some(stelle) = struktur.felder.iter().position(|(feld, _)| *feld == name)
                    && matches!(struktur.felder[stelle].1, Typ::Funktion(_))
                {
                    feld_aufruf = Some((stelle, struktur.felder[stelle].1));
                }
                if feld_aufruf.is_some() {
                    objekt_typ = Typ::Fehler;
                }
                match self.methode(objekt_typ, &name, argumente, &typen, pos) {
                    Some((stelle, rueckgabe)) => {
                        *nummer = stelle;
                        rueckgabe
                    }
                    None => Typ::Fehler,
                }
            }
            AusdruckArt::ElternAufruf { name, argumente, klasse } => {
                let name = name.clone();
                match self.eltern_aufruf(&name, argumente, pos) {
                    Some((in_klasse, rueckgabe)) => {
                        *klasse = in_klasse;
                        rueckgabe
                    }
                    None => Typ::Fehler,
                }
            }
            AusdruckArt::Nix => Typ::NixWert,
            // Die drei entstehen erst hier; ein zweites Mal kommt niemand vorbei.
            AusdruckArt::IstNix { .. } => Typ::Jona,
            AusdruckArt::Sicher { .. } | AusdruckArt::FunktionsWert { .. } => ausdruck.typ,
            AusdruckArt::Lambda { funktion, kurz, fang, nummer } => {
                let kurz = *kurz;
                let (typ, gefangen) = self.lambda(funktion, kurz, pos);
                *fang = gefangen;
                *nummer = self.naechste_lambda;
                self.naechste_lambda += 1;
                typ
            }
            AusdruckArt::WertAufruf { funktion, argumente } => {
                let f_typ = self.ausdruck(funktion);
                let typen: Vec<Typ> = argumente.iter_mut().map(|a| self.ausdruck(a)).collect();
                self.wert_aufruf(funktion, f_typ, argumente, &typen, pos)
            }
            AusdruckArt::Erzeug { .. } => unreachable!("oben erledigt"),
            AusdruckArt::OdaHoid { links, rechts } => {
                let mut l = self.ausdruck(links);
                if matches!(links.art, AusdruckArt::Sicher { .. }) {
                    sicher_auspacken(links);
                    l = links.typ;
                }
                let r = self.ausdruck(rechts);
                let ziel = match l {
                    Typ::Fehler => Typ::Fehler,
                    Typ::Vielleicht { .. } => self.typen.innen(l).unwrap_or(Typ::Fehler),
                    Typ::Ergebnis(_) if self.typen.wert_von(l) != Some(Typ::Nix) => {
                        self.typen.wert_von(l).unwrap_or(Typ::Fehler)
                    }
                    Typ::Ergebnis(_) => {
                        let meldung = format!(
                            "Do kummt ka Wert zruck, fia den ma an Ersotz bräucht. Nimm {} ... {}.",
                            self.wort(Schluessel::Try),
                            self.wort(Schluessel::Catch)
                        );
                        self.melde(links.pos, meldung);
                        Typ::Fehler
                    }
                    _ => {
                        let meldung = format!(
                            "Vua {} ghört wos, des {} oda a {} sei kunnt, des do is oba {}.",
                            self.wort(Schluessel::OrElse),
                            self.wort(Schluessel::Null),
                            self.wort(Schluessel::Error),
                            self.typ_name(l)
                        );
                        self.melde(links.pos, meldung);
                        Typ::Fehler
                    }
                };
                if ziel == Typ::Fehler || r == Typ::Fehler {
                    Typ::Fehler
                } else if !self.anpassen(rechts, ziel) {
                    let meldung = format!(
                        "Noch {} brauch i {}, des do is oba {}.",
                        self.wort(Schluessel::OrElse),
                        self.typ_name(ziel),
                        self.typ_name(r)
                    );
                    self.melde(rechts.pos, meldung);
                    Typ::Fehler
                } else {
                    ziel
                }
            }
            AusdruckArt::Probier { innen } => {
                let typ = self.ausdruck(innen);
                let probier = self.wort(Schluessel::Try).to_string();
                if !probier_erlaubt {
                    let meldung = format!(
                        "{probier} muass gaunz vuan stehn, zum Beispü: {} x = {probier} f() {}",
                        self.wort(Schluessel::Declare),
                        self.wort(Schluessel::Oida)
                    );
                    self.melde(pos, meldung);
                    Typ::Fehler
                } else if typ == Typ::Fehler {
                    Typ::Fehler
                } else if let Some(wert) = self.typen.wert_von(typ) {
                    let gibt_weiter = self.funktion.as_ref().is_some_and(|(_, r)| matches!(r, Typ::Ergebnis(_)));
                    if gibt_weiter {
                        wert
                    } else {
                        let meldung = format!(
                            "{probier} gibt an {f} weida. Des geht nur in ana {}, de söwa an {f} zruckgebn kaun \
                             ({} ... {} {f}). Sunst fang eam auf: {probier} ... {} ... {}",
                            self.wort(Schluessel::Function),
                            self.wort(Schluessel::Returns),
                            self.wort(Schluessel::Or),
                            self.wort(Schluessel::Catch),
                            self.wort(Schluessel::End),
                            f = self.wort(Schluessel::Error),
                        );
                        self.melde(pos, meldung);
                        Typ::Fehler
                    }
                } else {
                    let meldung = format!(
                        "{probier} braucht wos, des schiefgehn kaun (a {} mit ... {} {}), des do is oba {}.",
                        self.wort(Schluessel::Function),
                        self.wort(Schluessel::Or),
                        self.wort(Schluessel::Error),
                        self.typ_name(typ)
                    );
                    self.melde(innen.pos, meldung);
                    Typ::Fehler
                }
            }
        };
        ausdruck.typ = typ;
        if ist_funktions_wert {
            let AusdruckArt::Variable { name, .. } = std::mem::replace(&mut ausdruck.art, AusdruckArt::Nix) else {
                unreachable!()
            };
            ausdruck.art = AusdruckArt::FunktionsWert { name };
            return typ;
        }
        if let Some((stelle, feld_typ)) = feld_aufruf {
            // objekt.feld(...): erst das Feld holen, dann die Funktion darin aufrufen.
            let AusdruckArt::Methode { objekt, name, mut argumente, .. } =
                std::mem::replace(&mut ausdruck.art, AusdruckArt::Nix)
            else {
                unreachable!()
            };
            let typen: Vec<Typ> = argumente.iter().map(|a| a.typ).collect();
            let feld = Ausdruck { art: AusdruckArt::Feld { objekt, name, nummer: stelle }, pos, typ: feld_typ };
            let typ = self.wert_aufruf(&feld, feld_typ, &mut argumente, &typen, pos);
            ausdruck.art = AusdruckArt::WertAufruf { funktion: Box::new(feld), argumente };
            ausdruck.typ = typ;
            return typ;
        }
        // Eine Variable, von der hier feststeht, dass sie nicht NIX ist, zählt als ihr Wert.
        if let AusdruckArt::Variable { id, .. } = &ausdruck.art
            && self.sicher.contains(id)
            && let Some(innen) = self.typen.innen(typ)
        {
            let alt = std::mem::replace(ausdruck, Ausdruck::neu(AusdruckArt::Nix, pos));
            *ausdruck = Ausdruck { art: AusdruckArt::Sicher { innen: Box::new(alt) }, pos, typ: innen };
            return innen;
        }
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
        // Steht irgendwo NIX oder etwas, das NIX sein kann, darf in der ganzen Liste etwas fehlen.
        let fehlt_was = typen.iter().any(|t| matches!(t, Typ::NixWert | Typ::Vielleicht { .. }));
        if fehlt_was && !matches!(gemeinsam, Typ::Vielleicht { .. }) {
            gemeinsam = self.typen.vielleicht_von(gemeinsam);
        }
        // Objekte verschiedener Klassen: Es gilt die nächste Klasse, von der alle erben.
        for typ in &typen {
            if let (Typ::Klasse(a), Typ::Klasse(b)) = (gemeinsam, *typ) {
                let mut vorfahr = Some(a);
                while let Some(v) = vorfahr {
                    if self.typen.erbt_von(b, v) {
                        gemeinsam = Typ::Klasse(v);
                        break;
                    }
                    vorfahr = self.typen.klassen[v as usize].eltern;
                }
            }
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
    fn bau(&mut self, ausdruck: &mut Ausdruck, nummer: u32, geprueft: Option<Vec<Typ>>) -> Typ {
        let AusdruckArt::Aufruf { name, argumente } = &mut ausdruck.art else { unreachable!() };
        let name = name.clone();
        let mut argumente = std::mem::take(argumente);
        let typen: Vec<Typ> = match geprueft {
            Some(typen) => typen,
            None => argumente.iter_mut().map(|a| self.ausdruck(a)).collect(),
        };
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
        // Zwei Objekte vergleichen heißt fragen: Ist es dasselbe Objekt?
        // Das geht auch, wenn eines davon NIX sein kann.
        let kern = |t: Typ| self.typen.innen(t).unwrap_or(t);
        if let (Typ::Klasse(a), Typ::Klasse(b)) = (kern(l), kern(r))
            && matches!(op, BinaerOp::Gleich | BinaerOp::Ungleich)
            && (self.typen.erbt_von(a, b) || self.typen.erbt_von(b, a))
        {
            return Typ::Jona;
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
        if matches!(l, Typ::Vielleicht { .. }) || matches!(r, Typ::Vielleicht { .. }) {
            meldung.push_str(&format!(
                " Wos {nix} sei kunnt, muasst z'erst nochschaun ({} x != {nix} {} ...) oda mit {} an Ersotz augebn.",
                self.wort(Schluessel::If),
                self.wort(Schluessel::Then),
                self.wort(Schluessel::OrElse),
                nix = self.wort(Schluessel::Null)
            ));
        }
        if matches!(l, Typ::Ergebnis(_)) || matches!(r, Typ::Ergebnis(_)) {
            meldung.push_str(&format!(
                " Do kunnt a {} drin sei: Nimm z'erst {} oda {}.",
                self.wort(Schluessel::Error),
                self.wort(Schluessel::Try),
                self.wort(Schluessel::OrElse)
            ));
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
            Eingebaut::Laenge if typen.len() == 1 && matches!(typen[0], Typ::Verzeichnis(_)) => return Typ::Zoi,
            Eingebaut::Laenge if typen.len() == 1 && matches!(typen[0], Typ::Liste(_) | Typ::LeereListe) => {
                self.fertig_verlangt(&argumente[0]);
                return Typ::Zoi;
            }
            // ZOI AUS(text), KOMMAZOI AUS(text): die Zahl oder ein Fehler.
            Eingebaut::ZoiAus | Eingebaut::KommazoiAus => {
                if typen.len() != 1 {
                    let meldung = format!("'{name}' wü an Wert hom, du gibst eam {}.", anzahl(typen.len()));
                    self.melde(pos, meldung);
                } else if typen[0] != Typ::Kettn {
                    let meldung = format!(
                        "'{name}' wü ois 1. Wert {}, du gibst eam {}.",
                        self.typ_name(Typ::Kettn),
                        self.typ_name(typen[0])
                    );
                    self.melde(argumente[0].pos, meldung);
                }
                let wert = if funktion == Eingebaut::ZoiAus { Typ::Zoi } else { Typ::Kommazoi };
                return self.typen.ergebnis_von(wert);
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
        // HAU AUSSE(verzeichnis, schlüssel): den Eintrag entfernen (fehlt er, passiert nichts).
        if funktion == Eingebaut::Raus
            && let Some((k, _)) = self.typen.verzeichnis(typen[0])
        {
            if !self.anpassen(&mut argumente[1], k) {
                let meldung = format!(
                    "Da Schlissl von dem {} is {}, des do is oba {}.",
                    self.wort(Schluessel::Map),
                    self.typ_name(k),
                    self.typ_name(typen[1])
                );
                self.melde(argumente[1].pos, meldung);
            }
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
        // Das Feld eines Objekts kann man immer ändern, egal woher das Objekt kommt.
        AusdruckArt::Feld { objekt, .. } => matches!(objekt.typ, Typ::Klasse(_)) || ist_ziel(objekt),
        AusdruckArt::Sicher { innen } => ist_ziel(innen),
        _ => false,
    }
}

/// Wie das Ziel einer Zuweisung in einer Meldung heißt: `x`, `p.x`, `liste[...]`.
fn ziel_text(ausdruck: &Ausdruck) -> String {
    match &ausdruck.art {
        AusdruckArt::Variable { name, .. } => name.clone(),
        AusdruckArt::Feld { objekt, name, .. } => format!("{}.{name}", ziel_text(objekt)),
        AusdruckArt::Stelle { liste, .. } => format!("{}[...]", ziel_text(liste)),
        AusdruckArt::Selbst { .. } => "SÖWA".to_string(),
        AusdruckArt::Sicher { innen } => ziel_text(innen),
        _ => "des".to_string(),
    }
}

/// Packt einen Ausdruck in eine Umwandlung ein.
fn einpacken(ausdruck: &mut Ausdruck, ziel: Typ) {
    let pos = ausdruck.pos;
    let alt = std::mem::replace(ausdruck, Ausdruck::neu(AusdruckArt::Zoi(0), pos));
    *ausdruck = Ausdruck { art: AusdruckArt::Umwandlung { ziel, innen: Box::new(alt) }, pos, typ: ziel };
}

/// Nimmt die Hülle [`AusdruckArt::Sicher`] wieder ab: Übrig bleibt die Variable mit ihrem VIELLEICHT-Typ.
fn sicher_auspacken(ausdruck: &mut Ausdruck) {
    let pos = ausdruck.pos;
    if let AusdruckArt::Sicher { innen } = std::mem::replace(&mut ausdruck.art, AusdruckArt::Nix) {
        *ausdruck = *innen;
    } else {
        unreachable!("an {pos:?} steht keine sichere Variable");
    }
}

/// Was eine (schon geprüfte) Bedingung über NIX sagt: die Variablen, die
/// sicher da sind, wenn sie stimmt, und die, die sicher da sind, wenn nicht.
fn fakten(bedingung: &Ausdruck) -> (Vec<usize>, Vec<usize>) {
    match &bedingung.art {
        AusdruckArt::IstNix { innen, ist } => match &innen.art {
            AusdruckArt::Variable { id, .. } if *ist => (Vec::new(), vec![*id]),
            AusdruckArt::Variable { id, .. } => (vec![*id], Vec::new()),
            _ => (Vec::new(), Vec::new()),
        },
        // a UND b stimmt: beide stimmen. Stimmt es nicht, weiß man nichts.
        AusdruckArt::Binaer { op: BinaerOp::Und, links, rechts } => {
            let (mut wahr, _) = fakten(links);
            wahr.extend(fakten(rechts).0);
            (wahr, Vec::new())
        }
        // a ODA b stimmt nicht: beide stimmen nicht.
        AusdruckArt::Binaer { op: BinaerOp::Oda, links, rechts } => {
            let (_, mut falsch) = fakten(links);
            falsch.extend(fakten(rechts).1);
            (Vec::new(), falsch)
        }
        AusdruckArt::Unaer { op: UnaerOp::Ned, innen } => {
            let (wahr, falsch) = fakten(innen);
            (falsch, wahr)
        }
        _ => (Vec::new(), Vec::new()),
    }
}

/// Was nach einer Verzweigung noch sicher ist: was in beiden Zweigen gilt.
/// Ein Zweig, aus dem es nicht weitergeht (GIB ZRUCK, HAU OB, WEIDA), zählt nicht mit.
fn nach_zweigen(
    nach_dann: HashSet<usize>,
    dann_weg: bool,
    nach_sunst: HashSet<usize>,
    sunst_weg: bool,
) -> HashSet<usize> {
    match (dann_weg, sunst_weg) {
        (true, false) => nach_sunst,
        (false, true) => nach_dann,
        _ => nach_dann.intersection(&nach_sunst).copied().collect(),
    }
}

/// Geht es nach diesem Block sicher nicht mit der nächsten Anweisung weiter?
fn verlaesst(block: &[Anweisung]) -> bool {
    block.iter().any(|a| match &a.art {
        AnweisungArt::GibZruck { .. } | AnweisungArt::HauOb | AnweisungArt::Weida => true,
        AnweisungArt::Wenn { dann, sunst, .. } | AnweisungArt::Probier { dann, sunst, .. } => {
            verlaesst(dann) && verlaesst(sunst)
        }
        _ => false,
    })
}

/// Die Namen der Variablen, denen in diesem Block (auch weiter innen) etwas zugewiesen wird.
fn sammle_zugewiesene(block: &[Anweisung], namen: &mut Vec<String>) {
    for a in block {
        match &a.art {
            AnweisungArt::Zuweisung { ziel, op: None, .. } => {
                if let AusdruckArt::Variable { name, .. } = &ziel.art {
                    namen.push(name.clone());
                }
            }
            AnweisungArt::Wenn { dann, sunst, .. } | AnweisungArt::Probier { dann, sunst, .. } => {
                sammle_zugewiesene(dann, namen);
                sammle_zugewiesene(sunst, namen);
            }
            AnweisungArt::Solong { rumpf, .. }
            | AnweisungArt::Fia { rumpf, .. }
            | AnweisungArt::FiaJede { rumpf, .. } => sammle_zugewiesene(rumpf, namen),
            _ => {}
        }
    }
}

/// Hört dieser Block auf jedem Weg mit GIB ZRUCK auf?
fn gibt_sicher_zruck(block: &[Anweisung]) -> bool {
    block.iter().any(|a| match &a.art {
        AnweisungArt::GibZruck { .. } => true,
        AnweisungArt::Wenn { dann, sunst, .. } | AnweisungArt::Probier { dann, sunst, .. } => {
            gibt_sicher_zruck(dann) && gibt_sicher_zruck(sunst)
        }
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

    /// Für Tests: der Typ "Liste von diesem Typ", wenn es ihn im Programm gibt.
    trait AlsListe {
        fn pipe_liste(self, typen: &Typen) -> Typ;
    }

    impl AlsListe for Typ {
        fn pipe_liste(self, typen: &Typen) -> Typ {
            let nummer = typen.listen.iter().position(|t| *t == self).expect("so eine Liste kommt im Programm vor");
            Typ::Liste(nummer as u32)
        }
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
        assert_eq!(f.meldung, "An Typ 'Gibtsned' kenn i ned. Gibt's a STRUKTUR oda a KLASSE mit dem Nom?");
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

    const WESEN: &str = "KLASSE Wesen\n name KETTN\n lebn ZOI = 100\n \
        FUNKTIAUN sog() GIBT ZRUCK KETTN\n  GIB ZRUCK SÖWA.name OIDA\n PASST SCHO\n \
        FUNKTIAUN lebt() GIBT ZRUCK JONA\n  GIB ZRUCK SÖWA.lebn > 0 OIDA\n PASST SCHO\nPASST SCHO\n\
        KLASSE Gegner ERBT VON Wesen\n staerke ZOI\n \
        FUNKTIAUN sog() GIBT ZRUCK KETTN\n  GIB ZRUCK DE OIDN.sog() + \"!\" OIDA\n PASST SCHO\nPASST SCHO\n";

    #[test]
    fn klassen_erben_felder_und_methoden() {
        let programm = ok(&format!(
            "{WESEN}SOG AU g = Gegner(\"Sepp\", 5) OIDA\nSCHREIB(g.name, g.lebn, g.staerke, g.lebt(), g.sog()) OIDA"
        ));
        let (wesen, gegner) = (&programm.typen.klassen[0], &programm.typen.klassen[1]);
        assert_eq!(gegner.eltern, Some(0));
        // Die geerbten Felder kommen zuerst; lebn hat einen eigenen Wert und wird nicht angegeben.
        let felder: Vec<(&str, bool)> = gegner.felder.iter().map(|f| (f.name.as_str(), f.hat_standard)).collect();
        assert_eq!(felder, vec![("name", false), ("lebn", true), ("staerke", false)]);
        // sog() ist überschrieben und braucht einen Platz in der Sprungtabelle, lebt() nicht.
        let methoden: Vec<(&str, u32, Option<usize>)> =
            gegner.methoden.iter().map(|m| (m.name.as_str(), m.klasse, m.platz)).collect();
        assert_eq!(methoden, vec![("sog", 1, Some(0)), ("lebt", 0, None)]);
        assert_eq!(wesen.methoden[0].platz, Some(0));
        assert_eq!((wesen.plaetze, gegner.plaetze), (1, 1));

        let AnweisungArt::Deklaration { wert, .. } = &programm.haupt[0].art else { panic!() };
        assert_eq!(wert.typ, Typ::Klasse(1));
        assert!(matches!(&wert.art, AusdruckArt::Neu { klasse: 1, argumente } if argumente.len() == 2));
        // In der Methode von Gegner ruft DE OIDN die Fassung von Wesen.
        let AnweisungArt::GibZruck { wert: Some(wert) } = &programm.klassen[1].methoden[0].rumpf[0].art else { panic!() };
        let AusdruckArt::Binaer { links, .. } = &wert.art else { panic!() };
        assert!(matches!(links.art, AusdruckArt::ElternAufruf { klasse: 0, .. }));
    }

    #[test]
    fn kind_passt_wo_die_eltern_verlangt_sind() {
        let programm = ok(&format!(
            "{WESEN}FUNKTIAUN zeig(w Wesen)\n SCHREIB(w.sog()) OIDA\nPASST SCHO\n\
             SOG AU w Wesen = Gegner(\"a\", 1) OIDA\nSOG AU olle = [Gegner(\"b\", 2), Wesen(\"c\")] OIDA\n\
             zeig(Gegner(\"d\", 3)) OIDA\nw = Gegner(\"e\", 4) OIDA\nSCHREIB(w == olle[0]) OIDA"
        ));
        // Im Baum steht der Typ, als der der Wert verwendet wird.
        let AnweisungArt::Deklaration { wert, .. } = &programm.haupt[0].art else { panic!() };
        assert_eq!(wert.typ, Typ::Klasse(0));
        assert!(matches!(&wert.art, AusdruckArt::Umwandlung { ziel: Typ::Klasse(0), innen } if innen.typ == Typ::Klasse(1)));
        // Verschiedene Klassen in einer Liste: Es gilt die nächste gemeinsame.
        let AnweisungArt::Deklaration { wert, .. } = &programm.haupt[1].art else { panic!() };
        assert_eq!(programm.typen.element(wert.typ), Some(Typ::Klasse(0)));

        // Umgekehrt geht es nicht, und eine Liste von Kindern ist keine Liste von Eltern.
        let f = fehler(&format!("{WESEN}SOG AU g Gegner = Wesen(\"a\") OIDA"));
        assert_eq!(f.meldung, "'g' soll a Gegner sei, oba des do is a Wesen.");
        let f = fehler(&format!("{WESEN}SOG AU gs = [Gegner(\"a\", 1)] OIDA\nSOG AU ws LISTN VON Wesen = gs OIDA"));
        assert!(f.meldung.contains("LISTN VON Wesen"), "{}", f.meldung);
        let f = fehler(&format!("{WESEN}SOG AU w Wesen = Gegner(\"a\", 1) OIDA\nSCHREIB(w.staerke) OIDA"));
        assert_eq!(f.meldung, "A Föd 'staerke' gibt's in 'Wesen' ned. Es gibt: name, lebn.");
    }

    #[test]
    fn methoden_ueberschreiben_nur_mit_gleichem_aufbau() {
        let f = fehler(
            "KLASSE A\n FUNKTIAUN f(x ZOI) GIBT ZRUCK ZOI\n  GIB ZRUCK x OIDA\n PASST SCHO\nPASST SCHO\n\
             KLASSE B ERBT VON A\n FUNKTIAUN f(x KOMMAZOI) GIBT ZRUCK ZOI\n  GIB ZRUCK 1 OIDA\n PASST SCHO\nPASST SCHO",
        );
        assert!(f.meldung.starts_with("De Methode 'f' in 'B' passt ned zu der in 'A'"), "{}", f.meldung);
        // Geschwister dürfen gleich heißende Methoden haben, die nichts miteinander zu tun haben.
        let programm = ok("KLASSE A\nPASST SCHO\n\
             KLASSE B ERBT VON A\n FUNKTIAUN f() GIBT ZRUCK ZOI\n  GIB ZRUCK 1 OIDA\n PASST SCHO\nPASST SCHO\n\
             KLASSE C ERBT VON A\n FUNKTIAUN f(x KETTN)\n PASST SCHO\nPASST SCHO");
        assert_eq!(programm.typen.klassen[1].methoden[0].platz, None);
        assert_eq!(programm.typen.klassen[0].plaetze, 0);
    }

    const SUCH: &str = "KLASSE Wesen\n name KETTN\n ziel VIELLEICHT Wesen = NIX\nPASST SCHO\n\
        FUNKTIAUN such(n ZOI) GIBT ZRUCK VIELLEICHT Wesen\n GIB ZRUCK NIX OIDA\nPASST SCHO\n\
        FUNKTIAUN teil(a ZOI, b ZOI) GIBT ZRUCK ZOI ODA FEHLA\n \
        WENN b == 0 DAUN\n  GIB ZRUCK FEHLA(\"na\") OIDA\n PASST SCHO\n GIB ZRUCK a / b OIDA\nPASST SCHO\n";

    /// Die Anweisung `SCHREIB(...)` an der Stelle und ihr erster Wert.
    fn geschrieben(anweisungen: &[Anweisung], stelle: usize) -> &Ausdruck {
        let AnweisungArt::Schreib { argumente } = &anweisungen[stelle].art else { panic!("{:?}", anweisungen[stelle]) };
        &argumente[0]
    }

    #[test]
    fn vielleicht_typen() {
        let programm = ok(&format!(
            "{SUCH}SOG AU w = such(1) OIDA\nSOG AU z VIELLEICHT ZOI = NIX OIDA\nSOG AU l = [1, NIX] OIDA\n\
             SOG AU t VIELLEICHT KETTN = \"do\" OIDA"
        ));
        let typ = |n: usize| match &programm.haupt[n].art {
            AnweisungArt::Deklaration { wert, .. } => wert.typ,
            _ => panic!(),
        };
        // Ein Objekt oder Text, der fehlen kann, bleibt ein Zeiger; eine Zahl bekommt einen Merker.
        assert!(matches!(typ(0), Typ::Vielleicht { zeiger: true, .. }));
        assert_eq!(programm.typen.innen(typ(0)), Some(Typ::Klasse(0)));
        assert!(matches!(typ(1), Typ::Vielleicht { zeiger: false, .. }));
        assert_eq!(programm.typen.innen(typ(1)), Some(Typ::Zoi));
        // NIX in einer Liste: Dann darf überall darin etwas fehlen.
        assert_eq!(programm.typen.element(typ(2)), Some(typ(1)));
        // Ein echter Wert wird eingepackt.
        let AnweisungArt::Deklaration { wert, .. } = &programm.haupt[3].art else { panic!() };
        assert!(matches!(&wert.art, AusdruckArt::Umwandlung { innen, .. } if innen.typ == Typ::Kettn));

        let f = fehler("SOG AU x = NIX OIDA");
        assert!(f.meldung.starts_with("Bei NIX waaß i ned, wos do fehln soll."), "{}", f.meldung);
        let f = fehler("SOG AU x ZOI = NIX OIDA");
        assert_eq!(f.meldung, "'x' soll a ZOI sei, oba des do is NIX.");
        let f = fehler("STRUKTUR Knoten\n naechster VIELLEICHT Knoten\nPASST SCHO");
        assert!(f.meldung.contains("kaun si ned söwa enthoitn"), "{}", f.meldung);
    }

    #[test]
    fn erst_nachschauen_dann_verwenden() {
        let f = fehler(&format!("{SUCH}SOG AU w = such(1) OIDA\nSCHREIB(w.name) OIDA"));
        assert_eq!(f.meldung, "'w' kunnt NIX sei. Schau z'erst noch: WENN w != NIX DAUN ... (oda nimm ODA HOID).");

        // Im Block nach der Frage ist w ein Wesen, danach wieder nicht.
        let programm = ok(&format!(
            "{SUCH}SOG AU w = such(1) OIDA\nWENN w != NIX DAUN\n SCHREIB(w.name) OIDA\nSUNST\n SCHREIB(w) OIDA\nPASST SCHO\nSCHREIB(w) OIDA"
        ));
        let AnweisungArt::Wenn { bedingung, dann, sunst } = &programm.haupt[1].art else { panic!() };
        assert!(matches!(&bedingung.art, AusdruckArt::IstNix { ist: false, .. }));
        let AusdruckArt::Feld { objekt, .. } = &geschrieben(dann, 0).art else { panic!() };
        assert!(matches!(objekt.art, AusdruckArt::Sicher { .. }));
        assert_eq!(objekt.typ, Typ::Klasse(0));
        assert!(matches!(geschrieben(sunst, 0).art, AusdruckArt::Variable { .. }));
        assert!(matches!(geschrieben(&programm.haupt, 2).art, AusdruckArt::Variable { .. }));

        // Wer bei NIX aufhört, hat danach sicher etwas.
        ok(&format!(
            "{SUCH}FUNKTIAUN f() GIBT ZRUCK KETTN\n SOG AU w = such(1) OIDA\n \
             WENN w == NIX DAUN\n  GIB ZRUCK \"\" OIDA\n PASST SCHO\n GIB ZRUCK w.name OIDA\nPASST SCHO"
        ));
        // Rechts von UND gilt, was links gefragt wurde; rechts von ODA das Gegenteil.
        ok(&format!("{SUCH}SOG AU w = such(1) OIDA\nSCHREIB(w != NIX UND w.name == \"a\", w == NIX ODA w.name == \"a\") OIDA"));
        let f = fehler(&format!("{SUCH}SOG AU w = such(1) OIDA\nSCHREIB(w != NIX ODA w.name == \"a\") OIDA"));
        assert!(f.meldung.starts_with("'w' kunnt NIX sei."), "{}", f.meldung);
        // In einer Schleife mit HAU OB und WEIDA.
        ok(&format!(
            "{SUCH}FIA i VON 1 BIS 3 TUA\n SOG AU w = such(i) OIDA\n WENN w == NIX DAUN\n  WEIDA OIDA\n PASST SCHO\n SCHREIB(w.name) OIDA\nPASST SCHO"
        ));
    }

    #[test]
    fn zuweisen_hebt_die_sicherheit_auf() {
        let f = fehler(&format!(
            "{SUCH}SOG AU w = such(1) OIDA\nWENN w != NIX DAUN\n w = such(2) OIDA\n SCHREIB(w.name) OIDA\nPASST SCHO"
        ));
        assert_eq!(f.pos.zeile, 17, "{f:?}");
        // Umgekehrt: Nach einem echten Wert steht fest, dass etwas da ist.
        ok(&format!("{SUCH}SOG AU w = such(1) OIDA\nw = Wesen(\"a\") OIDA\nSCHREIB(w.name) OIDA"));
        // In einer Schleife zählt auch, was weiter unten zugewiesen wird: Beim
        // zweiten Durchlauf könnte es schon NIX sein.
        let f = fehler(&format!(
            "{SUCH}SOG AU w = such(1) OIDA\nWENN w != NIX DAUN\n SOLONG WOAHR TUA\n  SCHREIB(w.name) OIDA\n  w = NIX OIDA\n PASST SCHO\nPASST SCHO"
        ));
        assert_eq!(f.pos.zeile, 17, "{f:?}");
        // Die Kette entlang: Die Bedingung der Schleife fragt jedes Mal neu.
        ok(&format!("{SUCH}SOG AU w = such(1) OIDA\nSOLONG w != NIX TUA\n SCHREIB(w.name) OIDA\n w = w.ziel OIDA\nPASST SCHO"));
        // Ein Feld gilt nie als sicher: Jemand anderer könnte es ändern.
        let f = fehler(&format!(
            "{SUCH}SOG AU w = Wesen(\"a\") OIDA\nWENN w.ziel != NIX DAUN\n SCHREIB(w.ziel.name) OIDA\nPASST SCHO"
        ));
        assert!(f.meldung.starts_with("Des kunnt NIX sei. Tua's z'erst in a Variable"), "{}", f.meldung);
    }

    #[test]
    fn oda_hoid() {
        let programm = ok(&format!("{SUCH}SOG AU a = such(1) ODA HOID Wesen(\"neamd\") OIDA\nSOG AU b = teil(1, 0) ODA HOID 7 OIDA"));
        let typ = |n: usize| match &programm.haupt[n].art {
            AnweisungArt::Deklaration { wert, .. } => wert.typ,
            _ => panic!(),
        };
        assert_eq!((typ(0), typ(1)), (Typ::Klasse(0), Typ::Zoi));
        let f = fehler("SOG AU a = 5 ODA HOID 6 OIDA");
        assert_eq!(f.meldung, "Vua ODA HOID ghört wos, des NIX oda a FEHLA sei kunnt, des do is oba a ZOI.");
        let f = fehler(&format!("{SUCH}SOG AU a = teil(1, 2) ODA HOID \"zwa\" OIDA"));
        assert_eq!(f.meldung, "Noch ODA HOID brauch i a ZOI, des do is oba a KETTN.");
    }

    #[test]
    fn fehler_muss_man_behandeln() {
        let f = fehler(&format!("{SUCH}teil(1, 2) OIDA"));
        assert!(f.meldung.starts_with("Do kaun wos schiefgehn, des derfst ned afoch wegschmeißn."), "{}", f.meldung);
        let f = fehler(&format!("{SUCH}SCHREIB(teil(1, 2) + 1) OIDA"));
        assert!(f.meldung.contains("Do kunnt a FEHLA drin sei"), "{}", f.meldung);
        // Auffangen, mit und ohne Namen.
        let programm = ok(&format!(
            "{SUCH}PROBIER q = teil(1, 2)\n SCHREIB(q + 1) OIDA\nWONNS SCHIEFGEHT f\n SCHREIB(f.meldung) OIDA\nPASST SCHO\n\
             PROBIER teil(1, 2)\nWONNS SCHIEFGEHT\nPASST SCHO"
        ));
        let AnweisungArt::Probier { id, fehler_id, dann, sunst, .. } = &programm.haupt[0].art else { panic!() };
        assert_ne!(id, fehler_id);
        assert_eq!(geschrieben(dann, 0).typ, Typ::Zoi);
        assert_eq!(geschrieben(sunst, 0).typ, Typ::Kettn);
        // q gilt nur im ersten Block, f nur im zweiten.
        let f = fehler(&format!(
            "{SUCH}PROBIER q = teil(1, 2)\n SCHREIB(q) OIDA\nWONNS SCHIEFGEHT f\n SCHREIB(q) OIDA\nPASST SCHO"
        ));
        assert!(f.meldung.starts_with("'q' kenn i ned"), "{}", f.meldung);
    }

    #[test]
    fn probier_gibt_den_fehler_weiter() {
        let programm = ok(&format!(
            "{SUCH}FUNKTIAUN f() GIBT ZRUCK KETTN ODA FEHLA\n SOG AU q = PROBIER teil(1, 2) OIDA\n GIB ZRUCK KETTN(q) OIDA\nPASST SCHO\n\
             FUNKTIAUN g() GIBT ZRUCK NIX ODA FEHLA\n PROBIER f() OIDA\n WENN WOAHR DAUN\n  GIB ZRUCK OIDA\n PASST SCHO\nPASST SCHO"
        ));
        let f = &programm.funktionen[2];
        let AnweisungArt::Deklaration { wert, .. } = &f.rumpf[0].art else { panic!() };
        assert!(matches!(wert.art, AusdruckArt::Probier { .. }));
        assert_eq!(wert.typ, Typ::Zoi);
        // Der Text wird beim Zurückgeben in "Wert oder Fehler" eingepackt.
        let AnweisungArt::GibZruck { wert: Some(wert) } = &f.rumpf[1].art else { panic!() };
        assert_eq!(wert.typ, f.rueckgabe);
        assert_eq!(programm.typen.wert_von(f.rueckgabe), Some(Typ::Kettn));
        assert_eq!(programm.typen.wert_von(programm.funktionen[3].rueckgabe), Some(Typ::Nix));

        let f = fehler(&format!("{SUCH}SOG AU q = PROBIER teil(1, 2) OIDA"));
        assert!(f.meldung.starts_with("PROBIER gibt an FEHLA weida. Des geht nur in ana FUNKTIAUN"), "{}", f.meldung);
        let f = fehler(&format!(
            "{SUCH}FUNKTIAUN f() GIBT ZRUCK ZOI ODA FEHLA\n GIB ZRUCK 1 OIDA\nPASST SCHO\nFUNKTIAUN g() GIBT ZRUCK ZOI ODA FEHLA\n SCHREIB(PROBIER f()) OIDA\n GIB ZRUCK 1 OIDA\nPASST SCHO"
        ));
        assert!(f.meldung.starts_with("PROBIER muass gaunz vuan stehn"), "{}", f.meldung);
        // Ein Wert ODA FEHLA braucht auf jedem Weg ein GIB ZRUCK, NIX ODA FEHLA nicht.
        let f = fehler("FUNKTIAUN f() GIBT ZRUCK ZOI ODA FEHLA\nPASST SCHO");
        assert!(f.meldung.contains("ned jeda Weg hört mit GIB ZRUCK auf"), "{}", f.meldung);
        ok("FUNKTIAUN f() GIBT ZRUCK NIX ODA FEHLA\nPASST SCHO");
    }

    #[test]
    fn funktionen_als_werte() {
        let programm = ok("FUNKTIAUN doppelt(x ZOI) GIBT ZRUCK ZOI\n GIB ZRUCK x * 2 OIDA\nPASST SCHO\n\
             SOG AU f = doppelt OIDA\nSOG AU a = 5 OIDA\nSOG AU t = \"x\" OIDA\n\
             SOG AU g = FUNKTIAUN(x ZOI) => x + a + LÄNGN(t) + a OIDA\nSOG AU h = FUNKTIAUN(x ZOI) => x OIDA\n\
             SCHREIB(f(1), g(2), h(3)) OIDA");
        let wert = |n: usize| match &programm.haupt[n].art {
            AnweisungArt::Deklaration { wert, .. } => wert,
            _ => panic!(),
        };
        // Eine Funktion mit Namen als Wert; alle drei haben denselben Typ.
        assert!(matches!(&wert(0).art, AusdruckArt::FunktionsWert { name } if name == "doppelt"));
        assert_eq!(wert(0).typ, wert(3).typ);
        assert_eq!(wert(3).typ, wert(4).typ);
        let f = programm.typen.funktions_typ(wert(0).typ).unwrap();
        assert_eq!((f.parameter.clone(), f.rueckgabe), (vec![Typ::Zoi], Typ::Zoi));
        // g nimmt a und t mit, jedes einmal; h nichts. Drinnen haben sie eigene Nummern.
        let AusdruckArt::Lambda { fang, nummer, funktion, .. } = &wert(3).art else { panic!() };
        let namen: Vec<(&str, Typ)> = fang.iter().map(|f| (f.name.as_str(), f.typ)).collect();
        assert_eq!(namen, vec![("a", Typ::Zoi), ("t", Typ::Kettn)]);
        let AusdruckArt::Variable { id: aussen, .. } = &fang[0].wert.art else { panic!() };
        assert_ne!(*aussen, fang[0].id);
        assert_eq!(funktion.rueckgabe, Typ::Zoi);
        let AusdruckArt::Lambda { fang: fang_h, nummer: nummer_h, .. } = &wert(4).art else { panic!() };
        assert!(fang_h.is_empty());
        assert_ne!(nummer, nummer_h);
        // Aufrufen: über die Variable.
        let AnweisungArt::Schreib { argumente } = &programm.haupt[5].art else { panic!() };
        assert!(matches!(&argumente[0].art, AusdruckArt::WertAufruf { funktion, .. } if matches!(funktion.art, AusdruckArt::Variable { .. })));

        let f = fehler("SOG AU a = 1 OIDA\nSOG AU f = FUNKTIAUN() => a += 1 OIDA");
        assert!(f.meldung.starts_with("'a' kummt von außn"), "{}", f.meldung);
        let f = fehler("SOG AU a = 1 OIDA\na(2) OIDA");
        assert_eq!(f.meldung, "A ZOI kaun ma ned aufruafn, des is ka FUNKTIAUN.");
    }

    #[test]
    fn vorlagen_werden_je_typ_einmal_gebaut() {
        let programm = ok("FUNKTIAUN erstes VON T(liste LISTN VON T) GIBT ZRUCK T\n GIB ZRUCK liste[0] OIDA\nPASST SCHO\n\
             KLASSE Stopl VON T\n drin LISTN VON T = []\n FUNKTIAUN drauf(wos T)\n  HAU EINE(SÖWA.drin, wos) OIDA\n PASST SCHO\nPASST SCHO\n\
             STRUKTUR Paar VON A UND B\n links A\n rechts B\nPASST SCHO\n\
             SCHREIB(erstes([1]), erstes([2, 3]), erstes([\"a\"])) OIDA\n\
             SOG AU s = Stopl VON ZOI() OIDA\nSOG AU t Stopl VON ZOI = Stopl VON ZOI() OIDA\nSOG AU u = Stopl VON KETTN() OIDA\n\
             SOG AU p = Paar(1, \"a\") OIDA\nSOG AU q = Paar VON ZOI UND KETTN(2, \"b\") OIDA");
        // Die Vorlage selbst fällt weg; je Typ gibt es eine Fassung.
        let namen: Vec<&str> = programm.funktionen.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(namen.len(), 2);
        assert!(namen.contains(&"erstes VON ZOI") && namen.contains(&"erstes VON KETTN"), "{namen:?}");
        let klassen: Vec<(&str, bool)> = programm.typen.klassen.iter().map(|k| (k.name.as_str(), k.vorlage)).collect();
        assert_eq!(klassen, vec![("Stopl", true), ("Stopl VON ZOI", false), ("Stopl VON KETTN", false)]);
        // Die Klassen stehen im Programm in der Reihenfolge ihrer Nummern.
        let im_programm: Vec<&str> = programm.klassen.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(im_programm, vec!["Stopl", "Stopl VON ZOI", "Stopl VON KETTN"]);
        assert_eq!(programm.typen.klassen[1].felder[0].typ, programm.typen.klassen[1].methoden[0].parameter[0].pipe_liste(&programm.typen));
        let strukturen: Vec<&str> = programm.typen.strukturen.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(strukturen, vec!["Paar VON ZOI UND KETTN"]);
        assert_eq!(programm.typen.strukturen[0].felder, vec![("links".to_string(), Typ::Zoi), ("rechts".to_string(), Typ::Kettn)]);

        // Ob der Rumpf zum Typ passt, zeigt sich erst beim Verwenden.
        let f = fehler("STRUKTUR P\n x ZOI\nPASST SCHO\nFUNKTIAUN klana VON T(a T, b T) GIBT ZRUCK JONA\n GIB ZRUCK a < b OIDA\nPASST SCHO\n\
             SCHREIB(klana(1, 2), klana(P(1), P(2))) OIDA");
        assert_eq!(f.meldung, "A P < a P? Des geht si ned aus. (bei 'klana VON P')");
        let f = fehler("FUNKTIAUN erstes VON T(liste LISTN VON T) GIBT ZRUCK T\n GIB ZRUCK liste[0] OIDA\nPASST SCHO\nSCHREIB(erstes(5)) OIDA");
        assert!(f.meldung.starts_with("Bei 'erstes' waaß i ned, wos 'T' sei soll"), "{}", f.meldung);
    }

    #[test]
    fn verzeichnis() {
        let programm = ok("SOG AU v = VAZEICHNIS VON KETTN UND ZOI() OIDA\nv[\"a\"] = 1 OIDA\nSOG AU x = v[\"a\"] OIDA\n\
             FIA JEDE k IN v TUA\n SCHREIB(k + \"!\") OIDA\nPASST SCHO\nHAU AUSSE(v, \"a\") OIDA\nSCHREIB(LÄNGN(v)) OIDA");
        let AnweisungArt::Deklaration { wert, .. } = &programm.haupt[0].art else { panic!() };
        assert_eq!(programm.typen.verzeichnis(wert.typ), Some((Typ::Kettn, Typ::Zoi)));
        // Lesen gibt VIELLEICHT, weil der Schlüssel fehlen kann.
        let AnweisungArt::Deklaration { wert, .. } = &programm.haupt[2].art else { panic!() };
        assert_eq!(programm.typen.innen(wert.typ), Some(Typ::Zoi));
        let f = fehler("SOG AU v = VAZEICHNIS VON KETTN UND ZOI() OIDA\nSCHREIB(v[\"a\"] + 1) OIDA");
        assert!(f.meldung.starts_with("A VIELLEICHT ZOI + a ZOI?"), "{}", f.meldung);
        let f = fehler("SOG AU v = VAZEICHNIS VON JONA UND ZOI() OIDA");
        assert_eq!(f.meldung, "Ois Schlissl in an VAZEICHNIS geht nur a ZOI oda a KETTN, ned a JONA.");
    }

    #[test]
    fn soewa_gibt_es_nur_in_methoden() {
        let f = fehler("SCHREIB(SÖWA) OIDA");
        assert_eq!(f.meldung, "SÖWA gibt's nur in ana Methode.");
        let f = fehler("FUNKTIAUN f()\n DE OIDN.g() OIDA\nPASST SCHO");
        assert_eq!(f.meldung, "DE OIDN gibt's nur in ana Methode.");
        // Der Wert, mit dem ein Feld beginnt, wird ausgerechnet, bevor es das Objekt gibt.
        let f = fehler("KLASSE A\n x ZOI\n y ZOI = SÖWA.x\nPASST SCHO");
        assert_eq!(f.meldung, "SÖWA gibt's nur in ana Methode.");
    }
}
