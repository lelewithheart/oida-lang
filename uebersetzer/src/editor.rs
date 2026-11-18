//! Schnittstelle für Editoren (die VS-Code-Erweiterung in `vscode/`).
//!
//! - [`untersuche`] liefert für einen Quelltext als JSON: alle Fehler, die
//!   Art jedes Wortes (zum Einfärben) und die Wörter aus dem Wörterbuch (für
//!   Vorschläge). Der Editor fragt also immer den Compiler und kennt die
//!   Sprache nicht selbst; so stimmt alles auch mit einem eigenen Wörterbuch.
//! - [`dateien`] erzeugt die festen Dateien der Erweiterung aus dem
//!   Wörterbuch: Grammatik (Einfärben ohne Compiler), Einrückregeln, Schnipsel.

use std::fmt::Write;

use crate::fehler::{Fehler, Position};
use crate::lexer::{self, Token, TokenArt};
use crate::woerterbuch::{Schluessel, Woerterbuch};
use crate::{parser, typen};

/// Die Arten, als die ein Wort eingefärbt wird. Die Nummer steht im JSON;
/// die Reihenfolge muss zu `ARTEN` in `vscode/extension.js` passen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art {
    Schluesselwort = 0,
    Typ = 1,
    Funktion = 2,
    Variable = 3,
    Zahl = 4,
    Text = 5,
    Wahrheitswert = 6,
    Endwort = 7,
}

fn art_von_schluessel(schluessel: Schluessel) -> Art {
    match schluessel {
        Schluessel::Oida => Art::Endwort,
        Schluessel::Int | Schluessel::Float | Schluessel::Boolean | Schluessel::Text => Art::Typ,
        Schluessel::True | Schluessel::False => Art::Wahrheitswert,
        Schluessel::Pi => Art::Zahl,
        s if s.ist_funktion() => Art::Funktion,
        _ => Art::Schluesselwort,
    }
}

/// Ein Text als JSON-Zeichenkette, mit Anführungszeichen.
fn json_text(text: &str) -> String {
    let mut aus = String::with_capacity(text.len() + 2);
    aus.push('"');
    for c in text.chars() {
        match c {
            '"' => aus.push_str("\\\""),
            '\\' => aus.push_str("\\\\"),
            '\n' => aus.push_str("\\n"),
            '\r' => aus.push_str("\\r"),
            '\t' => aus.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(aus, "\\u{:04x}", c as u32);
            }
            c => aus.push(c),
        }
    }
    aus.push('"');
    aus
}

/// Rechnet Positionen des Compilers (Zeile und Spalte ab 1, in Zeichen) in
/// die des Editors um (ab 0, Spalte in UTF-16-Einheiten).
struct Umrechner<'a> {
    zeilen: Vec<&'a str>,
}

impl Umrechner<'_> {
    fn spalte(&self, pos: Position) -> usize {
        let zeile = self.zeilen.get(pos.zeile as usize - 1).copied().unwrap_or("");
        zeile.chars().take(pos.spalte as usize - 1).map(char::len_utf16).sum()
    }

    /// Zeile, Spalte und Länge eines Bereichs. Geht er über mehrere Zeilen,
    /// zählt nur der Teil in der ersten.
    fn bereich(&self, von: Position, bis: Position) -> (usize, usize, usize) {
        let anfang = self.spalte(von);
        let ende = if bis.zeile == von.zeile {
            self.spalte(bis)
        } else {
            let zeile = self.zeilen.get(von.zeile as usize - 1).copied().unwrap_or("");
            zeile.trim_end_matches('\r').chars().map(char::len_utf16).sum()
        };
        (von.zeile as usize - 1, anfang, ende.saturating_sub(anfang).max(1))
    }
}

