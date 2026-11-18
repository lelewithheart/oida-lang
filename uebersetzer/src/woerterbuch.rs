//! Das Wörterbuch: welche Dialekt-Wörter für welches Schlüsselwort stehen.
//!
//! Dasselbe Format wie `woerterbuch.txt` von OIDA-SQL:
//! `FESTES WORT = DIALEKT, VARIANTE2`, mehrere Wörter rechts sind erlaubt,
//! `#` leitet einen Kommentar ein. Beim Lesen gewinnt der längste Treffer.

use std::collections::{HashMap, HashSet};

/// Die Schlüsselwörter, die der Compiler kennt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Schluessel {
    Oida,
    Declare,
    If,
    Then,
    Else,
    While,
    Do,
    End,
    Function,
    Returns,
    Return,
    Print,
    And,
    Or,
    Not,
    True,
    False,
    Int,
    Float,
    Boolean,
    Text,
    // Schleifen
    For,
    From,
    To,
    Step,
    Break,
    Continue,
    // Eingebaute Funktionen (siehe [`Schluessel::ist_funktion`])
    Read,
    Length,
    Substring,
    Find,
    Upper,
    Lower,
    Trim,
    Replace,
    Sqrt,
    Pow,
    Abs,
    Min,
    Max,
    Sin,
    Cos,
    Atan2,
    Exp,
    Floor,
    Ceil,
    Round,
    Random,
    Seed,
    // Feste Werte
    Pi,
    // Strukturen und Listen
    Struct,
    List,
    Each,
    In,
    Add,
    Remove,
}

impl Schluessel {
    /// Alle Schlüsselwörter mit dem festen Wort, das links im Wörterbuch steht.
    pub const ALLE: [(Schluessel, &'static str); 56] = [
        (Schluessel::Oida, "OIDA"),
        (Schluessel::Declare, "DECLARE"),
        (Schluessel::If, "IF"),
        (Schluessel::Then, "THEN"),
        (Schluessel::Else, "ELSE"),
        (Schluessel::While, "WHILE"),
        (Schluessel::Do, "DO"),
        (Schluessel::End, "END"),
        (Schluessel::Function, "FUNCTION"),
        (Schluessel::Returns, "RETURNS"),
        (Schluessel::Return, "RETURN"),
        (Schluessel::Print, "PRINT"),
        (Schluessel::And, "AND"),
        (Schluessel::Or, "OR"),
        (Schluessel::Not, "NOT"),
        (Schluessel::True, "TRUE"),
        (Schluessel::False, "FALSE"),
        (Schluessel::Int, "INT"),
        (Schluessel::Float, "FLOAT"),
        (Schluessel::Boolean, "BOOLEAN"),
        (Schluessel::Text, "TEXT"),
        (Schluessel::For, "FOR"),
        (Schluessel::From, "FROM"),
        (Schluessel::To, "TO"),
        (Schluessel::Step, "STEP"),
        (Schluessel::Break, "BREAK"),
        (Schluessel::Continue, "CONTINUE"),
        (Schluessel::Read, "READ"),
        (Schluessel::Length, "LENGTH"),
        (Schluessel::Substring, "SUBSTRING"),
        (Schluessel::Find, "FIND"),
        (Schluessel::Upper, "UPPER"),
        (Schluessel::Lower, "LOWER"),
        (Schluessel::Trim, "TRIM"),
        (Schluessel::Replace, "REPLACE"),
        (Schluessel::Sqrt, "SQRT"),
        (Schluessel::Pow, "POW"),
        (Schluessel::Abs, "ABS"),
        (Schluessel::Min, "MIN"),
        (Schluessel::Max, "MAX"),
        (Schluessel::Sin, "SIN"),
        (Schluessel::Cos, "COS"),
        (Schluessel::Atan2, "ATAN2"),
        (Schluessel::Exp, "EXP"),
        (Schluessel::Floor, "FLOOR"),
        (Schluessel::Ceil, "CEIL"),
        (Schluessel::Round, "ROUND"),
        (Schluessel::Random, "RANDOM"),
        (Schluessel::Seed, "SEED"),
        (Schluessel::Pi, "PI"),
        (Schluessel::Struct, "STRUCT"),
        (Schluessel::List, "LIST"),
        (Schluessel::Each, "EACH"),
        (Schluessel::In, "IN"),
        (Schluessel::Add, "ADD"),
        (Schluessel::Remove, "REMOVE"),
    ];

    /// Ist das der Name einer eingebauten Funktion (LÄNGN, WURZL ...)?
    ///
    /// Solche Wörter gelten nur als Wort der Sprache, wenn gleich eine
    /// Klammer folgt. Ohne Klammer bleiben sie normale Namen, damit man eine
    /// Variable ruhig `winkl` oder `hoch` nennen kann.
    pub fn ist_funktion(self) -> bool {
        use Schluessel::*;
        matches!(
            self,
            Read | Length
                | Substring
                | Find
                | Upper
                | Lower
                | Trim
                | Replace
                | Sqrt
                | Pow
                | Abs
                | Min
                | Max
                | Sin
                | Cos
                | Atan2
                | Exp
                | Floor
                | Ceil
                | Round
                | Random
                | Seed
                | Add
                | Remove
        )
    }

    /// Das feste Wort (linke Seite im Wörterbuch).
    pub fn fest(self) -> &'static str {
        Self::ALLE.iter().find(|(s, _)| *s == self).map(|(_, w)| *w).unwrap()
    }

