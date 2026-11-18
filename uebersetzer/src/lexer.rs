//! Lexer: zerlegt den Quelltext in Tokens.
//!
//! Erst wird der Text in einzelne Wörter, Zahlen, Texte und Zeichen zerlegt.
//! Dann werden Wortfolgen, die im Wörterbuch stehen, zu einem Schlüsselwort
//! zusammengefasst (`SOG AU`, `PASST SCHO`), der längste Treffer gewinnt.

use crate::fehler::{Fehler, Position};
use crate::woerterbuch::{Schluessel, Woerterbuch, norm, wort_anfang, wort_zeichen};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenArt {
    Schluessel(Schluessel),
    Name(String),
    Zoi(i64),
    Kommazoi(f64),
    Text(String),
    KlammerAuf,
    KlammerZu,
    /// `[` und `]`: Listen
    EckigAuf,
    EckigZu,
    Komma,
    /// `.` zwischen einem Wert und seinem Feld: `punkt.x`
    Punkt,
    Plus,
    Minus,
    Mal,
    Durch,
    Prozent,
    /// `+=`, `-=`, `*=`, `/=`, `%=`: rechnen und gleich wieder zuweisen
    PlusGleich,
    MinusGleich,
    MalGleich,
    DurchGleich,
    ProzentGleich,
    /// `=` (Zuweisung, in Bedingungen auch Vergleich)
    Gleich,
    /// `==`
    GleichGleich,
    /// `=>`: kurze Funktion ohne Namen, `FUNKTIAUN(x ZOI) => x * 2`
    Pfeil,
    /// `!=` oder `<>`
    Ungleich,
    Kleiner,
    KleinerGleich,
    Groesser,
    GroesserGleich,
    DateiEnde,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub art: TokenArt,
    /// Wo das Token anfängt.
    pub pos: Position,
    /// Die Stelle direkt hinter dem Token.
    pub ende: Position,
}

/// Ein Token vor dem Zusammenfassen der Schlüsselwörter.
struct Roh {
    token: Token,
    /// Zwischen dem vorigen Token und diesem steht nur Leerraum.
    klebt: bool,
}

struct Leser<'a> {
    zeichen: Vec<char>,
    i: usize,
    zeile: u32,
    spalte: u32,
    wb: &'a Woerterbuch,
    /// Alle Fehler bisher. Nach einem Fehler wird weitergelesen, damit der
    /// Editor auch den Rest der Datei einfärben kann.
    fehler: Vec<Fehler>,
}

