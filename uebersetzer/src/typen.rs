//! Typprüfung mit Typerkennung (Inferenz).
//!
//! Geht einmal durch den Syntaxbaum und
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
    };

    // Erst alle Funktionen kennenlernen, damit die Reihenfolge egal ist.
    for f in &programm.funktionen {
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
    for a in &mut programm.haupt {
        p.anweisung(a);
    }

    if p.fehler.is_empty() {
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

    /// Wie ein Typ in einer Meldung heißt, mit Artikel: "a ZOI".
    fn typ_name(&self, typ: Typ) -> String {
        match typ {
            Typ::Zoi => format!("a {}", self.wort(Schluessel::Int)),
            Typ::Kommazoi => format!("a {}", self.wort(Schluessel::Float)),
            Typ::Jona => format!("a {}", self.wort(Schluessel::Boolean)),
            Typ::Kettn => format!("a {}", self.wort(Schluessel::Text)),
            Typ::Nix | Typ::Fehler => "nix".to_string(),
        }
    }

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
        self.funktion = Some((f.name.clone(), f.rueckgabe));
        for parameter in &mut f.parameter {
            parameter.id = self.neue_variable(&parameter.name, parameter.typ, parameter.pos);
        }
        for a in &mut f.rumpf {
            self.anweisung(a);
        }
        if f.rueckgabe != Typ::Nix && !gibt_sicher_zruck(&f.rumpf) {
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
            AnweisungArt::Deklaration { name, typ, wert, id } => {
                let wert_typ = self.ausdruck(wert);
                let mut var_typ = typ.unwrap_or(wert_typ);
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
                }
                // Erst jetzt eintragen: "SOG AU x = x" soll das äußere x meinen.
                *id = self.neue_variable(name, var_typ, pos);
            }
            AnweisungArt::Zuweisung { name, wert, id } => {
                let wert_typ = self.ausdruck(wert);
                match self.finde_variable(name).map(|v| (v.id, v.typ)) {
                    None => self.unbekannte_variable(name, pos),
                    Some((var_id, var_typ)) => {
                        *id = var_id;
                        if !self.anpassen(wert, var_typ) {
                            let meldung = format!(
                                "'{name}' is {}, do kaunst {} einetuan.",
                                self.typ_name(var_typ),
                                self.typ_name(wert_typ).replacen("a ", "ka ", 1)
                            );
                            self.melde(wert.pos, meldung);
                        }
                    }
                }
            }
            AnweisungArt::Wenn { bedingung, dann, sunst } => {
                self.bedingung(bedingung, Schluessel::If);
                self.block(dann);
                self.block(sunst);
            }
            AnweisungArt::Solong { bedingung, rumpf } => {
                self.bedingung(bedingung, Schluessel::While);
                self.block(rumpf);
            }
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
                            self.typ_name(wert_typ.unwrap()).replacen("a ", "", 1)
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
                    if self.ausdruck(argument) == Typ::Nix {
                        let meldung = format!(
                            "Des gibt nix zruck, do gibt's nix zum {}.",
                            self.wort(Schluessel::Print)
                        );
                        self.melde(argument.pos, meldung);
                    }
                }
            }
            AnweisungArt::Ausdruck(ausdruck) => {
                self.ausdruck(ausdruck);
                if !matches!(ausdruck.art, AusdruckArt::Aufruf { .. }) {
                    self.melde(ausdruck.pos, "Des rechnet wos aus und schmeißt's glei wieda weg. Wos soll des?");
                }
            }
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
    /// wird dabei von selbst eine KOMMAZOI.
    fn anpassen(&mut self, ausdruck: &mut Ausdruck, ziel: Typ) -> bool {
        if ausdruck.typ == ziel || ausdruck.typ == Typ::Fehler || ziel == Typ::Fehler {
            return true;
        }
        if ausdruck.typ == Typ::Zoi && ziel == Typ::Kommazoi {
            einpacken(ausdruck, Typ::Kommazoi);
            return true;
        }
        false
    }

    /// Bestimmt den Typ eines Ausdrucks und trägt ihn ein.
    fn ausdruck(&mut self, ausdruck: &mut Ausdruck) -> Typ {
        let pos = ausdruck.pos;
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
        };
        ausdruck.typ = typ;
        typ
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
        let zahl = |t: Typ| t == Typ::Zoi || t == Typ::Kommazoi;

        // Zwei Zahlen: trifft ZOI auf KOMMAZOI, wird mit KOMMAZOI gerechnet.
        let mut zahlen = None;
        if zahl(l) && zahl(r) {
            let gemeinsam = if l == Typ::Kommazoi || r == Typ::Kommazoi { Typ::Kommazoi } else { Typ::Zoi };
            self.anpassen(links, gemeinsam);
            self.anpassen(rechts, gemeinsam);
            zahlen = Some(gemeinsam);
        }

        let ergebnis = match op {
            BinaerOp::Plus => zahlen.or((l == Typ::Kettn && r == Typ::Kettn).then_some(Typ::Kettn)),
            BinaerOp::Minus | BinaerOp::Mal | BinaerOp::Durch | BinaerOp::Rest => zahlen,
            BinaerOp::Gleich | BinaerOp::Ungleich => (zahlen.is_some() || (l == r && l != Typ::Nix)).then_some(Typ::Jona),
            BinaerOp::Kleiner | BinaerOp::KleinerGleich | BinaerOp::Groesser | BinaerOp::GroesserGleich => {
                (zahlen.is_some() || (l == Typ::Kettn && r == Typ::Kettn)).then_some(Typ::Jona)
            }
            BinaerOp::Und | BinaerOp::Oda => (l == Typ::Jona && r == Typ::Jona).then_some(Typ::Jona),
        };
        if let Some(typ) = ergebnis {
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
        let AnweisungArt::Zuweisung { id: ziel, .. } = programm.haupt[2].art else { panic!() };
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
}