/// Untersucht einen Quelltext für den Editor. Ergebnis als JSON:
///
/// ```text
/// {
///   "woerterbuch": "woher das Wörterbuch kommt",
///   "fehler":  [ { "zeile": 0, "spalte": 4, "laenge": 3, "meldung": "..." } ],
///   "tokens":  [ [zeile, spalte, laenge, art], ... ],
///   "woerter": [ { "wort": "WENN", "fest": "IF", "art": 0 }, ... ]
/// }
/// ```
///
/// Zeilen und Spalten fangen bei 0 an, Spalten und Längen sind in
/// UTF-16-Einheiten (so zählt VS Code). `art` ist die Nummer aus [`Art`].
pub fn untersuche(quelltext: &str, wb: &Woerterbuch) -> String {
    let quelltext = quelltext.strip_prefix('\u{feff}').unwrap_or(quelltext);
    let (tokens, mut fehler) = lexer::zerlegen_nachsichtig(quelltext, wb);
    if fehler.is_empty() {
        match parser::parsen(tokens.clone(), wb) {
            Err(f) => fehler.push(f),
            Ok(mut programm) => {
                if let Err(alle) = typen::pruefen(&mut programm, wb) {
                    fehler = alle;
                }
            }
        }
    }
    let um = Umrechner { zeilen: quelltext.split('\n').collect() };

    let mut aus = String::from("{");
    let _ = write!(aus, "\"woerterbuch\":{},", json_text(&wb.herkunft));

    aus.push_str("\"fehler\":[");
    for (i, f) in fehler.iter().enumerate() {
        let (zeile, spalte, laenge) = fehler_bereich(f, &tokens, &um);
        let komma = if i > 0 { "," } else { "" };
        let _ = write!(
            aus,
            "{komma}{{\"zeile\":{zeile},\"spalte\":{spalte},\"laenge\":{laenge},\"meldung\":{}}}",
            json_text(&f.meldung)
        );
    }

    aus.push_str("],\"tokens\":[");
    let mut erstes = true;
    for (i, token) in tokens.iter().enumerate() {
        let art = match &token.art {
            TokenArt::Schluessel(s) => art_von_schluessel(*s),
            TokenArt::Name(_) => {
                let aufruf = tokens.get(i + 1).is_some_and(|t| t.art == TokenArt::KlammerAuf);
                if aufruf { Art::Funktion } else { Art::Variable }
            }
            TokenArt::Zoi(_) | TokenArt::Kommazoi(_) => Art::Zahl,
            TokenArt::Text(_) => Art::Text,
            _ => continue,
        };
        if token.pos.zeile != token.ende.zeile {
            // Geht über mehrere Zeilen (PASST <Zeilenumbruch> SCHO): das kann
            // der Editor nicht als ein Stück einfärben, dort greift die Grammatik.
            continue;
        }
        let (zeile, spalte, laenge) = um.bereich(token.pos, token.ende);
        let komma = if erstes { "" } else { "," };
        erstes = false;
        let _ = write!(aus, "{komma}[{zeile},{spalte},{laenge},{}]", art as u8);
    }

    aus.push_str("],\"woerter\":[");
    let mut erstes = true;
    for (schluessel, fest) in Schluessel::ALLE {
        for variante in wb.varianten(schluessel) {
            let komma = if erstes { "" } else { "," };
            erstes = false;
            let _ = write!(
                aus,
                "{komma}{{\"wort\":{},\"fest\":{},\"art\":{}}}",
                json_text(variante),
                json_text(fest),
                art_von_schluessel(schluessel) as u8
            );
        }
    }
    aus.push_str("]}");
    aus
}

/// Was der Editor für einen Fehler unterstreicht: das Token an der Stelle;
/// steht dort keines (fehlendes OIDA), das Token direkt davor.
fn fehler_bereich(fehler: &Fehler, tokens: &[Token], um: &Umrechner) -> (usize, usize, usize) {
    let echt = |t: &&Token| t.art != TokenArt::DateiEnde;
    let token = tokens
        .iter()
        .filter(echt)
        .find(|t| t.pos == fehler.pos)
        .or_else(|| tokens.iter().filter(echt).find(|t| t.ende == fehler.pos));
    match token {
        Some(token) => um.bereich(token.pos, token.ende),
        None => (fehler.pos.zeile as usize - 1, um.spalte(fehler.pos), 1),
    }
}

// ------------------------------------------------------ Dateien für VS Code

/// Buchstabe, Ziffer oder `_` (für reguläre Ausdrücke).
const WORTZEICHEN: &str = r"[\p{L}\p{N}_]";