    fn aus_festem_wort(wort: &str) -> Option<Schluessel> {
        Self::ALLE.iter().find(|(_, w)| *w == wort).map(|(s, _)| *s)
    }
}

/// Eine Wortfolge aus dem Wörterbuch (meist nur ein Wort).
#[derive(Debug, Clone)]
struct Phrase {
    woerter: Vec<String>, // normalisiert (Großbuchstaben)
    schluessel: Schluessel,
}

#[derive(Debug, Clone)]
pub struct Woerterbuch {
    /// Erstes Wort (normalisiert) -> alle Phrasen, die so anfangen, längste zuerst.
    phrasen: HashMap<String, Vec<Phrase>>,
    /// Alle Varianten je Schlüsselwort, so wie sie in der Datei stehen.
    varianten: HashMap<Schluessel, Vec<String>>,
    laengste: usize,
    /// Woher das Wörterbuch kommt (für `oida --woerter`).
    pub herkunft: String,
}

/// Das Wörterbuch, das beim Bauen in den Compiler eingebaut wird.
pub const EINGEBAUT: &str = include_str!("../../woerterbuch.txt");

/// Groß-/Kleinschreibung ist bei Schlüsselwörtern egal.
pub fn norm(wort: &str) -> String {
    wort.to_uppercase()
}

