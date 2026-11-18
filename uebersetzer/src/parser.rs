//! Parser: macht aus den Tokens den Syntaxbaum.
//!
//! Grammatik (grob):
//!
//! ```text
//! programm   = { struktur | klasse | funktion | anweisung }
//! struktur   = STRUKTUR name { name typ [ OIDA ] } PASST SCHO [ OIDA ]
//! klasse     = KLASSE name [ ERBT VON name ]
//!              { name typ [ "=" ausdruck ] [ OIDA ] | funktion } PASST SCHO [ OIDA ]
//! funktion   = FUNKTIAUN name "(" [ name typ { "," name typ } ] ")" [ GIBT ZRUCK typ ]
//!              { anweisung } PASST SCHO [ OIDA ]
//! typ        = ZOI | KOMMAZOI | JONA | KETTN | name | LISTN VON typ
//! anweisung  = SOG AU name [ typ ] "=" ausdruck OIDA
//!            | ziel ( "=" | "+=" | "-=" | "*=" | "/=" | "%=" ) ausdruck OIDA
//!            | WENN ausdruck DAUN { anweisung } [ SUNST ( wenn | { anweisung } ) ] PASST SCHO [ OIDA ]
//!            | SOLONG ausdruck TUA { anweisung } PASST SCHO [ OIDA ]
//!            | FIA name VON ausdruck BIS ausdruck [ SCHRITT ausdruck ] TUA { anweisung } PASST SCHO [ OIDA ]
//!            | FIA JEDE name IN ausdruck TUA { anweisung } PASST SCHO [ OIDA ]
//!            | HAU OB OIDA
//!            | WEIDA OIDA
//!            | GIB ZRUCK [ ausdruck ] OIDA
//!            | SCHREIB "(" [ ausdruck { "," ausdruck } ] ")" OIDA
//!            | ausdruck OIDA
//! ausdruck   = und { ODA und }
//! und        = ned { UND ned }
//! ned        = NED ned | vergleich
//! vergleich  = summe [ ( "==" | "=" | "!=" | "<>" | "<" | "<=" | ">" | ">=" ) summe ]
//! summe      = produkt { ( "+" | "-" ) produkt }
//! produkt    = unaer { ( "*" | "/" | "%" ) unaer }
//! unaer      = "-" unaer | primaer
//! primaer    = atom { "." name [ "(" argumente ")" ] | "[" ausdruck "]" }
//! atom       = zahl | text | WOAHR | FOISCH | PI | name [ "(" argumente ")" ]
//!            | SÖWA | DE OIDN "." name "(" argumente ")"
//!            | eingebaute_funktion "(" argumente ")"
//!            | ( ZOI | KOMMAZOI | JONA | KETTN ) "(" ausdruck ")"
//!            | "[" [ ausdruck { "," ausdruck } ] "]" | "(" ausdruck ")"
//! ziel       = ( name | SÖWA ) { "." name | "[" ausdruck "]" }
//! ```

use crate::ast::*;
use crate::fehler::{Fehler, Position};
use crate::lexer::{Token, TokenArt};
use crate::woerterbuch::{Schluessel, Woerterbuch};

struct Parser<'a> {
    tokens: Vec<Token>,
    i: usize,
    wb: &'a Woerterbuch,
}

/// Macht aus den Tokens (von [`crate::lexer::zerlegen`]) den Syntaxbaum.
pub fn parsen(tokens: Vec<Token>, wb: &Woerterbuch) -> Result<Programm, Fehler> {
    assert!(matches!(tokens.last().map(|t| &t.art), Some(TokenArt::DateiEnde)));
    Parser { tokens, i: 0, wb }.programm()
}