/// Alle Wortfolgen aus dem Wörterbuch, die längsten zuerst.
fn alle_phrasen(wb: &Woerterbuch) -> Vec<(Schluessel, String)> {
    let mut phrasen: Vec<(Schluessel, String)> = Schluessel::ALLE
        .iter()
        .flat_map(|(s, _)| wb.varianten(*s).iter().map(|v| (*s, v.clone())))
        .collect();
    phrasen.sort_by_key(|(_, v)| std::cmp::Reverse((v.split(' ').count(), v.chars().count())));
    phrasen
}

/// Regulärer Ausdruck für eine Wortfolge: zwischen den Wörtern beliebiger
/// Leerraum, davor und danach kein Wortzeichen.
fn phrasen_muster(phrase: &str) -> String {
    format!("(?<!{WORTZEICHEN}){}(?!{WORTZEICHEN})", phrase.split(' ').collect::<Vec<_>>().join(r"\s+"))
}

/// Mehrere Varianten als eine Auswahl `(?:A|B)`.
fn auswahl(wb: &Woerterbuch, schluessel: &[Schluessel]) -> String {
    let mut varianten: Vec<String> = schluessel
        .iter()
        .flat_map(|s| wb.varianten(*s).iter().map(|v| v.split(' ').collect::<Vec<_>>().join(r"\s+")))
        .collect();
    varianten.sort_by_key(|v| std::cmp::Reverse(v.chars().count()));
    format!("(?:{})", varianten.join("|"))
}

/// TextMate-Grammatik: färbt ein, solange (oder falls) der Compiler nicht
/// antwortet. Die Farbklassen sind dieselben wie bei der Antwort vom Compiler.
fn grammatik(wb: &Woerterbuch) -> String {
    let mut muster = vec![
        (r"comment.line.number-sign.oida".to_string(), r"#.*$".to_string()),
        (r"constant.numeric.oida".to_string(), format!(r"(?<!{WORTZEICHEN})\d+(?:\.\d+)?")),
    ];
    for (schluessel, phrase) in alle_phrasen(wb) {
        let klasse = match art_von_schluessel(schluessel) {
            Art::Endwort => "keyword.other.endwort.oida",
            Art::Typ => "entity.name.type.oida",
            Art::Wahrheitswert => "constant.language.boolean.oida",
            Art::Zahl => "constant.numeric.oida",
            Art::Funktion => "entity.name.function.oida",
            _ => "keyword.control.oida",
        };
        // Eine eingebaute Funktion gilt nur, wenn gleich die Klammer folgt.
        let klammer = if schluessel.ist_funktion() { r"(?=\s*\()" } else { "" };
        muster.push((klasse.to_string(), format!("(?i){}{klammer}", phrasen_muster(&phrase))));
    }
    muster.push(("entity.name.function.oida".to_string(), r"[\p{L}_][\p{L}\p{N}_]*(?=\s*\()".to_string()));
    muster.push(("variable.other.readwrite.oida".to_string(), r"[\p{L}_][\p{L}\p{N}_]*".to_string()));
    muster.push(("keyword.operator.oida".to_string(), r"==|!=|<>|<=|>=|[-+*/%=<>]".to_string()));

    let mut aus = String::from(
        r#"{
  "//": "Erzeugt mit: oida --editor-dateien <ordner>. Nicht von Hand ändern.",
  "name": "OIDA",
  "scopeName": "source.oida",
  "patterns": [
    { "name": "comment.block.oida", "begin": "/\\*", "end": "\\*/" },
    {
      "name": "string.quoted.double.oida", "begin": "\"", "end": "\"|$",
      "patterns": [ { "name": "constant.character.escape.oida", "match": "\\\\." } ]
    },
    {
      "name": "string.quoted.single.oida", "begin": "'", "end": "'|$",
      "patterns": [ { "name": "constant.character.escape.oida", "match": "\\\\." } ]
    }"#,
    );
    for (klasse, ausdruck) in muster {
        let _ = write!(aus, ",\n    {{ \"name\": {}, \"match\": {} }}", json_text(&klasse), json_text(&ausdruck));
    }
    aus.push_str("\n  ]\n}\n");
    aus
}

/// `language-configuration.json`: Kommentare, Klammern, Einrücken.
fn sprach_einstellungen(wb: &Woerterbuch) -> String {
    let davor = format!("(?<!{WORTZEICHEN})");
    let danach = format!("(?!{WORTZEICHEN})");
    let funktion = auswahl(wb, &[Schluessel::Function]);
    let oeffner = auswahl(wb, &[Schluessel::Then, Schluessel::Do]);
    // (Auch die Zählschleife hört in der ersten Zeile mit TUA auf.)
    let sunst = auswahl(wb, &[Schluessel::Else]);
    let ende = auswahl(wb, &[Schluessel::End]);
    // Eine Zeile weiter einrücken nach: FUNKTIAUN ..., ... DAUN, ... TUA, SUNST
    let mehr = format!(
        r"^\s*(?:{funktion}{danach}.*|.*{davor}{oeffner}|{sunst})\s*(?:#.*)?$"
    );
    // Zurückrücken bei: PASST SCHO, SUNST
    let weniger = format!(r"^\s*(?:{ende}|{sunst}){danach}");
    format!(
        r##"{{
  "//": "Erzeugt mit: oida --editor-dateien <ordner>. Nicht von Hand ändern.",
  "comments": {{ "lineComment": "#", "blockComment": ["/*", "*/"] }},
  "brackets": [["(", ")"]],
  "autoClosingPairs": [
    {{ "open": "(", "close": ")" }},
    {{ "open": "\"", "close": "\"", "notIn": ["string", "comment"] }},
    {{ "open": "'", "close": "'", "notIn": ["string", "comment"] }},
    {{ "open": "/*", "close": "*/", "notIn": ["string"] }}
  ],
  "surroundingPairs": [["(", ")"], ["\"", "\""], ["'", "'"]],
  "wordPattern": {{ "pattern": "[\\p{{L}}_][\\p{{L}}\\p{{N}}_]*", "flags": "u" }},
  "indentationRules": {{
    "increaseIndentPattern": {{ "pattern": {mehr}, "flags": "iu" }},
    "decreaseIndentPattern": {{ "pattern": {weniger}, "flags": "iu" }}
  }}
}}
"##,
        mehr = json_text(&mehr),
        weniger = json_text(&weniger)
    )
}