impl Leser<'_> {
    fn pos(&self) -> Position {
        Position::neu(self.zeile, self.spalte)
    }

    fn schau(&self, weiter: usize) -> Option<char> {
        self.zeichen.get(self.i + weiter).copied()
    }

    fn nimm(&mut self) -> Option<char> {
        let c = self.zeichen.get(self.i).copied()?;
        self.i += 1;
        if c == '\n' {
            self.zeile += 1;
            self.spalte = 1;
        } else {
            self.spalte += 1;
        }
        Some(c)
    }

    fn roh(&mut self) -> Vec<Roh> {
        let mut tokens = Vec::new();
        let mut klebt = false;
        while let Some(c) = self.schau(0) {
            if c.is_whitespace() {
                self.nimm();
                continue;
            }
            if c == '#' {
                while self.schau(0).is_some_and(|c| c != '\n') {
                    self.nimm();
                }
                klebt = false;
                continue;
            }
            if c == '/' && self.schau(1) == Some('*') {
                let anfang = self.pos();
                self.nimm();
                self.nimm();
                loop {
                    match self.nimm() {
                        Some('*') if self.schau(0) == Some('/') => {
                            self.nimm();
                            break;
                        }
                        Some(_) => {}
                        None => {
                            self.fehler.push(Fehler::neu(
                                anfang,
                                "Der Kommentar hört nimma auf. Do fehlt des */ am End.",
                            ));
                            break;
                        }
                    }
                }
                klebt = false;
                continue;
            }

            let pos = self.pos();
            let art = if wort_anfang(c) {
                let mut wort = String::new();
                while let Some(c) = self.schau(0).filter(|c| wort_zeichen(*c)) {
                    wort.push(c);
                    self.nimm();
                }
                Ok(TokenArt::Name(wort))
            } else if c.is_ascii_digit() {
                self.zahl(pos)
            } else if c == '"' || c == '\'' {
                self.text(pos)
            } else {
                self.zeichen(pos)
            };
            let art = match art {
                Ok(art) => art,
                Err(fehler) => {
                    // Das kaputte Stück ist schon überlesen; dahinter geht es weiter.
                    self.fehler.push(fehler);
                    klebt = false;
                    continue;
                }
            };
            tokens.push(Roh { token: Token { art, pos, ende: self.pos() }, klebt });
            klebt = true;
        }
        let ende = self.pos();
        tokens.push(Roh { token: Token { art: TokenArt::DateiEnde, pos: ende, ende }, klebt: false });
        tokens
    }

    fn zahl(&mut self, pos: Position) -> Result<TokenArt, Fehler> {
        let mut ziffern = String::new();
        while let Some(c) = self.schau(0).filter(char::is_ascii_digit) {
            ziffern.push(c);
            self.nimm();
        }
        let mut komma = false;
        if self.schau(0) == Some('.') && self.schau(1).is_some_and(|c| c.is_ascii_digit()) {
            komma = true;
            ziffern.push('.');
            self.nimm();
            while let Some(c) = self.schau(0).filter(char::is_ascii_digit) {
                ziffern.push(c);
                self.nimm();
            }
        }
        if self.schau(0).is_some_and(wort_anfang) {
            return Err(Fehler::neu(
                self.pos(),
                format!("Noch da Zoi {ziffern} kaun ned glei a Buchstob kumma. Do ghört a Leerzeichn her."),
            ));
        }
        if komma {
            Ok(TokenArt::Kommazoi(ziffern.parse().expect("Ziffern mit Punkt sind eine Kommazahl")))
        } else {
            match ziffern.parse::<i64>() {
                Ok(wert) => Ok(TokenArt::Zoi(wert)),
                Err(_) => Err(Fehler::neu(
                    pos,
                    format!("De Zoi {ziffern} is ma z'groß, de passt ned in 64 Bit."),
                )),
            }
        }
    }

    fn text(&mut self, pos: Position) -> Result<TokenArt, Fehler> {
        let anfuehrung = self.nimm().unwrap();
        let mut inhalt = String::new();
        loop {
            match self.nimm() {
                None | Some('\n') => {
                    return Err(Fehler::neu(
                        pos,
                        format!("Der Text hört nimma auf. Do fehlt des {anfuehrung} am End."),
                    ));
                }
                Some(c) if c == anfuehrung => break,
                Some('\\') => {
                    let stelle = Position::neu(self.zeile, self.spalte - 1);
                    match self.nimm() {
                        Some('n') => inhalt.push('\n'),
                        Some('t') => inhalt.push('\t'),
                        // Wagenrücklauf: steht in Dateien von Windows vor jedem Zeilenende.
                        Some('r') => inhalt.push('\r'),
                        Some('\\') => inhalt.push('\\'),
                        Some('"') => inhalt.push('"'),
                        Some('\'') => inhalt.push('\''),
                        Some(c) => {
                            // Merken und den Text trotzdem zu Ende lesen.
                            self.fehler.push(Fehler::neu(
                                stelle,
                                format!("\\{c} kenn i ned. Es gibt nur \\n, \\t, \\r, \\\\, \\\" und \\'."),
                            ));
                        }
                        None => {
                            return Err(Fehler::neu(
                                pos,
                                format!("Der Text hört nimma auf. Do fehlt des {anfuehrung} am End."),
                            ));
                        }
                    }
                }
                Some(c) => inhalt.push(c),
            }
        }
        Ok(TokenArt::Text(inhalt))
    }

    fn zeichen(&mut self, pos: Position) -> Result<TokenArt, Fehler> {
        let c = self.nimm().unwrap();
        let zwei = match (c, self.schau(0)) {
            ('=', Some('=')) => Some(TokenArt::GleichGleich),
            ('=', Some('>')) => Some(TokenArt::Pfeil),
            ('!', Some('=')) => Some(TokenArt::Ungleich),
            ('<', Some('>')) => Some(TokenArt::Ungleich),
            ('<', Some('=')) => Some(TokenArt::KleinerGleich),
            ('>', Some('=')) => Some(TokenArt::GroesserGleich),
            ('+', Some('=')) => Some(TokenArt::PlusGleich),
            ('-', Some('=')) => Some(TokenArt::MinusGleich),
            ('*', Some('=')) => Some(TokenArt::MalGleich),
            ('/', Some('=')) => Some(TokenArt::DurchGleich),
            ('%', Some('=')) => Some(TokenArt::ProzentGleich),
            _ => None,
        };
        if let Some(art) = zwei {
            self.nimm();
            return Ok(art);
        }
        Ok(match c {
            '=' => TokenArt::Gleich,
            '<' => TokenArt::Kleiner,
            '>' => TokenArt::Groesser,
            '(' => TokenArt::KlammerAuf,
            ')' => TokenArt::KlammerZu,
            '[' => TokenArt::EckigAuf,
            ']' => TokenArt::EckigZu,
            '.' => TokenArt::Punkt,
            ',' => TokenArt::Komma,
            '+' => TokenArt::Plus,
            '-' => TokenArt::Minus,
            '*' => TokenArt::Mal,
            '/' => TokenArt::Durch,
            '%' => TokenArt::Prozent,
            ';' => {
                return Err(Fehler::neu(
                    pos,
                    format!(
                        "An Strichpunkt brauchst do ned. Bei uns haaßt des {}.",
                        self.wb.wort(Schluessel::Oida)
                    ),
                ));
            }
            _ => {
                return Err(Fehler::neu(pos, format!("Wos soll des Zeichn '{c}' do? Des kenn i ned.")));
            }
        })
    }
}