impl Parser<'_> {
    // ------------------------------------------------------------ Hilfen

    fn aktuell(&self) -> &Token {
        &self.tokens[self.i]
    }

    fn naechstes(&self) -> &TokenArt {
        &self.tokens[(self.i + 1).min(self.tokens.len() - 1)].art
    }

    fn weiter(&mut self) -> Token {
        let token = self.tokens[self.i].clone();
        if self.i + 1 < self.tokens.len() {
            self.i += 1;
        }
        token
    }

    /// Die Stelle direkt hinter dem vorigen Token.
    fn hinter_vorigem(&self) -> Position {
        if self.i == 0 { self.aktuell().pos } else { self.tokens[self.i - 1].ende }
    }

    fn ist(&self, schluessel: Schluessel) -> bool {
        self.aktuell().art == TokenArt::Schluessel(schluessel)
    }

    fn nimm(&mut self, schluessel: Schluessel) -> bool {
        let passt = self.ist(schluessel);
        if passt {
            self.weiter();
        }
        passt
    }

    fn wort(&self, schluessel: Schluessel) -> &str {
        self.wb.wort(schluessel)
    }

    /// Wie ein Token in einer Meldung heißt.
    fn beschreib(&self, token: &Token) -> String {
        match &token.art {
            TokenArt::Schluessel(s) => self.wort(*s).to_string(),
            TokenArt::Name(n) => format!("'{n}'"),
            TokenArt::Zoi(z) => format!("de Zoi {z}"),
            TokenArt::Kommazoi(k) => format!("de Zoi {k:?}"),
            TokenArt::Text(_) => "a Text".to_string(),
            TokenArt::KlammerAuf => "'('".to_string(),
            TokenArt::KlammerZu => "')'".to_string(),
            TokenArt::EckigAuf => "'['".to_string(),
            TokenArt::EckigZu => "']'".to_string(),
            TokenArt::Punkt => "'.'".to_string(),
            TokenArt::Komma => "','".to_string(),
            TokenArt::Plus => "'+'".to_string(),
            TokenArt::Minus => "'-'".to_string(),
            TokenArt::Mal => "'*'".to_string(),
            TokenArt::Durch => "'/'".to_string(),
            TokenArt::Prozent => "'%'".to_string(),
            TokenArt::PlusGleich => "'+='".to_string(),
            TokenArt::MinusGleich => "'-='".to_string(),
            TokenArt::MalGleich => "'*='".to_string(),
            TokenArt::DurchGleich => "'/='".to_string(),
            TokenArt::ProzentGleich => "'%='".to_string(),
            TokenArt::Gleich => "'='".to_string(),
            TokenArt::GleichGleich => "'=='".to_string(),
            TokenArt::Pfeil => "'=>'".to_string(),
            TokenArt::Ungleich => "'!='".to_string(),
            TokenArt::Kleiner => "'<'".to_string(),
            TokenArt::KleinerGleich => "'<='".to_string(),
            TokenArt::Groesser => "'>'".to_string(),
            TokenArt::GroesserGleich => "'>='".to_string(),
            TokenArt::DateiEnde => "des End von da Datei".to_string(),
        }
    }

    /// Steht hier ein englisches Wort wie IF, das im Wörterbuch anders heißt?
    fn englisch(&self) -> Option<Fehler> {
        let token = self.aktuell();
        let TokenArt::Name(name) = &token.art else { return None };
        let schluessel = self.wb.festes_wort(name)?;
        Some(Fehler::neu(token.pos, format!("Des haaßt {}, ned {}!", self.wort(schluessel), name)))
    }

    /// Fehler "hier gehört X her, nicht das aktuelle Token".
    fn unerwartet(&self, erwartet: &str) -> Fehler {
        if let Some(fehler) = self.englisch() {
            return fehler;
        }
        let token = self.aktuell();
        Fehler::neu(token.pos, format!("Do ghört {erwartet} her, ned {}.", self.beschreib(token)))
    }

    fn erwarte(&mut self, art: TokenArt, erwartet: &str) -> Result<Token, Fehler> {
        if self.aktuell().art == art { Ok(self.weiter()) } else { Err(self.unerwartet(erwartet)) }
    }

    fn erwarte_schluessel(&mut self, schluessel: Schluessel) -> Result<Token, Fehler> {
        let erwartet = self.wort(schluessel).to_string();
        self.erwarte(TokenArt::Schluessel(schluessel), &erwartet)
    }

    /// Jede Anweisung hört mit OIDA auf. Der Fehler zeigt auf die Stelle,
    /// wo das OIDA hingehört (hinter das letzte Token der Anweisung).
    fn erwarte_oida(&mut self) -> Result<(), Fehler> {
        if self.nimm(Schluessel::Oida) {
            return Ok(());
        }
        if let Some(fehler) = self.englisch() {
            return Err(fehler);
        }
        Err(Fehler::neu(
            self.hinter_vorigem(),
            format!("Heast, wo is des {}?", self.wort(Schluessel::Oida)),
        ))
    }

    fn name(&mut self, wofuer: &str) -> Result<(String, Position), Fehler> {
        let token = self.aktuell().clone();
        match token.art {
            TokenArt::Name(name) => {
                self.weiter();
                Ok((name, token.pos))
            }
            TokenArt::Schluessel(s) => Err(Fehler::neu(
                token.pos,
                format!(
                    "Do ghört a Nom fia {wofuer} her. {} is scho a Wort von da Sproch, des kaunst ned nehma.",
                    self.wort(s)
                ),
            )),
            _ => Err(self.unerwartet(&format!("a Nom fia {wofuer}"))),
        }
    }

    fn typ_hier(&self) -> Option<Typ> {
        match self.aktuell().art {
            TokenArt::Schluessel(Schluessel::Int) => Some(Typ::Zoi),
            TokenArt::Schluessel(Schluessel::Float) => Some(Typ::Kommazoi),
            TokenArt::Schluessel(Schluessel::Boolean) => Some(Typ::Jona),
            TokenArt::Schluessel(Schluessel::Text) => Some(Typ::Kettn),
            TokenArt::Schluessel(Schluessel::Error) => Some(Typ::Fehla),
            _ => None,
        }
    }

    /// Ein Typ, wie man ihn hinschreibt. Am Ende darf `ODA FEHLA` stehen
    /// (statt eines Typs davor auch `NIX`): Dann ist es ein Wert oder ein Fehler.
    fn typ_angabe(&mut self) -> Result<TypAngabe, Fehler> {
        let pos = self.aktuell().pos;
        if self.nimm(Schluessel::Null) {
            self.erwarte_schluessel(Schluessel::Or)?;
            self.erwarte_schluessel(Schluessel::Error)?;
            return Ok(TypAngabe::OderFehler { wert: None, pos });
        }
        let kern = self.typ_kern()?;
        if self.ist(Schluessel::Or) && *self.naechstes() == TokenArt::Schluessel(Schluessel::Error) {
            self.weiter();
            self.weiter();
            return Ok(TypAngabe::OderFehler { wert: Some(Box::new(kern)), pos });
        }
        Ok(kern)
    }

    /// Ein Typ ohne `ODA FEHLA`: einer der einfachen, der Name einer Struktur
    /// oder Klasse, `LISTN VON <typ>` oder `VIELLEICHT <typ>`.
    fn typ_kern(&mut self) -> Result<TypAngabe, Fehler> {
        if let Some(typ) = self.typ_hier() {
            self.weiter();
            return Ok(TypAngabe::Einfach(typ));
        }
        let pos = self.aktuell().pos;
        if self.nimm(Schluessel::Maybe) {
            return Ok(TypAngabe::Vielleicht { innen: Box::new(self.typ_kern()?), pos });
        }
        // FUNKTIAUN(ZOI, KETTN) GIBT ZRUCK JONA: eine Funktion als Wert
        if self.ist(Schluessel::Function) && *self.naechstes() == TokenArt::KlammerAuf {
            self.weiter();
            self.weiter();
            let mut parameter = Vec::new();
            if self.aktuell().art != TokenArt::KlammerZu {
                loop {
                    parameter.push(self.typ_angabe()?);
                    if self.aktuell().art == TokenArt::Komma {
                        self.weiter();
                    } else {
                        break;
                    }
                }
            }
            self.erwarte(TokenArt::KlammerZu, "a ',' oda a ')'")?;
            let rueckgabe = if self.nimm(Schluessel::Returns) { Some(Box::new(self.typ_angabe()?)) } else { None };
            return Ok(TypAngabe::Funktion { parameter, rueckgabe, pos });
        }
        // VAZEICHNIS VON KETTN UND ZOI
        if self.nimm(Schluessel::Map) {
            self.erwarte_schluessel(Schluessel::From)?;
            let schluessel = Box::new(self.typ_kern()?);
            self.erwarte_schluessel(Schluessel::And)?;
            let wert = Box::new(self.typ_kern()?);
            return Ok(TypAngabe::Verzeichnis { schluessel, wert, pos });
        }
        if self.nimm(Schluessel::List) {
            self.erwarte_schluessel(Schluessel::From)?;
            return Ok(TypAngabe::Liste(Box::new(self.typ_kern()?)));
        }
        let token = self.aktuell().clone();
        if let TokenArt::Name(name) = token.art {
            self.weiter();
            // Stapel VON ZOI, Paar VON ZOI UND KETTN: eine Vorlage mit ihren Typen
            if self.nimm(Schluessel::From) {
                let mut argumente = vec![self.typ_kern()?];
                while self.nimm(Schluessel::And) {
                    argumente.push(self.typ_kern()?);
                }
                return Ok(TypAngabe::Generisch { name, argumente, pos: token.pos });
            }
            return Ok(TypAngabe::Name { name, pos: token.pos });
        }
        Err(self.unerwartet(&format!(
            "a Typ ({}, {}, {}, {}, {} {} ... oda a {})",
            self.wort(Schluessel::Int),
            self.wort(Schluessel::Float),
            self.wort(Schluessel::Boolean),
            self.wort(Schluessel::Text),
            self.wort(Schluessel::List),
            self.wort(Schluessel::From),
            self.wort(Schluessel::Struct)
        )))
    }

    /// `VON T` oder `VON A UND B` hinter dem Namen einer Funktion, Struktur
    /// oder Klasse: die Typen, die erst beim Verwenden feststehen.
    fn typ_parameter(&mut self) -> Result<Vec<String>, Fehler> {
        let mut namen = Vec::new();
        if self.nimm(Schluessel::From) {
            loop {
                namen.push(self.name("den Typ, der no offn is")?.0);
                if !self.nimm(Schluessel::And) {
                    break;
                }
            }
        }
        Ok(namen)
    }

    /// Die Parameter einer Funktion bis zur schließenden Klammer; die öffnende ist schon gelesen.
    fn parameter(&mut self) -> Result<Vec<Parameter>, Fehler> {
        let mut parameter = Vec::new();
        if self.aktuell().art != TokenArt::KlammerZu {
            loop {
                let (pname, ppos) = self.name("den Parameter")?;
                let angabe = self.typ_angabe()?;
                let typ = einfacher_typ(&angabe);
                parameter.push(Parameter { name: pname, angabe, typ, pos: ppos, id: 0 });
                if self.aktuell().art == TokenArt::Komma {
                    self.weiter();
                } else {
                    break;
                }
            }
        }
        self.erwarte(TokenArt::KlammerZu, "a ',' oda a ')'")?;
        Ok(parameter)
    }

    /// Eine Funktion ohne Namen; `FUNKTIAUN` und `(` sind schon gelesen.
    ///
    ///  - kurz: `FUNKTIAUN(x ZOI) => x * 2`
    ///  - mit Block: `FUNKTIAUN(x ZOI) GIBT ZRUCK ZOI ... PASST SCHO`
    fn lambda(&mut self, pos: Position) -> Result<Ausdruck, Fehler> {
        let parameter = self.parameter()?;
        let mut funktion = Funktion {
            name: String::new(),
            parameter,
            rueckgabe_angabe: None,
            rueckgabe: Typ::Nix,
            rumpf: Vec::new(),
            pos,
            selbst_id: 0,
            typ_parameter: Vec::new(),
        };
        if self.aktuell().art == TokenArt::Pfeil {
            let pfeil = self.weiter().pos;
            // => SCHREIB(...): Ausgeben ist eine Anweisung, soll in der kurzen Form aber auch gehen.
            if self.nimm(Schluessel::Print) {
                self.erwarte(TokenArt::KlammerAuf, "a '('")?;
                let argumente = self.argumente()?;
                funktion.rumpf.push(Anweisung { art: AnweisungArt::Schreib { argumente }, pos: pfeil });
                return Ok(Ausdruck::neu(
                    AusdruckArt::Lambda { funktion: Box::new(funktion), kurz: false, fang: Vec::new(), nummer: 0 },
                    pos,
                ));
            }
            // => ziel += wert: auch das ist eine Anweisung. (Ein einfaches = wäre hier ein Vergleich.)
            let anfang = self.i;
            if let Ok(ziel) = self.primaer()
                && let Some(op) = rechen_zuweisung(&self.aktuell().art)
            {
                self.weiter();
                let wert = self.ausdruck()?;
                let art = AnweisungArt::Zuweisung { ziel, op: Some(op), wert };
                funktion.rumpf.push(Anweisung { art, pos: pfeil });
                return Ok(Ausdruck::neu(
                    AusdruckArt::Lambda { funktion: Box::new(funktion), kurz: false, fang: Vec::new(), nummer: 0 },
                    pos,
                ));
            }
            self.i = anfang;
            let wert = self.ausdruck()?;
            // Vorerst als GIB ZRUCK; gibt der Ausdruck nichts zurück, macht die Typprüfung einen Aufruf daraus.
            funktion.rumpf.push(Anweisung { art: AnweisungArt::GibZruck { wert: Some(wert) }, pos: pfeil });
            return Ok(Ausdruck::neu(
                AusdruckArt::Lambda { funktion: Box::new(funktion), kurz: true, fang: Vec::new(), nummer: 0 },
                pos,
            ));
        }
        if self.nimm(Schluessel::Returns) {
            let angabe = self.typ_angabe()?;
            funktion.rueckgabe = einfacher_typ(&angabe);
            funktion.rueckgabe_angabe = Some(angabe);
        }
        funktion.rumpf = self.block()?;
        // Das OIDA danach gehört zur Anweisung rundherum, nicht zur Funktion.
        if !self.nimm(Schluessel::End) {
            return Err(Fehler::neu(
                pos,
                format!(
                    "De {} ohne Nom hört nimma auf. Do fehlt a {} (oda manst de kurze Form mit => ?).",
                    self.wort(Schluessel::Function),
                    self.wort(Schluessel::End)
                ),
            ));
        }
        Ok(Ausdruck::neu(
            AusdruckArt::Lambda { funktion: Box::new(funktion), kurz: false, fang: Vec::new(), nummer: 0 },
            pos,
        ))
    }

    // ---------------------------------------------------------- Programm

    fn programm(mut self) -> Result<Programm, Fehler> {
        let mut programm = Programm::default();
        loop {
            let token = self.aktuell();
            match token.art {
                TokenArt::DateiEnde => return Ok(programm),
                TokenArt::Schluessel(Schluessel::Function) => {
                    let funktion = self.funktion()?;
                    programm.funktionen.push(funktion);
                }
                TokenArt::Schluessel(Schluessel::Struct) => {
                    let struktur = self.struktur()?;
                    programm.strukturen.push(struktur);
                }
                TokenArt::Schluessel(Schluessel::Class) => {
                    let klasse = self.klasse()?;
                    programm.klassen.push(klasse);
                }
                TokenArt::Schluessel(Schluessel::End) => {
                    return Err(Fehler::neu(
                        token.pos,
                        format!("Des {} ghört zu nix, do is nix offn.", self.wort(Schluessel::End)),
                    ));
                }
                TokenArt::Schluessel(Schluessel::Else) => return Err(self.sunst_ohne_wenn()),
                _ => {
                    let anweisung = self.anweisung()?;
                    programm.haupt.push(anweisung);
                }
            }
        }
    }

    fn sunst_ohne_wenn(&self) -> Fehler {
        Fehler::neu(
            self.aktuell().pos,
            format!(
                "A {} ohne {}? Des geht si ned aus.",
                self.wort(Schluessel::Else),
                self.wort(Schluessel::If)
            ),
        )
    }

    fn struktur(&mut self) -> Result<StrukturDef, Fehler> {
        let pos = self.weiter().pos; // STRUKTUR
        let wofuer = format!("de {}", self.wort(Schluessel::Struct));
        let (name, _) = self.name(&wofuer)?;
        let typ_parameter = self.typ_parameter()?;
        let mut felder = Vec::new();
        while !self.ist(Schluessel::End) && self.aktuell().art != TokenArt::DateiEnde {
            let (feld, feld_pos) = self.name("des Föd")?;
            let angabe = self.typ_angabe()?;
            // Ein OIDA hinter dem Feld darf sein, muss aber nicht.
            self.nimm(Schluessel::Oida);
            felder.push(FeldDef { name: feld, angabe, pos: feld_pos });
        }
        self.erwarte_end(pos, Schluessel::Struct)?;
        Ok(StrukturDef { name, typ_parameter, felder, pos })
    }

    fn klasse(&mut self) -> Result<KlassenDef, Fehler> {
        let pos = self.weiter().pos; // KLASSE
        let wofuer = format!("de {}", self.wort(Schluessel::Class));
        let (name, _) = self.name(&wofuer)?;
        let typ_parameter = self.typ_parameter()?;
        let eltern = if self.nimm(Schluessel::Extends) {
            let wofuer = format!("de {}, von der '{name}' erbt", self.wort(Schluessel::Class));
            Some(self.name(&wofuer)?)
        } else {
            None
        };
        let mut felder = Vec::new();
        let mut methoden = Vec::new();
        while !self.ist(Schluessel::End) && self.aktuell().art != TokenArt::DateiEnde {
            if self.ist(Schluessel::Function) {
                methoden.push(self.funktion()?);
                continue;
            }
            let (feld, feld_pos) = self.name("des Föd")?;
            let angabe = self.typ_angabe()?;
            let standard = if self.aktuell().art == TokenArt::Gleich {
                self.weiter();
                Some(self.ausdruck()?)
            } else {
                None
            };
            // Ein OIDA hinter dem Feld darf sein, muss aber nicht.
            self.nimm(Schluessel::Oida);
            felder.push(KlassenFeldDef { name: feld, angabe, standard, pos: feld_pos });
        }
        self.erwarte_end(pos, Schluessel::Class)?;
        Ok(KlassenDef { name, typ_parameter, eltern, felder, methoden, pos })
    }

    fn funktion(&mut self) -> Result<Funktion, Fehler> {
        let pos = self.weiter().pos; // FUNKTIAUN
        let wofuer = format!("de {}", self.wort(Schluessel::Function));
        let (name, _) = self.name(&wofuer)?;
        let typ_parameter = self.typ_parameter()?;
        self.erwarte(TokenArt::KlammerAuf, "a '('")?;
        let parameter = self.parameter()?;
        let rueckgabe_angabe = if self.nimm(Schluessel::Returns) { Some(self.typ_angabe()?) } else { None };
        let rueckgabe = rueckgabe_angabe.as_ref().map_or(Typ::Nix, einfacher_typ);
        let rumpf = self.block()?;
        self.erwarte_end(pos, Schluessel::Function)?;
        Ok(Funktion { name, parameter, rueckgabe_angabe, rueckgabe, rumpf, pos, selbst_id: 0, typ_parameter })
    }

    /// Anweisungen bis PASST SCHO, SUNST oder zum Ende der Datei.
    fn block(&mut self) -> Result<Vec<Anweisung>, Fehler> {
        let mut anweisungen = Vec::new();
        loop {
            let token = self.aktuell();
            match token.art {
                TokenArt::DateiEnde
                | TokenArt::Schluessel(Schluessel::End)
                | TokenArt::Schluessel(Schluessel::Else)
                | TokenArt::Schluessel(Schluessel::Catch) => return Ok(anweisungen),
                TokenArt::Schluessel(wort @ (Schluessel::Function | Schluessel::Struct | Schluessel::Class)) => {
                    return Err(Fehler::neu(
                        token.pos,
                        format!("A {} kaunst ned in wos aundas einebaun, de ghört gaunz noch außn.", self.wort(wort)),
                    ));
                }
                _ => anweisungen.push(self.anweisung()?),
            }
        }
    }

    /// Erwartet das PASST SCHO zu dem Block, der bei `anfang` mit `wort` begonnen hat.
    /// Ein OIDA danach ist erlaubt, muss aber nicht sein.
    fn erwarte_end(&mut self, anfang: Position, wort: Schluessel) -> Result<(), Fehler> {
        if self.nimm(Schluessel::End) {
            self.nimm(Schluessel::Oida);
            return Ok(());
        }
        if self.ist(Schluessel::Else) {
            return Err(self.sunst_ohne_wenn());
        }
        if self.ist(Schluessel::Catch) {
            return Err(Fehler::neu(
                self.aktuell().pos,
                format!(
                    "{} ohne {}? Des ghört zu an {}.",
                    self.wort(Schluessel::Catch),
                    self.wort(Schluessel::Try),
                    self.wort(Schluessel::Try)
                ),
            ));
        }
        Err(Fehler::neu(
            anfang,
            format!(
                "Des {} hört nimma auf. Do fehlt a {}.",
                self.wort(wort),
                self.wort(Schluessel::End)
            ),
        ))
    }

    // ------------------------------------------------------- Anweisungen

    fn anweisung(&mut self) -> Result<Anweisung, Fehler> {
        let token = self.aktuell().clone();
        let pos = token.pos;
        let art = match token.art {
            TokenArt::Schluessel(Schluessel::Declare) => {
                self.weiter();
                let (name, _) = self.name("de Variable")?;
                // Steht vor dem = noch etwas, ist es der Typ.
                let angabe = if self.aktuell().art != TokenArt::Gleich && self.kann_typ_anfangen() {
                    Some(self.typ_angabe()?)
                } else {
                    None
                };
                self.erwarte(TokenArt::Gleich, "a '='")?;
                let wert = self.ausdruck()?;
                self.erwarte_oida()?;
                AnweisungArt::Deklaration { name, angabe, wert, id: 0 }
            }
            TokenArt::Schluessel(Schluessel::If) => {
                self.weiter();
                return self.wenn(pos);
            }
            TokenArt::Schluessel(Schluessel::While) => {
                self.weiter();
                let bedingung = self.ausdruck()?;
                self.erwarte_schluessel(Schluessel::Do)?;
                let rumpf = self.block()?;
                self.erwarte_end(pos, Schluessel::While)?;
                AnweisungArt::Solong { bedingung, rumpf }
            }
            TokenArt::Schluessel(Schluessel::For) if *self.naechstes() == TokenArt::Schluessel(Schluessel::Each) => {
                self.weiter();
                self.weiter();
                let (name, _) = self.name("des Element")?;
                self.erwarte_schluessel(Schluessel::In)?;
                let liste = self.ausdruck()?;
                self.erwarte_schluessel(Schluessel::Do)?;
                let rumpf = self.block()?;
                self.erwarte_end(pos, Schluessel::For)?;
                AnweisungArt::FiaJede { name, id: 0, liste, rumpf }
            }
            TokenArt::Schluessel(Schluessel::For) => {
                self.weiter();
                let (name, _) = self.name("de Zöhvariable")?;
                self.erwarte_schluessel(Schluessel::From)?;
                let von = self.ausdruck()?;
                self.erwarte_schluessel(Schluessel::To)?;
                let bis = self.ausdruck()?;
                let schritt = if self.nimm(Schluessel::Step) { Some(self.ausdruck()?) } else { None };
                self.erwarte_schluessel(Schluessel::Do)?;
                let rumpf = self.block()?;
                self.erwarte_end(pos, Schluessel::For)?;
                AnweisungArt::Fia { name, id: 0, von, bis, schritt, rumpf }
            }
            TokenArt::Schluessel(Schluessel::Break) => {
                self.weiter();
                self.erwarte_oida()?;
                AnweisungArt::HauOb
            }
            TokenArt::Schluessel(Schluessel::Continue) => {
                self.weiter();
                self.erwarte_oida()?;
                AnweisungArt::Weida
            }
            TokenArt::Schluessel(Schluessel::Return) => {
                self.weiter();
                let wert = if self.kann_ausdruck_anfangen() { Some(self.ausdruck()?) } else { None };
                self.erwarte_oida()?;
                AnweisungArt::GibZruck { wert }
            }
            TokenArt::Schluessel(Schluessel::Print) => {
                self.weiter();
                self.erwarte(TokenArt::KlammerAuf, "a '('")?;
                let argumente = self.argumente()?;
                self.erwarte_oida()?;
                AnweisungArt::Schreib { argumente }
            }
            TokenArt::Schluessel(Schluessel::Try) => {
                self.weiter();
                return self.probier(pos);
            }
            _ => {
                // Zuweisung? Dann steht vorne ein Ziel (x, p.x, liste[i], SÖWA.x) und danach = oder +=.
                if matches!(token.art, TokenArt::Name(_) | TokenArt::Schluessel(Schluessel::This)) {
                    let anfang = self.i;
                    if let Ok(ziel) = self.primaer() {
                        let op = match &self.aktuell().art {
                            TokenArt::Gleich => Some(None),
                            anderes => rechen_zuweisung(anderes).map(Some),
                        };
                        if let Some(op) = op {
                            self.weiter();
                            let wert = self.ausdruck()?;
                            self.erwarte_oida()?;
                            return Ok(Anweisung { art: AnweisungArt::Zuweisung { ziel, op, wert }, pos });
                        }
                    }
                    // Doch keine Zuweisung: von vorne als Ausdruck lesen.
                    self.i = anfang;
                }
                // "IF x > 3 THEN": gleich sagen, wie es richtig heißt.
                if *self.naechstes() != TokenArt::KlammerAuf {
                    if let Some(fehler) = self.englisch() {
                        return Err(fehler);
                    }
                }
                let ausdruck = self.ausdruck()?;
                self.erwarte_oida()?;
                AnweisungArt::Ausdruck(ausdruck)
            }
        };
        Ok(Anweisung { art, pos })
    }

    /// Das WENN selbst ist schon gelesen.
    fn wenn(&mut self, pos: Position) -> Result<Anweisung, Fehler> {
        let bedingung = self.ausdruck()?;
        self.erwarte_schluessel(Schluessel::Then)?;
        let dann = self.block()?;
        let mut sunst = Vec::new();
        if self.nimm(Schluessel::Else) {
            if self.ist(Schluessel::If) {
                // SUNST WENN ...: gehört zum selben PASST SCHO.
                let innen_pos = self.weiter().pos;
                sunst.push(self.wenn(innen_pos)?);
                return Ok(Anweisung { art: AnweisungArt::Wenn { bedingung, dann, sunst }, pos });
            }
            sunst = self.block()?;
            if self.ist(Schluessel::Else) {
                return Err(Fehler::neu(
                    self.aktuell().pos,
                    format!(
                        "Zwa {} hintaranaund geht ned. Manst vielleicht {} {}?",
                        self.wort(Schluessel::Else),
                        self.wort(Schluessel::Else),
                        self.wort(Schluessel::If)
                    ),
                ));
            }
        }
        self.erwarte_end(pos, Schluessel::If)?;
        Ok(Anweisung { art: AnweisungArt::Wenn { bedingung, dann, sunst }, pos })
    }

    /// Das PROBIER am Anfang einer Anweisung ist schon gelesen. Zwei Formen:
    ///
    ///  - `PROBIER ausdruck OIDA`: Geht es schief, gibt die Funktion den Fehler weiter.
    ///  - `PROBIER [name =] ausdruck ... WONNS SCHIEFGEHT [fehler] ... PASST SCHO`
    fn probier(&mut self, pos: Position) -> Result<Anweisung, Fehler> {
        let mut name = None;
        if let TokenArt::Name(wort) = &self.aktuell().art
            && *self.naechstes() == TokenArt::Gleich
        {
            name = Some(wort.clone());
            self.weiter();
            self.weiter();
        }
        let wert = self.ausdruck()?;
        if self.ist(Schluessel::Oida) {
            if let Some(name) = name {
                return Err(Fehler::neu(
                    pos,
                    format!(
                        "Manst {} {name} = {} ... {}? Oda es fehlt da Block bis {}.",
                        self.wort(Schluessel::Declare),
                        self.wort(Schluessel::Try),
                        self.wort(Schluessel::Oida),
                        self.wort(Schluessel::Catch)
                    ),
                ));
            }
            self.weiter();
            let innen = Ausdruck::neu(AusdruckArt::Probier { innen: Box::new(wert) }, pos);
            return Ok(Anweisung { art: AnweisungArt::Ausdruck(innen), pos });
        }
        let dann = self.block()?;
        if !self.nimm(Schluessel::Catch) {
            return Err(Fehler::neu(
                pos,
                format!(
                    "Noch {} ghört entweda glei a {} (daun geht da {} weida) oda a Block mit {}.",
                    self.wort(Schluessel::Try),
                    self.wort(Schluessel::Oida),
                    self.wort(Schluessel::Error),
                    self.wort(Schluessel::Catch)
                ),
            ));
        }
        let mut fehler_name = None;
        if let TokenArt::Name(wort) = &self.aktuell().art {
            // Ein Name gleich hinter WONNS SCHIEFGEHT ist der Fehler, außer es beginnt damit schon die erste Anweisung.
            let anweisung_beginnt = matches!(
                self.naechstes(),
                TokenArt::Gleich
                    | TokenArt::KlammerAuf
                    | TokenArt::Punkt
                    | TokenArt::EckigAuf
                    | TokenArt::PlusGleich
                    | TokenArt::MinusGleich
                    | TokenArt::MalGleich
                    | TokenArt::DurchGleich
                    | TokenArt::ProzentGleich
            );
            if !anweisung_beginnt {
                fehler_name = Some(wort.clone());
                self.weiter();
            }
        }
        let sunst = self.block()?;
        self.erwarte_end(pos, Schluessel::Try)?;
        Ok(Anweisung {
            art: AnweisungArt::Probier { name, id: 0, wert, dann, fehler_name, fehler_id: 0, sunst },
            pos,
        })
    }

    /// Die Argumente bis zur schließenden Klammer; die öffnende ist schon gelesen.
    fn argumente(&mut self) -> Result<Vec<Ausdruck>, Fehler> {
        let mut argumente = Vec::new();
        if self.aktuell().art == TokenArt::KlammerZu {
            self.weiter();
            return Ok(argumente);
        }
        loop {
            argumente.push(self.ausdruck()?);
            if self.aktuell().art == TokenArt::Komma {
                self.weiter();
            } else {
                self.erwarte(TokenArt::KlammerZu, "a ',' oda a ')'")?;
                return Ok(argumente);
            }
        }
    }

    // --------------------------------------------------------- Ausdrücke

    fn kann_typ_anfangen(&self) -> bool {
        self.typ_hier().is_some()
            || self.ist(Schluessel::List)
            || self.ist(Schluessel::Maybe)
            || self.ist(Schluessel::Null)
            || self.ist(Schluessel::Map)
            || (self.ist(Schluessel::Function) && *self.naechstes() == TokenArt::KlammerAuf)
            || matches!(self.aktuell().art, TokenArt::Name(_))
    }

    fn kann_ausdruck_anfangen(&self) -> bool {
        match &self.aktuell().art {
            TokenArt::Name(_)
            | TokenArt::Zoi(_)
            | TokenArt::Kommazoi(_)
            | TokenArt::Text(_)
            | TokenArt::KlammerAuf
            | TokenArt::EckigAuf
            | TokenArt::Minus => true,
            TokenArt::Schluessel(s) => {
                matches!(
                    s,
                    Schluessel::True
                        | Schluessel::False
                        | Schluessel::Not
                        | Schluessel::Pi
                        | Schluessel::This
                        | Schluessel::Super
                        | Schluessel::Null
                        | Schluessel::Try
                        | Schluessel::Map
                        | Schluessel::Function
                )
                    || s.ist_funktion()
                    || self.typ_hier().is_some()
            }
            _ => false,
        }
    }

    fn ausdruck(&mut self) -> Result<Ausdruck, Fehler> {
        // PROBIER f(): steht ganz vorne und gilt für alles dahinter.
        if self.ist(Schluessel::Try) {
            let pos = self.weiter().pos;
            let innen = self.ausdruck()?;
            return Ok(Ausdruck::neu(AusdruckArt::Probier { innen: Box::new(innen) }, pos));
        }
        // ODA HOID bindet am schwächsten: a ODA b ODA HOID c ist (a ODA b) ODA HOID c.
        let mut links = self.oda()?;
        while self.ist(Schluessel::OrElse) {
            let pos = self.weiter().pos;
            let rechts = self.oda()?;
            links = Ausdruck::neu(AusdruckArt::OdaHoid { links: Box::new(links), rechts: Box::new(rechts) }, pos);
        }
        Ok(links)
    }

    fn oda(&mut self) -> Result<Ausdruck, Fehler> {
        let mut links = self.und()?;
        while self.ist(Schluessel::Or) {
            let pos = self.weiter().pos;
            let rechts = self.und()?;
            links = binaer(BinaerOp::Oda, links, rechts, pos);
        }
        Ok(links)
    }

    fn und(&mut self) -> Result<Ausdruck, Fehler> {
        let mut links = self.ned()?;
        while self.ist(Schluessel::And) {
            let pos = self.weiter().pos;
            let rechts = self.ned()?;
            links = binaer(BinaerOp::Und, links, rechts, pos);
        }
        Ok(links)
    }

    fn ned(&mut self) -> Result<Ausdruck, Fehler> {
        if self.ist(Schluessel::Not) {
            let pos = self.weiter().pos;
            let innen = self.ned()?;
            return Ok(Ausdruck::neu(AusdruckArt::Unaer { op: UnaerOp::Ned, innen: Box::new(innen) }, pos));
        }
        self.vergleich()
    }

    fn vergleichs_op(&self) -> Option<BinaerOp> {
        Some(match self.aktuell().art {
            TokenArt::Gleich | TokenArt::GleichGleich => BinaerOp::Gleich,
            TokenArt::Ungleich => BinaerOp::Ungleich,
            TokenArt::Kleiner => BinaerOp::Kleiner,
            TokenArt::KleinerGleich => BinaerOp::KleinerGleich,
            TokenArt::Groesser => BinaerOp::Groesser,
            TokenArt::GroesserGleich => BinaerOp::GroesserGleich,
            _ => return None,
        })
    }

    fn vergleich(&mut self) -> Result<Ausdruck, Fehler> {
        let links = self.summe()?;
        let Some(op) = self.vergleichs_op() else { return Ok(links) };
        let pos = self.weiter().pos;
        let rechts = self.summe()?;
        if self.vergleichs_op().is_some() {
            return Err(Fehler::neu(
                self.aktuell().pos,
                format!(
                    "Vergleiche kaunst ned aneinaundahängen. Schreib {} dazwischn.",
                    self.wort(Schluessel::And)
                ),
            ));
        }
        Ok(binaer(op, links, rechts, pos))
    }

    fn summe(&mut self) -> Result<Ausdruck, Fehler> {
        let mut links = self.produkt()?;
        loop {
            let op = match self.aktuell().art {
                TokenArt::Plus => BinaerOp::Plus,
                TokenArt::Minus => BinaerOp::Minus,
                _ => return Ok(links),
            };
            let pos = self.weiter().pos;
            let rechts = self.produkt()?;
            links = binaer(op, links, rechts, pos);
        }
    }

    fn produkt(&mut self) -> Result<Ausdruck, Fehler> {
        let mut links = self.unaer()?;
        loop {
            let op = match self.aktuell().art {
                TokenArt::Mal => BinaerOp::Mal,
                TokenArt::Durch => BinaerOp::Durch,
                TokenArt::Prozent => BinaerOp::Rest,
                _ => return Ok(links),
            };
            let pos = self.weiter().pos;
            let rechts = self.unaer()?;
            links = binaer(op, links, rechts, pos);
        }
    }

    fn unaer(&mut self) -> Result<Ausdruck, Fehler> {
        if self.aktuell().art == TokenArt::Minus {
            let pos = self.weiter().pos;
            let innen = self.unaer()?;
            return Ok(Ausdruck::neu(AusdruckArt::Unaer { op: UnaerOp::Minus, innen: Box::new(innen) }, pos));
        }
        self.primaer()
    }

    /// Ein Wert und was direkt daran hängt: Felder (`.x`) und Stellen (`[i]`).
    fn primaer(&mut self) -> Result<Ausdruck, Fehler> {
        let mut ausdruck = self.atom()?;
        loop {
            match self.aktuell().art {
                TokenArt::Punkt => {
                    self.weiter();
                    let (name, pos) = self.name("des Föd oda de Methode")?;
                    let objekt = Box::new(ausdruck);
                    ausdruck = if self.aktuell().art == TokenArt::KlammerAuf {
                        // objekt.name(...): eine Methode
                        self.weiter();
                        let argumente = self.argumente()?;
                        Ausdruck::neu(AusdruckArt::Methode { objekt, name, argumente, nummer: 0 }, pos)
                    } else {
                        Ausdruck::neu(AusdruckArt::Feld { objekt, name, nummer: 0 }, pos)
                    };
                }
                TokenArt::EckigAuf => {
                    let pos = self.weiter().pos;
                    let index = self.ausdruck()?;
                    self.erwarte(TokenArt::EckigZu, "a ']'")?;
                    ausdruck = Ausdruck::neu(
                        AusdruckArt::Stelle { liste: Box::new(ausdruck), index: Box::new(index) },
                        pos,
                    );
                }
                // wert(...): Was vorne steht, ist eine Funktion als Wert, etwa liste[0](x) oder mach()(x).
                TokenArt::KlammerAuf => {
                    let pos = self.weiter().pos;
                    let argumente = self.argumente()?;
                    ausdruck = Ausdruck::neu(AusdruckArt::WertAufruf { funktion: Box::new(ausdruck), argumente }, pos);
                }
                _ => return Ok(ausdruck),
            }
        }
    }

    fn atom(&mut self) -> Result<Ausdruck, Fehler> {
        let token = self.aktuell().clone();
        let pos = token.pos;
        let art = match token.art {
            TokenArt::Zoi(wert) => {
                self.weiter();
                AusdruckArt::Zoi(wert)
            }
            TokenArt::Kommazoi(wert) => {
                self.weiter();
                AusdruckArt::Kommazoi(wert)
            }
            TokenArt::Text(inhalt) => {
                self.weiter();
                AusdruckArt::Text(inhalt)
            }
            TokenArt::Schluessel(Schluessel::True) => {
                self.weiter();
                AusdruckArt::Jona(true)
            }
            TokenArt::Schluessel(Schluessel::False) => {
                self.weiter();
                AusdruckArt::Jona(false)
            }
            TokenArt::Schluessel(Schluessel::Pi) => {
                self.weiter();
                AusdruckArt::Kommazoi(std::f64::consts::PI)
            }
            TokenArt::Schluessel(Schluessel::This) => {
                self.weiter();
                AusdruckArt::Selbst { id: 0 }
            }
            TokenArt::Schluessel(Schluessel::Super) => {
                // DE OIDN.name(...): die Fassung der Elternklasse
                self.weiter();
                self.erwarte(TokenArt::Punkt, "a '.'")?;
                let (name, name_pos) = self.name("de Methode")?;
                self.erwarte(TokenArt::KlammerAuf, "a '('")?;
                let argumente = self.argumente()?;
                return Ok(Ausdruck::neu(AusdruckArt::ElternAufruf { name, argumente, klasse: 0 }, name_pos));
            }
            TokenArt::Schluessel(s) if eingebaut(s).is_some() => {
                // Der Lexer lässt so ein Wort nur durch, wenn die Klammer folgt.
                self.weiter();
                self.erwarte(TokenArt::KlammerAuf, "a '('")?;
                AusdruckArt::Eingebaut { funktion: eingebaut(s).unwrap(), argumente: self.argumente()? }
            }
            TokenArt::Schluessel(Schluessel::Null) => {
                self.weiter();
                AusdruckArt::Nix
            }
            TokenArt::Schluessel(Schluessel::Function) if *self.naechstes() == TokenArt::KlammerAuf => {
                self.weiter();
                self.weiter();
                return self.lambda(pos);
            }
            // Stapel VON ZOI(...), VAZEICHNIS VON KETTN UND ZOI(): ein neuer Wert, der Typ steht dabei.
            TokenArt::Schluessel(Schluessel::Map) => {
                let angabe = self.typ_kern()?;
                self.erwarte(TokenArt::KlammerAuf, "a '('")?;
                AusdruckArt::Erzeug { angabe, argumente: self.argumente()? }
            }
            TokenArt::Name(_) if *self.naechstes() == TokenArt::Schluessel(Schluessel::From) => {
                let angabe = self.typ_kern()?;
                self.erwarte(TokenArt::KlammerAuf, "a '('")?;
                AusdruckArt::Erzeug { angabe, argumente: self.argumente()? }
            }
            TokenArt::Name(name) => {
                self.weiter();
                if self.aktuell().art == TokenArt::KlammerAuf {
                    self.weiter();
                    AusdruckArt::Aufruf { name, argumente: self.argumente()? }
                } else {
                    AusdruckArt::Variable { name, id: 0 }
                }
            }
            TokenArt::KlammerAuf => {
                self.weiter();
                let innen = self.ausdruck()?;
                self.erwarte(TokenArt::KlammerZu, "a ')'")?;
                return Ok(innen);
            }
            TokenArt::EckigAuf => {
                // [a, b, c]: eine neue Liste
                self.weiter();
                let mut elemente = Vec::new();
                if self.aktuell().art != TokenArt::EckigZu {
                    loop {
                        elemente.push(self.ausdruck()?);
                        if self.aktuell().art == TokenArt::Komma {
                            self.weiter();
                        } else {
                            break;
                        }
                    }
                }
                self.erwarte(TokenArt::EckigZu, "a ',' oda a ']'")?;
                AusdruckArt::ListenWert { elemente }
            }
            _ => {
                if let Some(ziel) = self.typ_hier() {
                    // ZOI(x), KETTN(x) ...: Umwandlung
                    self.weiter();
                    self.erwarte(TokenArt::KlammerAuf, "a '('")?;
                    let innen = self.ausdruck()?;
                    self.erwarte(TokenArt::KlammerZu, "a ')'")?;
                    AusdruckArt::Umwandlung { ziel, innen: Box::new(innen) }
                } else if self.ist(Schluessel::Try) {
                    return Err(Fehler::neu(
                        pos,
                        format!(
                            "{p} muass gaunz vuan stehn, zum Beispü: {} x = {p} f() {}",
                            self.wort(Schluessel::Declare),
                            self.wort(Schluessel::Oida),
                            p = self.wort(Schluessel::Try)
                        ),
                    ));
                } else {
                    return Err(self.unerwartet("a Wert"));
                }
            }
        };
        Ok(Ausdruck::neu(art, pos))
    }
}