/// Schnipsel (Vorlagen zum Einfügen) mit den Wörtern aus dem Wörterbuch.
fn schnipsel(wb: &Woerterbuch) -> String {
    let w = |s: Schluessel| wb.wort(s);
    use Schluessel::*;
    let liste: Vec<(String, String, Vec<String>, &str)> = vec![
        (
            w(If).to_string(),
            w(If).to_string(),
            vec![format!("{} ${{1:bedingung}} {}", w(If), w(Then)), "\t$0".into(), w(End).to_string()],
            "Verzweigung",
        ),
        (
            format!("{} {}", w(If), w(Else)),
            format!("{}{}", w(If), w(Else)),
            vec![
                format!("{} ${{1:bedingung}} {}", w(If), w(Then)),
                "\t$2".into(),
                w(Else).to_string(),
                "\t$0".into(),
                w(End).to_string(),
            ],
            "Verzweigung mit Sonst-Teil",
        ),
        (
            w(While).to_string(),
            w(While).to_string(),
            vec![format!("{} ${{1:bedingung}} {}", w(While), w(Do)), "\t$0".into(), w(End).to_string()],
            "Schleife",
        ),
        (
            w(For).to_string(),
            w(For).to_string(),
            vec![
                format!("{} ${{1:i}} {} ${{2:1}} {} ${{3:10}} {}", w(For), w(From), w(To), w(Do)),
                "\t$0".into(),
                w(End).to_string(),
            ],
            "Zählschleife",
        ),
        (
            w(Function).to_string(),
            w(Function).to_string(),
            vec![
                format!(
                    "{} ${{1:name}}(${{2:x}} ${{3:{}}}) {} ${{4:{}}}",
                    w(Function),
                    w(Int),
                    w(Returns),
                    w(Int)
                ),
                format!("\t{} ${{0:x}} {}", w(Return), w(Oida)),
                w(End).to_string(),
            ],
            "Funktion mit Rückgabewert",
        ),
        (
            w(Declare).to_string(),
            w(Declare).to_string(),
            vec![format!("{} ${{1:name}} = ${{2:0}} {}", w(Declare), w(Oida))],
            "Variable ansagen",
        ),
        (
            w(Print).to_string(),
            w(Print).to_string(),
            vec![format!("{}($1) {}", w(Print), w(Oida))],
            "Ausgabe",
        ),
    ];
    let mut aus = String::from("{\n  \"//\": \"Erzeugt mit: oida --editor-dateien <ordner>. Nicht von Hand ändern.\"");
    for (name, kuerzel, zeilen, beschreibung) in liste {
        let zeilen: Vec<String> = zeilen.iter().map(|z| json_text(z)).collect();
        let _ = write!(
            aus,
            ",\n  {}: {{\n    \"prefix\": {},\n    \"body\": [{}],\n    \"description\": {}\n  }}",
            json_text(&name),
            json_text(&kuerzel.replace(' ', "").to_lowercase()),
            zeilen.join(", "),
            json_text(beschreibung)
        );
    }
    aus.push_str("\n}\n");
    aus
}