/// Zerlegt den Quelltext in Tokens. Das letzte Token ist immer `DateiEnde`.
/// Beim ersten Fehler ist Schluss.
pub fn zerlegen(quelltext: &str, wb: &Woerterbuch) -> Result<Vec<Token>, Fehler> {
    let (tokens, mut fehler) = zerlegen_nachsichtig(quelltext, wb);
    if fehler.is_empty() { Ok(tokens) } else { Err(fehler.remove(0)) }
}

/// Wie [`zerlegen`], liest aber nach einem Fehler weiter und gibt alle Tokens
/// zurück, die sich erkennen lassen, dazu alle Fehler. Für den Editor.
pub fn zerlegen_nachsichtig(quelltext: &str, wb: &Woerterbuch) -> (Vec<Token>, Vec<Fehler>) {
    let quelltext = quelltext.strip_prefix('\u{feff}').unwrap_or(quelltext); // BOM vom Windows-Editor
    let mut leser =
        Leser { zeichen: quelltext.chars().collect(), i: 0, zeile: 1, spalte: 1, wb, fehler: Vec::new() };
    let roh = leser.roh();

    // Wortfolgen aus dem Wörterbuch zu Schlüsselwörtern zusammenfassen.
    let mut tokens = Vec::with_capacity(roh.len());
    let mut i = 0;
    while i < roh.len() {
        // Direkt hinter einem Punkt steht immer ein Name (Feld oder Methode),
        // auch wenn das Wort sonst eines der Sprache wäre.
        let nach_punkt = i > 0 && roh[i - 1].token.art == TokenArt::Punkt;
        if let TokenArt::Name(wort) = &roh[i].token.art
            && !nach_punkt
        {
            let mut folge = vec![norm(wort)];
            let mut j = i + 1;
            while j < roh.len() && roh[j].klebt && folge.len() < wb.laengste_phrase() {
                let TokenArt::Name(naechstes) = &roh[j].token.art else { break };
                folge.push(norm(naechstes));
                j += 1;
            }
            // Der längste Treffer gewinnt. Der Name einer eingebauten Funktion
            // zählt aber nur, wenn gleich eine Klammer folgt; sonst wird der
            // nächstkürzere Treffer versucht (PI MOI DAUMEN ohne Klammer ist PI).
            let mut treffer = wb.finde(&folge);
            while let Some((schluessel, anzahl)) = treffer {
                let klammer_folgt = roh.get(i + anzahl).is_some_and(|r| r.token.art == TokenArt::KlammerAuf);
                if !schluessel.ist_funktion() || klammer_folgt {
                    break;
                }
                treffer = if anzahl > 1 { wb.finde(&folge[..anzahl - 1]) } else { None };
            }
            if let Some((schluessel, anzahl)) = treffer {
                tokens.push(Token {
                    art: TokenArt::Schluessel(schluessel),
                    pos: roh[i].token.pos,
                    ende: roh[i + anzahl - 1].token.ende,
                });
                i += anzahl;
                continue;
            }
        }
        tokens.push(roh[i].token.clone());
        i += 1;
    }
    (tokens, leser.fehler)
}