/// Bei den vier einfachen Typen steht der Typ schon beim Lesen fest. Strukturen
/// und Listen löst erst die Typprüfung auf.
fn einfacher_typ(angabe: &TypAngabe) -> Typ {
    match angabe {
        TypAngabe::Einfach(typ) => *typ,
        _ => Typ::Nix,
    }
}

/// `+=` und Verwandte: welche Rechnung dahintersteckt.
fn rechen_zuweisung(token: &TokenArt) -> Option<BinaerOp> {
    Some(match token {
        TokenArt::PlusGleich => BinaerOp::Plus,
        TokenArt::MinusGleich => BinaerOp::Minus,
        TokenArt::MalGleich => BinaerOp::Mal,
        TokenArt::DurchGleich => BinaerOp::Durch,
        TokenArt::ProzentGleich => BinaerOp::Rest,
        _ => return None,
    })
}

/// Welche eingebaute Funktion zu einem Wort gehört.
fn eingebaut(schluessel: Schluessel) -> Option<Eingebaut> {
    Some(match schluessel {
        Schluessel::Read => Eingebaut::Lies,
        Schluessel::Length => Eingebaut::Laenge,
        Schluessel::Substring => Eingebaut::Stueck,
        Schluessel::Find => Eingebaut::Suche,
        Schluessel::Upper => Eingebaut::Gross,
        Schluessel::Lower => Eingebaut::Klein,
        Schluessel::Trim => Eingebaut::Stutz,
        Schluessel::Replace => Eingebaut::Ersetz,
        Schluessel::Sqrt => Eingebaut::Wurzel,
        Schluessel::Pow => Eingebaut::Hoch,
        Schluessel::Abs => Eingebaut::Betrag,
        Schluessel::Min => Eingebaut::Min,
        Schluessel::Max => Eingebaut::Max,
        Schluessel::Sin => Eingebaut::Sinus,
        Schluessel::Cos => Eingebaut::Kosinus,
        Schluessel::Atan2 => Eingebaut::Winkel,
        Schluessel::Exp => Eingebaut::Exp,
        Schluessel::Floor => Eingebaut::Abrunden,
        Schluessel::Ceil => Eingebaut::Aufrunden,
        Schluessel::Round => Eingebaut::Runden,
        Schluessel::Random => Eingebaut::Zufall,
        Schluessel::Seed => Eingebaut::Samen,
        Schluessel::Add => Eingebaut::Dazu,
        Schluessel::Remove => Eingebaut::Raus,
        Schluessel::ParseInt => Eingebaut::ZoiAus,
        Schluessel::ParseFloat => Eingebaut::KommazoiAus,
        _ => return None,
    })
}