/// Die Dateien der VS-Code-Erweiterung, die vom Wörterbuch abhängen:
/// Pfad (unterhalb vom Ordner der Erweiterung) und Inhalt.
pub fn dateien(wb: &Woerterbuch) -> Vec<(&'static str, String)> {
    vec![
        ("syntaxes/oida.tmLanguage.json", grammatik(wb)),
        ("language-configuration.json", sprach_einstellungen(wb)),
        ("snippets/oida.json", schnipsel(wb)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wb() -> Woerterbuch {
        Woerterbuch::eingebaut()
    }

    /// Holt ein Feld wie `"tokens":[...]` als Text aus dem JSON.
    fn feld<'a>(json: &'a str, name: &str) -> &'a str {
        let anfang = json.find(&format!("\"{name}\":[")).unwrap_or_else(|| panic!("{name} fehlt: {json}"));
        let rest = &json[anfang + name.len() + 3..];
        let ende = match name {
            "tokens" => rest.find("],\"woerter\"").unwrap() + 1,
            "fehler" => rest.find("],\"tokens\"").unwrap() + 1,
            _ => rest.len() - 1,
        };
        &rest[..ende]
    }

    #[test]
    fn tokens_mit_art() {
        let json = untersuche("SOG AU x = 5 OIDA\nSCHREIB(f(x), \"a\", WOAHR, ZOI(1.5)) OIDA", &wb());
        assert_eq!(
            feld(&json, "tokens"),
            "[[0,0,6,0],[0,7,1,3],[0,11,1,4],[0,13,4,7],\
             [1,0,7,0],[1,8,1,2],[1,10,1,3],[1,14,3,5],[1,19,5,6],[1,26,3,1],[1,30,3,4],[1,36,4,7]]"
        );
    }

    #[test]
    fn spalten_zaehlen_in_utf16() {
        // Das Gesicht braucht in UTF-16 zwei Einheiten, das ö eine.
        let json = untersuche("SCHREIB(\"😀ö\", x) OIDA", &wb());
        assert!(feld(&json, "tokens").starts_with("[[0,0,7,0],[0,8,5,5],[0,15,1,3],"), "{json}");
        assert!(feld(&json, "fehler").contains("\"zeile\":0,\"spalte\":15,\"laenge\":1"), "{json}");
    }

    #[test]
    fn fehlendes_oida_unterstreicht_das_wort_davor() {
        let json = untersuche("SOG AU alter = 17\nSCHREIB(alter) OIDA", &wb());
        assert_eq!(
            feld(&json, "fehler"),
            "[{\"zeile\":0,\"spalte\":15,\"laenge\":2,\"meldung\":\"Heast, wo is des OIDA?\"}]"
        );
    }

    #[test]
    fn typfehler_kommen_alle() {
        let json = untersuche("SOG AU a = 1 OIDA\na = \"x\" OIDA\nSCHREIB(b) OIDA", &wb());
        let fehler = feld(&json, "fehler");
        assert_eq!(fehler.matches("\"meldung\"").count(), 2, "{fehler}");
        assert!(fehler.contains("\"zeile\":1,\"spalte\":4,\"laenge\":3"), "{fehler}");
        assert!(fehler.contains("\"zeile\":2,\"spalte\":8,\"laenge\":1"), "{fehler}");
    }

    #[test]
    fn nach_einem_kaputten_text_wird_weiter_eingefaerbt() {
        let json = untersuche("SOG AU a = \"offen\nSCHREIB(1) OIDA", &wb());
        assert!(feld(&json, "fehler").contains("hört nimma auf"), "{json}");
        assert!(feld(&json, "tokens").contains("[1,0,7,0]"), "{json}");
    }

    #[test]
    fn ohne_fehler_ist_die_liste_leer() {
        let json = untersuche("SCHREIB(1) OIDA", &wb());
        assert_eq!(feld(&json, "fehler"), "[]");
    }

    #[test]
    fn woerter_fuer_vorschlaege() {
        let json = untersuche("", &wb());
        let woerter = feld(&json, "woerter");
        assert!(woerter.contains("{\"wort\":\"SOG AU\",\"fest\":\"DECLARE\",\"art\":0}"), "{woerter}");
        assert!(woerter.contains("{\"wort\":\"KETTN\",\"fest\":\"TEXT\",\"art\":1}"), "{woerter}");
        assert!(woerter.contains("{\"wort\":\"STIMMT NED\",\"fest\":\"FALSE\",\"art\":6}"), "{woerter}");
    }

    #[test]
    fn eingebaute_funktionen_sind_funktionen() {
        // LÄNGN mit Klammer ist eine Funktion (2), PI eine Zahl (4), hoch ohne Klammer eine Variable (3).
        let json = untersuche("SOG AU hoch = LÄNGN(\"a\") * PI OIDA", &wb());
        assert_eq!(feld(&json, "tokens"), "[[0,0,6,0],[0,7,4,3],[0,14,5,2],[0,20,3,5],[0,27,2,4],[0,30,4,7]]");
        let woerter = feld(&json, "woerter");
        assert!(woerter.contains("{\"wort\":\"LÄNGN\",\"fest\":\"LENGTH\",\"art\":2}"), "{woerter}");
        assert!(woerter.contains("{\"wort\":\"HAU OB\",\"fest\":\"BREAK\",\"art\":0}"), "{woerter}");
    }

    #[test]
    fn grammatik_kennt_funktionen_nur_mit_klammer() {
        let g = grammatik(&wb());
        assert!(g.contains(r"WURZL(?![\\p{L}\\p{N}_])(?=\\s*\\()"), "{g}");
        // FIA ist keine Funktion und braucht keine Klammer.
        assert!(g.contains(r#"FIA(?![\\p{L}\\p{N}_])" }"#), "{g}");
    }

    #[test]
    fn json_text_maskiert() {
        assert_eq!(json_text("a\"b\\c\nd\u{1}"), "\"a\\\"b\\\\c\\nd\\u0001\"");
    }

    #[test]
    fn grammatik_hat_die_laengsten_woerter_zuerst() {
        let g = grammatik(&wb());
        let stimmt_ned = g.find(r"STIMMT\\s+NED").expect("STIMMT NED fehlt");
        let stimmt = g.find(r"STIMMT(?!").expect("STIMMT fehlt");
        let ned = g.find(r"])NED(?!").expect("NED fehlt");
        assert!(stimmt_ned < stimmt && stimmt_ned < ned, "{g}");
        assert!(g.contains("\"scopeName\": \"source.oida\""));
    }

    #[test]
    fn erzeugte_dateien_richten_sich_nach_dem_woerterbuch() {
        let eigenes = Woerterbuch::laden("IF = FOIS\nEND = AUS IS\nTHEN = NOCHA\n", "test").unwrap();
        let alle = dateien(&eigenes);
        let finde = |name: &str| &alle.iter().find(|(n, _)| *n == name).unwrap().1;
        assert!(finde("syntaxes/oida.tmLanguage.json").contains(r"AUS\\s+IS"));
        assert!(finde("language-configuration.json").contains(r"AUS\\s+IS"));
        assert!(finde("snippets/oida.json").contains("FOIS ${1:bedingung} NOCHA"));
    }
}