pub fn wort_anfang(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

pub fn wort_zeichen(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Woerterbuch {
    /// Das eingebaute Wörterbuch.
    pub fn eingebaut() -> Woerterbuch {
        Woerterbuch::laden(EINGEBAUT, "eingebaut")
            .unwrap_or_else(|e| panic!("Das eingebaute Wörterbuch ist kaputt: {e}"))
    }

    /// Liest ein Wörterbuch. `herkunft` ist der Dateiname für Fehlermeldungen.
    pub fn laden(inhalt: &str, herkunft: &str) -> Result<Woerterbuch, String> {
        let inhalt = inhalt.strip_prefix('\u{feff}').unwrap_or(inhalt); // BOM vom Windows-Editor
        let mut varianten: HashMap<Schluessel, Vec<String>> = HashMap::new();
        // Wortfolge (normalisiert) -> wem sie gehört
        let mut vergeben: HashMap<String, Schluessel> = HashMap::new();
        let mut phrasen: HashMap<String, Vec<Phrase>> = HashMap::new();
        let mut hat_variante: HashSet<Schluessel> = HashSet::new();

        let mut eintragen = |schluessel: Schluessel, variante: &str, ort: &str| -> Result<(), String> {
            let woerter = woerter(variante, ort)?;
            let schluessel_text = woerter.join(" ");
            if let Some(vorher) = vergeben.get(&schluessel_text) {
                if *vorher == schluessel {
                    return Ok(());
                }
                return Err(format!(
                    "{ort}: '{variante}' steht schon bei {} und kann nicht auch {} heißen.",
                    vorher.fest(),
                    schluessel.fest()
                ));
            }
            vergeben.insert(schluessel_text, schluessel);
            phrasen.entry(woerter[0].clone()).or_default().push(Phrase { woerter, schluessel });
            varianten.entry(schluessel).or_default().push(variante.to_string());
            Ok(())
        };

        for (nr, roh) in inhalt.lines().enumerate() {
            let ort = format!("{herkunft}, Zeile {}", nr + 1);
            let zeile = roh.trim();
            if zeile.is_empty() || zeile.starts_with('#') {
                continue;
            }
            let Some((links, rechts)) = zeile.split_once('=') else {
                return Err(format!("{ort}: '=' fehlt: {zeile}"));
            };
            let fest = norm(links.trim());
            let Some(schluessel) = Schluessel::aus_festem_wort(&fest) else {
                let bekannt: Vec<&str> = Schluessel::ALLE.iter().map(|(_, w)| *w).collect();
                return Err(format!(
                    "{ort}: Das Wort '{}' links vom '=' kennt der Compiler nicht. Erlaubt sind: {}",
                    links.trim(),
                    bekannt.join(", ")
                ));
            };
            for variante in rechts.split(',') {
                let variante = variante.split_whitespace().collect::<Vec<_>>().join(" ");
                if variante.is_empty() {
                    continue;
                }
                eintragen(schluessel, &variante, &ort)?;
                hat_variante.insert(schluessel);
            }
        }

        // Was fehlt oder rechts leer ist, heißt so wie das feste Wort.
        for (schluessel, fest) in Schluessel::ALLE {
            if !hat_variante.contains(&schluessel) {
                eintragen(schluessel, fest, &format!("{herkunft}, Wort {fest} (ohne Übersetzung)"))?;
            }
        }

        let mut laengste = 1;
        for liste in phrasen.values_mut() {
            liste.sort_by(|a, b| b.woerter.len().cmp(&a.woerter.len()));
            laengste = laengste.max(liste[0].woerter.len());
        }
        Ok(Woerterbuch { phrasen, varianten, laengste, herkunft: herkunft.to_string() })
    }

    /// Das Wort, das in Meldungen für ein Schlüsselwort verwendet wird
    /// (die erste Variante aus dem Wörterbuch).
    pub fn wort(&self, schluessel: Schluessel) -> &str {
        &self.varianten[&schluessel][0]
    }

    /// Alle Varianten für ein Schlüsselwort.
    pub fn varianten(&self, schluessel: Schluessel) -> &[String] {
        &self.varianten[&schluessel]
    }

    /// Aus wie vielen Wörtern die längste Phrase besteht.
    pub fn laengste_phrase(&self) -> usize {
        self.laengste
    }

    /// Sucht die längste Phrase, mit der `folge` (normalisierte Wörter) beginnt.
    /// Gibt das Schlüsselwort zurück und aus wie vielen Wörtern es besteht.
    pub fn finde(&self, folge: &[String]) -> Option<(Schluessel, usize)> {
        let kandidaten = self.phrasen.get(folge.first()?)?;
        kandidaten
            .iter()
            .find(|p| p.woerter.len() <= folge.len() && p.woerter[..] == folge[..p.woerter.len()])
            .map(|p| (p.schluessel, p.woerter.len()))
    }

    /// Ist `name` eines der festen Wörter (IF, WHILE ...), das aber im
    /// Wörterbuch anders heißt? Für den Hinweis "Des haaßt WENN, ned IF!".
    pub fn festes_wort(&self, name: &str) -> Option<Schluessel> {
        let schluessel = Schluessel::aus_festem_wort(&norm(name))?;
        let selbst_erlaubt = self.varianten[&schluessel].iter().any(|v| norm(v) == norm(name));
        if selbst_erlaubt { None } else { Some(schluessel) }
    }
}

/// Zerlegt einen Eintrag in normalisierte Wörter und prüft, dass es nur Wörter sind.
fn woerter(eintrag: &str, ort: &str) -> Result<Vec<String>, String> {
    let mut liste = Vec::new();
    for wort in eintrag.split_whitespace() {
        let mut zeichen = wort.chars();
        let anfang_ok = zeichen.next().is_some_and(wort_anfang);
        if !anfang_ok || !zeichen.all(wort_zeichen) {
            return Err(format!(
                "{ort}: '{eintrag}' darf nur aus Wörtern bestehen (Buchstaben, Ziffern, _), gefunden: {wort}"
            ));
        }
        liste.push(norm(wort));
    }
    if liste.is_empty() {
        return Err(format!("{ort}: hier fehlt ein Wort."));
    }
    Ok(liste)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folge(text: &str) -> Vec<String> {
        text.split_whitespace().map(norm).collect()
    }

    #[test]
    fn eingebautes_woerterbuch_ist_in_ordnung() {
        let wb = Woerterbuch::eingebaut();
        assert_eq!(wb.wort(Schluessel::If), "WENN");
        assert_eq!(wb.wort(Schluessel::Oida), "OIDA");
        assert_eq!(wb.finde(&folge("waun x")), Some((Schluessel::If, 1)));
    }

    #[test]
    fn laengster_treffer_gewinnt() {
        let wb = Woerterbuch::eingebaut();
        // STIMMT allein ist TRUE, STIMMT NED ist FALSE.
        assert_eq!(wb.finde(&folge("STIMMT")), Some((Schluessel::True, 1)));
        assert_eq!(wb.finde(&folge("stimmt ned")), Some((Schluessel::False, 2)));
        assert_eq!(wb.finde(&folge("GIB ZRUCK x")), Some((Schluessel::Return, 2)));
        assert_eq!(wb.finde(&folge("GIBT ZRUCK ZOI")), Some((Schluessel::Returns, 2)));
        // GIB allein ist kein Schlüsselwort.
        assert_eq!(wb.finde(&folge("GIB")), None);
    }

    #[test]
    fn neue_woerter_von_schritt_1() {
        let wb = Woerterbuch::eingebaut();
        assert_eq!(wb.finde(&folge("HAU OB")), Some((Schluessel::Break, 2)));
        assert_eq!(wb.finde(&folge("fia i von")), Some((Schluessel::For, 1)));
        assert_eq!(wb.finde(&folge("LÄNGN")), Some((Schluessel::Length, 1)));
        // PI MOI DAUMEN ist RUNDN, PI allein die Zahl.
        assert_eq!(wb.finde(&folge("PI MOI DAUMEN")), Some((Schluessel::Round, 3)));
        assert_eq!(wb.finde(&folge("PI")), Some((Schluessel::Pi, 1)));
        assert!(Schluessel::Round.ist_funktion());
        assert!(!Schluessel::Pi.ist_funktion());
        assert!(!Schluessel::For.ist_funktion());
    }

    #[test]
    fn eigene_woerter() {
        let wb = Woerterbuch::laden("IF = FOIS\nOIDA = HAWARA\n", "test").unwrap();
        assert_eq!(wb.finde(&folge("fois")), Some((Schluessel::If, 1)));
        assert_eq!(wb.finde(&folge("hawara")), Some((Schluessel::Oida, 1)));
        assert_eq!(wb.finde(&folge("oida")), None);
        // Was nicht drinsteht, heißt wie das feste Wort.
        assert_eq!(wb.finde(&folge("while")), Some((Schluessel::While, 1)));
    }

    #[test]
    fn rechts_leer_heisst_festes_wort() {
        let wb = Woerterbuch::laden("IF =\n", "test").unwrap();
        assert_eq!(wb.wort(Schluessel::If), "IF");
    }

    #[test]
    fn doppeltes_wort_wird_gemeldet() {
        let fehler = Woerterbuch::laden("IF = WENN\nWHILE = WENN\n", "test").unwrap_err();
        assert!(fehler.contains("Zeile 2"), "{fehler}");
        assert!(fehler.contains("steht schon bei IF"), "{fehler}");
    }

    #[test]
    fn unbekanntes_festes_wort_wird_gemeldet() {
        let fehler = Woerterbuch::laden("SELECT = GIB MA\n", "test").unwrap_err();
        assert!(fehler.contains("SELECT"), "{fehler}");
    }

    #[test]
    fn fehlendes_gleich_wird_gemeldet() {
        let fehler = Woerterbuch::laden("IF WENN\n", "test").unwrap_err();
        assert!(fehler.contains("'=' fehlt"), "{fehler}");
    }

    #[test]
    fn hinweis_auf_festes_wort() {
        let wb = Woerterbuch::eingebaut();
        assert_eq!(wb.festes_wort("if"), Some(Schluessel::If));
        assert_eq!(wb.festes_wort("oida"), None); // OIDA heißt wirklich OIDA
        assert_eq!(wb.festes_wort("alter"), None);
    }
}