fn binaer(op: BinaerOp, links: Ausdruck, rechts: Ausdruck, pos: Position) -> Ausdruck {
    Ausdruck::neu(AusdruckArt::Binaer { op, links: Box::new(links), rechts: Box::new(rechts) }, pos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::zerlegen;

    fn parse(quelltext: &str) -> Programm {
        let wb = Woerterbuch::eingebaut();
        parsen(zerlegen(quelltext, &wb).unwrap(), &wb).unwrap()
    }

    fn fehler(quelltext: &str) -> Fehler {
        let wb = Woerterbuch::eingebaut();
        parsen(zerlegen(quelltext, &wb).unwrap(), &wb).unwrap_err()
    }

    /// Schreibt einen Ausdruck voll geklammert hin, damit man die Rangfolge sieht.
    fn klammern(a: &Ausdruck) -> String {
        match &a.art {
            AusdruckArt::Zoi(z) => z.to_string(),
            AusdruckArt::Kommazoi(k) => format!("{k:?}"),
            AusdruckArt::Jona(j) => j.to_string(),
            AusdruckArt::Text(t) => format!("{t:?}"),
            AusdruckArt::Variable { name, .. } => name.clone(),
            AusdruckArt::Unaer { op, innen } => {
                format!("({}{})", if *op == UnaerOp::Minus { "-" } else { "NED " }, klammern(innen))
            }
            AusdruckArt::Binaer { op, links, rechts } => {
                format!("({} {} {})", klammern(links), op.zeichen(), klammern(rechts))
            }
            AusdruckArt::Aufruf { name, argumente } => {
                format!("{name}({})", argumente.iter().map(klammern).collect::<Vec<_>>().join(", "))
            }
            AusdruckArt::Eingebaut { funktion, argumente } => {
                format!("{funktion:?}({})", argumente.iter().map(klammern).collect::<Vec<_>>().join(", "))
            }
            AusdruckArt::Umwandlung { ziel, innen } => format!("{ziel:?}({})", klammern(innen)),
            AusdruckArt::Feld { objekt, name, .. } => format!("{}.{name}", klammern(objekt)),
            AusdruckArt::Stelle { liste, index } => format!("{}[{}]", klammern(liste), klammern(index)),
            AusdruckArt::ListenWert { elemente } => {
                format!("[{}]", elemente.iter().map(klammern).collect::<Vec<_>>().join(", "))
            }
            AusdruckArt::Selbst { .. } => "SÖWA".to_string(),
            AusdruckArt::Methode { objekt, name, argumente, .. } => format!(
                "{}.{name}({})",
                klammern(objekt),
                argumente.iter().map(klammern).collect::<Vec<_>>().join(", ")
            ),
            AusdruckArt::ElternAufruf { name, argumente, .. } => {
                format!("OIDN.{name}({})", argumente.iter().map(klammern).collect::<Vec<_>>().join(", "))
            }
            AusdruckArt::Nix => "NIX".to_string(),
            AusdruckArt::Lambda { funktion, kurz: true, .. } => {
                let AnweisungArt::GibZruck { wert: Some(wert) } = &funktion.rumpf[0].art else { panic!() };
                let namen: Vec<&str> = funktion.parameter.iter().map(|p| p.name.as_str()).collect();
                format!("FUNKTIAUN({}) => {}", namen.join(", "), klammern(wert))
            }
            AusdruckArt::Lambda { funktion, .. } => {
                let namen: Vec<&str> = funktion.parameter.iter().map(|p| p.name.as_str()).collect();
                format!("FUNKTIAUN({}) [{} Anweisungen]", namen.join(", "), funktion.rumpf.len())
            }
            AusdruckArt::WertAufruf { funktion, argumente } => {
                let argumente: Vec<String> = argumente.iter().map(klammern).collect();
                format!("{}({})", klammern(funktion), argumente.join(", "))
            }
            AusdruckArt::Erzeug { argumente, .. } => {
                let argumente: Vec<String> = argumente.iter().map(klammern).collect();
                format!("NEU({})", argumente.join(", "))
            }
            AusdruckArt::FunktionsWert { .. } => unreachable!("entsteht erst in der Typprüfung"),
            AusdruckArt::OdaHoid { links, rechts } => format!("({} ODA HOID {})", klammern(links), klammern(rechts)),
            AusdruckArt::Probier { innen } => format!("PROBIER {}", klammern(innen)),
            AusdruckArt::Bau { .. }
            | AusdruckArt::Neu { .. }
            | AusdruckArt::IstNix { .. }
            | AusdruckArt::Sicher { .. } => unreachable!("entsteht erst in der Typprüfung"),
        }
    }

    fn ausdruck(quelltext: &str) -> String {
        let programm = parse(&format!("x = {quelltext} OIDA"));
        match &programm.haupt[0].art {
            AnweisungArt::Zuweisung { wert, op: None, .. } => klammern(wert),
            anderes => panic!("keine Zuweisung: {anderes:?}"),
        }
    }

    #[test]
    fn rangfolge() {
        assert_eq!(ausdruck("1 + 2 * 3"), "(1 + (2 * 3))");
        assert_eq!(ausdruck("(1 + 2) * 3"), "((1 + 2) * 3)");
        assert_eq!(ausdruck("10 - 4 - 3"), "((10 - 4) - 3)");
        assert_eq!(ausdruck("-a * b"), "((-a) * b)");
        assert_eq!(ausdruck("a < b UND NED c ODA d"), "(((a < b) UND (NED c)) ODA d)");
        assert_eq!(ausdruck("a % 3 = 0"), "((a % 3) == 0)");
        assert_eq!(ausdruck("a <> b"), "(a != b)");
    }

    #[test]
    fn aufruf_und_umwandlung() {
        assert_eq!(ausdruck("f(1, g(), \"a\")"), "f(1, g(), \"a\")");
        assert_eq!(ausdruck("KETTN(3) + ZOI(2.5)"), "(Kettn(3) + Zoi(2.5))");
        assert_eq!(ausdruck("WOAHR ODA STIMMT NED"), "(true ODA false)");
    }

    #[test]
    fn deklaration_mit_und_ohne_typ() {
        let p = parse("SOG AU a = 1 OIDA SOG AU b KOMMAZOI = 2 OIDA");
        assert!(matches!(&p.haupt[0].art, AnweisungArt::Deklaration { name, angabe: None, .. } if name == "a"));
        assert!(matches!(
            &p.haupt[1].art,
            AnweisungArt::Deklaration { name, angabe: Some(TypAngabe::Einfach(Typ::Kommazoi)), .. } if name == "b"
        ));
    }

    #[test]
    fn wenn_sunst_wenn() {
        let p = parse(
            "WENN a DAUN SCHREIB(1) OIDA SUNST WENN b DAUN SCHREIB(2) OIDA SUNST SCHREIB(3) OIDA PASST SCHO",
        );
        assert_eq!(p.haupt.len(), 1);
        let AnweisungArt::Wenn { dann, sunst, .. } = &p.haupt[0].art else { panic!() };
        assert_eq!(dann.len(), 1);
        assert_eq!(sunst.len(), 1);
        let AnweisungArt::Wenn { dann, sunst, .. } = &sunst[0].art else { panic!() };
        assert_eq!((dann.len(), sunst.len()), (1, 1));
    }

    #[test]
    fn schleife() {
        let p = parse("SOLONG i < 10 TUA i = i + 1 OIDA PASST SCHO OIDA");
        let AnweisungArt::Solong { rumpf, .. } = &p.haupt[0].art else { panic!() };
        assert!(matches!(&rumpf[0].art, AnweisungArt::Zuweisung { ziel, .. } if klammern(ziel) == "i"));
    }

    #[test]
    fn zaehlschleife() {
        let p = parse("FIA i VON 1 BIS n - 1 SCHRITT 2 TUA SCHREIB(i) OIDA PASST SCHO");
        let AnweisungArt::Fia { name, von, bis, schritt, rumpf, .. } = &p.haupt[0].art else { panic!() };
        assert_eq!(name, "i");
        assert_eq!(klammern(von), "1");
        assert_eq!(klammern(bis), "(n - 1)");
        assert_eq!(klammern(schritt.as_ref().unwrap()), "2");
        assert_eq!(rumpf.len(), 1);

        let p = parse("FIA i VON 0 BIS 9 TUA PASST SCHO OIDA");
        assert!(matches!(&p.haupt[0].art, AnweisungArt::Fia { schritt: None, .. }));
    }

    #[test]
    fn schleife_verlassen() {
        let p = parse("SOLONG WOAHR TUA WEIDA OIDA HAU OB OIDA PASST SCHO");
        let AnweisungArt::Solong { rumpf, .. } = &p.haupt[0].art else { panic!() };
        assert!(matches!(rumpf[0].art, AnweisungArt::Weida));
        assert!(matches!(rumpf[1].art, AnweisungArt::HauOb));
    }

    #[test]
    fn rechnen_und_zuweisen() {
        let p = parse("x += 2 * y OIDA x %= 3 OIDA");
        let AnweisungArt::Zuweisung { ziel, op, wert } = &p.haupt[0].art else { panic!() };
        assert_eq!((klammern(ziel), *op, klammern(wert)), ("x".to_string(), Some(BinaerOp::Plus), "(2 * y)".to_string()));
        let AnweisungArt::Zuweisung { op, wert, .. } = &p.haupt[1].art else { panic!() };
        assert_eq!((*op, klammern(wert)), (Some(BinaerOp::Rest), "3".to_string()));
    }

    #[test]
    fn eingebaute_funktionen() {
        assert_eq!(ausdruck("LÄNGN(s) + WURZL(2.0)"), "(Laenge(s) + Wurzel(2.0))");
        assert_eq!(ausdruck("STICKL(LIES(), 0, 3)"), "Stueck(Lies(), 0, 3)");
        assert_eq!(ausdruck("2 * PI"), format!("(2 * {:?})", std::f64::consts::PI));
        // Ohne Klammer ist es ein normaler Name.
        assert_eq!(ausdruck("winkl + hoch"), "(winkl + hoch)");
    }

    #[test]
    fn struktur_mit_feldern() {
        let p = parse("STRUKTUR Gegner\n  pos Punkt\n  lebn KOMMAZOI OIDA\n  beute LISTN VON LISTN VON ZOI\nPASST SCHO");
        let s = &p.strukturen[0];
        assert_eq!(s.name, "Gegner");
        assert_eq!(s.felder.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["pos", "lebn", "beute"]);
        assert!(matches!(&s.felder[0].angabe, TypAngabe::Name { name, .. } if name == "Punkt"));
        assert_eq!(s.felder[1].angabe, TypAngabe::Einfach(Typ::Kommazoi));
        assert_eq!(
            s.felder[2].angabe,
            TypAngabe::Liste(Box::new(TypAngabe::Liste(Box::new(TypAngabe::Einfach(Typ::Zoi)))))
        );
    }

    #[test]
    fn felder_stellen_und_listen() {
        assert_eq!(ausdruck("a.b.c + liste[i + 1].x"), "(a.b.c + liste[(i + 1)].x)");
        assert_eq!(ausdruck("[1, 2 * 3, f(x)][0]"), "[1, (2 * 3), f(x)][0]");
        assert_eq!(ausdruck("[]"), "[]");
        assert_eq!(ausdruck("Punkt(1, 2).x"), "Punkt(1, 2).x");
        assert_eq!(ausdruck("-p.x"), "(-p.x)");
    }

    #[test]
    fn zuweisung_an_feld_und_stelle() {
        let p = parse("p.x = 1 OIDA liste[i].pos.y += 2 OIDA liste[0] = [] OIDA");
        let ziele: Vec<String> = p
            .haupt
            .iter()
            .map(|a| match &a.art {
                AnweisungArt::Zuweisung { ziel, .. } => klammern(ziel),
                anderes => panic!("keine Zuweisung: {anderes:?}"),
            })
            .collect();
        assert_eq!(ziele, vec!["p.x", "liste[i].pos.y", "liste[0]"]);
        // Ein Vergleich am Anfang bleibt ein Vergleich.
        let p = parse("f(x) == 1 OIDA");
        assert!(matches!(&p.haupt[0].art, AnweisungArt::Ausdruck(_)));
    }

    #[test]
    fn typen_bei_variablen_und_funktionen() {
        let p = parse(
            "SOG AU xs LISTN VON Punkt = [] OIDA\nFUNKTIAUN f(p Punkt, xs LISTN VON ZOI) GIBT ZRUCK LISTN VON Punkt\nPASST SCHO",
        );
        let AnweisungArt::Deklaration { angabe: Some(TypAngabe::Liste(innen)), .. } = &p.haupt[0].art else { panic!() };
        assert!(matches!(&**innen, TypAngabe::Name { name, .. } if name == "Punkt"));
        let f = &p.funktionen[0];
        assert!(matches!(&f.parameter[0].angabe, TypAngabe::Name { .. }));
        assert_eq!(f.parameter[1].angabe, TypAngabe::Liste(Box::new(TypAngabe::Einfach(Typ::Zoi))));
        assert!(matches!(&f.rueckgabe_angabe, Some(TypAngabe::Liste(_))));
    }

    #[test]
    fn fia_jede() {
        let p = parse("FIA JEDE g IN gegner TUA SCHREIB(g) OIDA PASST SCHO");
        let AnweisungArt::FiaJede { name, liste, rumpf, .. } = &p.haupt[0].art else { panic!() };
        assert_eq!((name.as_str(), klammern(liste), rumpf.len()), ("g", "gegner".to_string(), 1));
        assert_eq!(fehler("FIA JEDE g gegner TUA PASST SCHO").meldung, "Do ghört IN her, ned 'gegner'.");
    }

    #[test]
    fn klasse_mit_feldern_und_methoden() {
        let p = parse(
            "KLASSE Gegner ERBT VON Wesen\n  staerke ZOI\n  lebn ZOI = 100 OIDA\n  beute LISTN VON KETTN = []\n\
             \n  FUNKTIAUN treffer(schaden ZOI) GIBT ZRUCK JONA\n    SÖWA.lebn -= schaden OIDA\n    GIB ZRUCK SÖWA.lebn <= 0 OIDA\n  PASST SCHO\n\
             \n  FUNKTIAUN beweg()\n    DE OIDN.beweg() OIDA\n  PASST SCHO\nPASST SCHO",
        );
        let k = &p.klassen[0];
        assert_eq!(k.name, "Gegner");
        assert_eq!(k.eltern.as_ref().map(|(n, _)| n.as_str()), Some("Wesen"));
        assert_eq!(k.felder.iter().map(|f| (f.name.as_str(), f.standard.is_some())).collect::<Vec<_>>(), vec![
            ("staerke", false),
            ("lebn", true),
            ("beute", true)
        ]);
        assert_eq!(k.methoden.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), vec!["treffer", "beweg"]);
        let AnweisungArt::Zuweisung { ziel, op: Some(BinaerOp::Minus), .. } = &k.methoden[0].rumpf[0].art else { panic!() };
        assert_eq!(klammern(ziel), "SÖWA.lebn");
        let AnweisungArt::Ausdruck(a) = &k.methoden[1].rumpf[0].art else { panic!() };
        assert_eq!(klammern(a), "OIDN.beweg()");
    }

    #[test]
    fn methoden_aufrufen() {
        assert_eq!(ausdruck("g.treffer(10) UND liste[0].pos.laenge() > 1"), "(g.treffer(10) UND (liste[0].pos.laenge() > 1))");
        assert_eq!(ausdruck("mach().name"), "mach().name");
        let p = parse("g.treffer(10) OIDA\nSÖWA.schritt() OIDA");
        assert!(matches!(&p.haupt[0].art, AnweisungArt::Ausdruck(a) if klammern(a) == "g.treffer(10)"));
        assert!(matches!(&p.haupt[1].art, AnweisungArt::Ausdruck(a) if klammern(a) == "SÖWA.schritt()"));
    }

    #[test]
    fn nix_und_oda_hoid() {
        // ODA HOID bindet schwächer als ODA; PROBIER steht ganz vorne und gilt für alles dahinter.
        assert_eq!(ausdruck("a ODA b ODA HOID c UND d"), "((a ODA b) ODA HOID (c UND d))");
        assert_eq!(ausdruck("x == NIX ODA y != NIX"), "((x == NIX) ODA (y != NIX))");
        assert_eq!(ausdruck("a ODA HOID b ODA HOID 3"), "((a ODA HOID b) ODA HOID 3)");
        assert_eq!(ausdruck("PROBIER f(1) ODA HOID 2"), "PROBIER (f(1) ODA HOID 2)");
        // "oda hoid" klein geschrieben ist dasselbe Wort.
        assert_eq!(ausdruck("a oda hoid 1"), "(a ODA HOID 1)");
    }

    #[test]
    fn typen_mit_vielleicht_und_fehla() {
        let p = parse(
            "FUNKTIAUN f(a VIELLEICHT ZOI, b LISTN VON VIELLEICHT Punkt) GIBT ZRUCK VIELLEICHT KETTN ODA FEHLA\nPASST SCHO\n\
             FUNKTIAUN g(f FEHLA) GIBT ZRUCK NIX ODA FEHLA\nPASST SCHO\nSOG AU x VIELLEICHT Wesen = NIX OIDA",
        );
        let f = &p.funktionen[0];
        assert!(matches!(&f.parameter[0].angabe, TypAngabe::Vielleicht { innen, .. } if **innen == TypAngabe::Einfach(Typ::Zoi)));
        let TypAngabe::Liste(element) = &f.parameter[1].angabe else { panic!() };
        assert!(matches!(&**element, TypAngabe::Vielleicht { .. }));
        // ODA FEHLA gilt für den ganzen Typ davor.
        let Some(TypAngabe::OderFehler { wert: Some(wert), .. }) = &f.rueckgabe_angabe else { panic!() };
        assert!(matches!(&**wert, TypAngabe::Vielleicht { .. }));
        let g = &p.funktionen[1];
        assert_eq!(g.parameter[0].angabe, TypAngabe::Einfach(Typ::Fehla));
        assert!(matches!(&g.rueckgabe_angabe, Some(TypAngabe::OderFehler { wert: None, .. })));
        let AnweisungArt::Deklaration { angabe: Some(TypAngabe::Vielleicht { .. }), wert, .. } = &p.haupt[0].art else { panic!() };
        assert_eq!(wert.art, AusdruckArt::Nix);
    }

    #[test]
    fn probier_in_beiden_formen() {
        let p = parse(
            "PROBIER x = f(1)\n SCHREIB(x) OIDA\nWONNS SCHIEFGEHT fehla_do\n SCHREIB(fehla_do.meldung) OIDA\nPASST SCHO\n\
             PROBIER g()\nWONNS SCHIEFGEHT\n h() OIDA\nPASST SCHO\nPROBIER g() OIDA\nSOG AU y = PROBIER f(2) OIDA",
        );
        let AnweisungArt::Probier { name, wert, dann, fehler_name, sunst, .. } = &p.haupt[0].art else { panic!() };
        assert_eq!((name.as_deref(), fehler_name.as_deref()), (Some("x"), Some("fehla_do")));
        assert_eq!((klammern(wert), dann.len(), sunst.len()), ("f(1)".to_string(), 1, 1));
        // Ohne Namen; der Aufruf h() ist schon die erste Anweisung, nicht der Name des Fehlers.
        let AnweisungArt::Probier { name: None, fehler_name: None, dann, sunst, .. } = &p.haupt[1].art else { panic!() };
        assert_eq!((dann.len(), sunst.len()), (0, 1));
        // Mit OIDA gleich dahinter: Der Fehler geht weiter.
        assert!(matches!(&p.haupt[2].art, AnweisungArt::Ausdruck(a) if klammern(a) == "PROBIER g()"));
        let AnweisungArt::Deklaration { wert, .. } = &p.haupt[3].art else { panic!() };
        assert_eq!(klammern(wert), "PROBIER f(2)");

        let f = fehler("PROBIER x = f(1) OIDA");
        assert!(f.meldung.starts_with("Manst SOG AU x = PROBIER ... OIDA?"), "{}", f.meldung);
        let f = fehler("PROBIER f(1)\nPASST SCHO");
        assert!(f.meldung.starts_with("Noch PROBIER ghört entweda glei a OIDA"), "{}", f.meldung);
    }

    #[test]
    fn fia_ohne_von() {
        let f = fehler("FIA i BIS 3 TUA PASST SCHO");
        assert_eq!(f.meldung, "Do ghört VON her, ned BIS.");
    }

    #[test]
    fn funktion() {
        let p = parse(
            "FUNKTIAUN addier(a ZOI, b KOMMAZOI) GIBT ZRUCK KOMMAZOI\n  GIB ZRUCK a + b OIDA\nPASST SCHO\naddier(1, 2.0) OIDA",
        );
        let f = &p.funktionen[0];
        assert_eq!(f.name, "addier");
        assert_eq!(f.parameter.iter().map(|p| (p.name.as_str(), p.typ)).collect::<Vec<_>>(), vec![
            ("a", Typ::Zoi),
            ("b", Typ::Kommazoi)
        ]);
        assert_eq!(f.rueckgabe, Typ::Kommazoi);
        assert!(matches!(&f.rumpf[0].art, AnweisungArt::GibZruck { wert: Some(_) }));
        assert!(matches!(&p.haupt[0].art, AnweisungArt::Ausdruck(_)));
    }

    #[test]
    fn funktion_ohne_rueckgabe() {
        let p = parse("FUNKTIAUN servas()\n GIB ZRUCK OIDA\nPASST SCHO");
        assert_eq!(p.funktionen[0].rueckgabe, Typ::Nix);
        assert!(matches!(&p.funktionen[0].rumpf[0].art, AnweisungArt::GibZruck { wert: None }));
    }

    #[test]
    fn fehlendes_oida() {
        let f = fehler("SOG AU x = 5\nSCHREIB(x) OIDA");
        assert_eq!(f.meldung, "Heast, wo is des OIDA?");
        // Der Pfeil zeigt hinter die 5, nicht auf die nächste Zeile.
        assert_eq!(f.pos, Position::neu(1, 13));
    }

    #[test]
    fn fehlendes_oida_am_dateiende() {
        let f = fehler("SCHREIB(1)");
        assert_eq!(f.meldung, "Heast, wo is des OIDA?");
        assert_eq!(f.pos, Position::neu(1, 11));
    }

    #[test]
    fn fehlendes_passt_scho() {
        let f = fehler("SOG AU x = 1 OIDA\nWENN x > 0 DAUN\n  SCHREIB(x) OIDA\n");
        assert_eq!(f.pos, Position::neu(2, 1));
        assert!(f.meldung.contains("Do fehlt a PASST SCHO"), "{}", f.meldung);
    }

    #[test]
    fn fehlendes_daun() {
        let f = fehler("WENN x > 0\n  SCHREIB(x) OIDA PASST SCHO");
        assert_eq!(f.meldung, "Do ghört DAUN her, ned SCHREIB.");
        assert_eq!(f.pos, Position::neu(2, 3));
    }

    #[test]
    fn passt_scho_ohne_anfang() {
        let f = fehler("SCHREIB(1) OIDA PASST SCHO");
        assert!(f.meldung.contains("ghört zu nix"), "{}", f.meldung);
    }

    #[test]
    fn englisches_wort() {
        assert_eq!(fehler("IF x > 3 THEN").meldung, "Des haaßt WENN, ned IF!");
        assert_eq!(fehler("WENN a AND b DAUN").meldung, "Des haaßt UND, ned AND!");
    }

    #[test]
    fn schluesselwort_als_name() {
        let f = fehler("SOG AU WENN = 5 OIDA");
        assert!(f.meldung.contains("scho a Wort von da Sproch"), "{}", f.meldung);
    }

    #[test]
    fn verkettete_vergleiche() {
        let f = fehler("x = 1 < a < 3 OIDA");
        assert!(f.meldung.contains("Schreib UND dazwischn"), "{}", f.meldung);
    }

    #[test]
    fn funktion_in_funktion() {
        let f = fehler("FUNKTIAUN a()\nFUNKTIAUN b()\nPASST SCHO\nPASST SCHO");
        assert_eq!(f.pos, Position::neu(2, 1));
    }

    #[test]
    fn fehlender_wert() {
        let f = fehler("SOG AU x = OIDA");
        assert_eq!(f.meldung, "Do ghört a Wert her, ned OIDA.");
    }
}