#[cfg(test)]
mod tests {
    use super::{TokenArt, zerlegen, zerlegen_nachsichtig};
    use crate::fehler::{Fehler, Position};
    use crate::woerterbuch::{Schluessel as S, Woerterbuch};
    use TokenArt::*;

    fn arten(quelltext: &str) -> Vec<TokenArt> {
        let wb = Woerterbuch::eingebaut();
        zerlegen(quelltext, &wb).unwrap().into_iter().map(|t| t.art).collect()
    }

    fn fehler(quelltext: &str) -> Fehler {
        let wb = Woerterbuch::eingebaut();
        zerlegen(quelltext, &wb).unwrap_err()
    }

    fn name(n: &str) -> TokenArt {
        Name(n.to_string())
    }

    #[test]
    fn deklaration() {
        assert_eq!(
            arten("SOG AU alter = 17 OIDA"),
            vec![
                Schluessel(S::Declare),
                name("alter"),
                Gleich,
                Zoi(17),
                Schluessel(S::Oida),
                DateiEnde
            ]
        );
    }

    #[test]
    fn mehrteilige_woerter_und_gross_klein() {
        assert_eq!(
            arten("passt  scho Gib\n zruck stimmt ned stimmt"),
            vec![Schluessel(S::End), Schluessel(S::Return), Schluessel(S::False), Schluessel(S::True), DateiEnde]
        );
    }

    #[test]
    fn halbes_schluesselwort_ist_ein_name() {
        // SOG allein und AU allein sind normale Namen.
        assert_eq!(arten("SOG x AU"), vec![name("SOG"), name("x"), name("AU"), DateiEnde]);
    }

    #[test]
    fn kommentar_trennt_wortfolgen() {
        assert_eq!(arten("PASST /* na */ SCHO"), vec![name("PASST"), name("SCHO"), DateiEnde]);
    }

    #[test]
    fn zahlen_und_texte() {
        assert_eq!(
            arten(r#"3.14 42 "Servas\n" 'a"b'"#),
            vec![Kommazoi(3.14), Zoi(42), Text("Servas\n".into()), Text("a\"b".into()), DateiEnde]
        );
    }

    #[test]
    fn zeichen() {
        assert_eq!(
            arten("( ) , + - * / % = == != <> < <= > >="),
            vec![
                KlammerAuf,
                KlammerZu,
                Komma,
                Plus,
                Minus,
                Mal,
                Durch,
                Prozent,
                Gleich,
                GleichGleich,
                Ungleich,
                Ungleich,
                Kleiner,
                KleinerGleich,
                Groesser,
                GroesserGleich,
                DateiEnde
            ]
        );
    }

    #[test]
    fn rechnen_und_zuweisen() {
        assert_eq!(
            arten("x += 1 -= *= /= %= + ="),
            vec![name("x"), PlusGleich, Zoi(1), MinusGleich, MalGleich, DurchGleich, ProzentGleich, Plus, Gleich, DateiEnde]
        );
    }

    #[test]
    fn eingebaute_funktion_nur_mit_klammer() {
        // Mit Klammer ist WINKL die Funktion, ohne ein ganz normaler Name.
        assert_eq!(
            arten("winkl = WINKL (y, x)"),
            vec![
                name("winkl"),
                Gleich,
                Schluessel(S::Atan2),
                KlammerAuf,
                name("y"),
                Komma,
                name("x"),
                KlammerZu,
                DateiEnde
            ]
        );
        // PI MOI DAUMEN ist RUNDN, aber nur mit Klammer; sonst bleibt PI die Zahl.
        assert_eq!(
            arten("PI MOI DAUMEN(PI) PI moi"),
            vec![Schluessel(S::Round), KlammerAuf, Schluessel(S::Pi), KlammerZu, Schluessel(S::Pi), name("moi"), DateiEnde]
        );
        // Wörter, die keine Funktionen sind, gelten immer.
        assert_eq!(arten("HAU OB fia"), vec![Schluessel(S::Break), Schluessel(S::For), DateiEnde]);
    }

    #[test]
    fn felder_und_listen() {
        assert_eq!(
            arten("p.x = liste[0].y + 1.5"),
            vec![
                name("p"),
                Punkt,
                name("x"),
                Gleich,
                name("liste"),
                EckigAuf,
                Zoi(0),
                EckigZu,
                Punkt,
                name("y"),
                Plus,
                Kommazoi(1.5),
                DateiEnde
            ]
        );
        assert_eq!(
            arten("HAU EINE(xs, 1) STRUKTUR LISTN VON FIA JEDE x IN xs"),
            vec![
                Schluessel(S::Add),
                KlammerAuf,
                name("xs"),
                Komma,
                Zoi(1),
                KlammerZu,
                Schluessel(S::Struct),
                Schluessel(S::List),
                Schluessel(S::From),
                Schluessel(S::For),
                Schluessel(S::Each),
                name("x"),
                Schluessel(S::In),
                name("xs"),
                DateiEnde
            ]
        );
    }

    #[test]
    fn hinter_dem_punkt_ist_alles_ein_name() {
        assert_eq!(
            arten("x.von.ende.längn(1) SÖWA.lebn DE OIDN.schritt()"),
            vec![
                name("x"),
                Punkt,
                name("von"),
                Punkt,
                name("ende"),
                Punkt,
                name("längn"),
                KlammerAuf,
                Zoi(1),
                KlammerZu,
                Schluessel(S::This),
                Punkt,
                name("lebn"),
                Schluessel(S::Super),
                Punkt,
                name("schritt"),
                KlammerAuf,
                KlammerZu,
                DateiEnde
            ]
        );
        assert_eq!(arten("KLASSE A ERBT VON B"), vec![Schluessel(S::Class), name("A"), Schluessel(S::Extends), name("B"), DateiEnde]);
    }

    #[test]
    fn kommentare_werden_ueberlesen() {
        assert_eq!(arten("# nix\n1 /* a\nb */ 2 # no wos"), vec![Zoi(1), Zoi(2), DateiEnde]);
    }

    #[test]
    fn umlaute_in_namen() {
        assert_eq!(arten("größe"), vec![name("größe"), DateiEnde]);
    }

    #[test]
    fn positionen() {
        let wb = Woerterbuch::eingebaut();
        let tokens = zerlegen("SOG AU x = 5\n  OIDA", &wb).unwrap();
        assert_eq!(tokens[0].pos, Position::neu(1, 1));
        assert_eq!(tokens[0].ende, Position::neu(1, 7));
        assert_eq!(tokens[3].pos, Position::neu(1, 12));
        assert_eq!(tokens[3].ende, Position::neu(1, 13));
        assert_eq!(tokens[4].pos, Position::neu(2, 3));
    }

    #[test]
    fn text_ohne_ende() {
        let f = fehler("SOG AU x = \"offen\nOIDA");
        assert_eq!(f.pos, Position::neu(1, 12));
        assert!(f.meldung.contains("hört nimma auf"), "{}", f.meldung);
    }

    #[test]
    fn strichpunkt() {
        let f = fehler("SOG AU x = 5;");
        assert_eq!(f.pos, Position::neu(1, 13));
        assert!(f.meldung.contains("Strichpunkt"), "{}", f.meldung);
    }

    #[test]
    fn zu_grosse_zahl() {
        let f = fehler("99999999999999999999");
        assert!(f.meldung.contains("z'groß"), "{}", f.meldung);
    }

    #[test]
    fn nachsichtig_liest_nach_fehlern_weiter() {
        let wb = Woerterbuch::eingebaut();
        let (tokens, fehler) = zerlegen_nachsichtig("x = \"offen\ny = 5 ? 3; z = \"a\\qb\" OIDA", &wb);
        let arten: Vec<TokenArt> = tokens.into_iter().map(|t| t.art).collect();
        assert_eq!(
            arten,
            vec![
                name("x"),
                Gleich,
                name("y"),
                Gleich,
                Zoi(5),
                Zoi(3),
                name("z"),
                Gleich,
                Text("ab".into()),
                Schluessel(S::Oida),
                DateiEnde
            ]
        );
        assert_eq!(
            fehler.iter().map(|f| (f.pos.zeile, f.pos.spalte)).collect::<Vec<_>>(),
            vec![(1, 5), (2, 7), (2, 10), (2, 18)]
        );
    }

    #[test]
    fn unbekanntes_zeichen() {
        let f = fehler("x = 5 ? 3");
        assert_eq!(f.pos, Position::neu(1, 7));
    }
}
