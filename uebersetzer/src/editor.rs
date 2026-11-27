//! Schnittstelle für Editoren (die VS-Code-Erweiterung in `vscode/`).
//!
//! - [`untersuche`] liefert für einen Quelltext als JSON: alle Fehler, die
//!   Art jedes Wortes (zum Einfärben) und die Wörter aus dem Wörterbuch (für
//!   Vorschläge). Der Editor fragt also immer den Compiler und kennt die
//!   Sprache nicht selbst; so stimmt alles auch mit einem eigenen Wörterbuch.
//! - [`dateien`] erzeugt die festen Dateien der Erweiterung aus dem
//!   Wörterbuch: Grammatik (Einfärben ohne Compiler), Einrückregeln, Schnipsel.

use std::fmt::Write;
use std::path::Path;

use crate::fehler::{Fehler, Position};
use crate::lexer::{self, Token, TokenArt};
use crate::woerterbuch::{Schluessel, Woerterbuch};
use crate::{treiber, typen};

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
    /// Ein Feld einer Struktur oder eines Objekts: das `x` in `punkt.x`
    Feld = 8,
}

fn art_von_schluessel(schluessel: Schluessel) -> Art {
    match schluessel {
        Schluessel::Oida => Art::Endwort,
        Schluessel::Int | Schluessel::Float | Schluessel::Boolean | Schluessel::Text | Schluessel::Error => Art::Typ,
        // NIX ist ein fester Wert wie WOAHR und FOISCH.
        Schluessel::True | Schluessel::False | Schluessel::Null => Art::Wahrheitswert,
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
///   "woerter": [ { "wort": "WENN", "fest": "IF", "art": 0 }, ... ],
///   "namen":   [ { "name": "klemm", "art": "funktion", "text": "klemm VON T(wert T, ...) GIBT ZRUCK T",
///                  "doku": "...", "aus": "mathe", "zu": "", "parameter": ["wert", "untn", "obn"] }, ... ]
/// }
/// ```
///
/// Zeilen und Spalten fangen bei 0 an, Spalten und Längen sind in
/// UTF-16-Einheiten (so zählt VS Code). `art` ist die Nummer aus [`Art`].
pub fn untersuche(quelltext: &str, wb: &Woerterbuch) -> String {
    untersuche_datei(quelltext, wb, None)
}

/// Wie [`untersuche`]. `pfad` ist die Datei, aus der der Text stammt: Von
/// ihrem Ordner aus werden die Dateien gesucht, die sie mit BRAUCH dazuholt.
pub fn untersuche_datei(quelltext: &str, wb: &Woerterbuch, pfad: Option<&Path>) -> String {
    let quelltext = quelltext.strip_prefix('\u{feff}').unwrap_or(quelltext);
    let (tokens, mut fehler) = lexer::zerlegen_nachsichtig(quelltext, wb);
    // Die Namen der Strukturen und Klassen aus den dazugeholten Dateien.
    let mut fremde_typen: Vec<String> = Vec::new();
    if fehler.is_empty() {
        let (quellen, programm) = treiber::lade(pfad, quelltext, wb);
        let ergebnis = programm.and_then(|mut programm| {
            fremde_typen.extend(programm.strukturen.iter().map(|s| s.name.clone()));
            fremde_typen.extend(programm.klassen.iter().map(|k| k.name.clone()));
            typen::pruefen(&mut programm, wb)
        });
        if let Err(alle) = ergebnis {
            fehler = alle;
        }
        // Ein Fehler in einer anderen Datei wird dort gezeigt, wo sie dazugeholt wird.
        for f in &mut fehler {
            if f.pos.datei != 0 {
                let quelle = &quellen[f.pos.datei as usize];
                f.meldung = format!("In {}, Zeile {}: {}", quelle.name, f.pos.zeile, f.meldung);
                f.pos = quelle.von;
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

    // Die Namen der Strukturen und Klassen: das Wort hinter STRUKTUR oder KLASSE.
    // Sie werden überall als Typ eingefärbt.
    let strukturen: Vec<&str> = tokens
        .windows(2)
        .filter_map(|paar| match (&paar[0].art, &paar[1].art) {
            (TokenArt::Schluessel(Schluessel::Struct | Schluessel::Class), TokenArt::Name(name)) => {
                Some(name.as_str())
            }
            _ => None,
        })
        .collect();

    aus.push_str("],\"tokens\":[");
    let mut erstes = true;
    for (i, token) in tokens.iter().enumerate() {
        let art = match &token.art {
            TokenArt::Schluessel(s) => art_von_schluessel(*s),
            TokenArt::Name(name) => {
                let aufruf = tokens.get(i + 1).is_some_and(|t| t.art == TokenArt::KlammerAuf);
                let nach_punkt = i > 0 && tokens[i - 1].art == TokenArt::Punkt;
                if nach_punkt {
                    // objekt.feld oder objekt.methode(...)
                    if aufruf { Art::Funktion } else { Art::Feld }
                } else if strukturen.contains(&name.as_str()) || fremde_typen.contains(name) {
                    Art::Typ
                } else if aufruf {
                    Art::Funktion
                } else {
                    Art::Variable
                }
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

    aus.push_str("],\"namen\":[");
    for (i, n) in alle_namen(&tokens, quelltext, wb, pfad).iter().enumerate() {
        let komma = if i > 0 { "," } else { "" };
        let liste = |teile: &[String]| teile.iter().map(|t| json_text(t)).collect::<Vec<String>>().join(",");
        let _ = write!(
            aus,
            "{komma}{{\"name\":{},\"art\":{},\"text\":{},\"doku\":{},\"aus\":{},\"zu\":{},\"parameter\":[{}],\
             \"typ\":{},\"eltern\":{},\"kette\":[{}],\"jedes\":{},\"von\":{},\"bis\":{},\"innen\":{},\
             \"raum\":{},\"gheim\":{}}}",
            json_text(&n.name),
            json_text(n.art),
            json_text(&n.text),
            json_text(&n.doku),
            json_text(&n.aus),
            json_text(&n.zu),
            liste(&n.parameter),
            json_text(&n.typ),
            json_text(&n.eltern),
            liste(&n.kette),
            n.jedes,
            n.von,
            n.bis,
            n.innen,
            json_text(&n.raum),
            n.gheim
        );
    }
    aus.push_str("]}");
    aus
}

// ------------------------------------------------- Namen für die Vorschläge

/// Ein Name aus dem Programm, den der Editor vorschlagen kann.
#[derive(Default)]
struct Eintrag {
    name: String,
    /// "funktion", "methode", "typ", "wert" (einer Aufzählung), "feld", "variable" oder "fix"
    art: &'static str,
    /// Wie er im Programm dasteht: der Kopf der Funktion, das Feld mit seinem Typ ...
    text: String,
    /// Die Kommentarzeilen direkt darüber.
    doku: String,
    /// Woher er kommt: leer (diese Datei), "listen" (ein Teil der Bibliothek) oder "gegner.oida".
    aus: String,
    /// Bei Methoden, Feldern und Werten: die Klasse, Struktur oder Aufzählung dazu.
    zu: String,
    /// Bei Funktionen und Methoden: die Namen der Parameter.
    parameter: Vec<String>,
    /// Der Typ, wo er dasteht: bei Feldern, Parametern und Variablen ihrer, bei Funktionen und
    /// Methoden der, den sie zurückgeben. In der kurzen Schreibweise von [`typ_lesen`].
    typ: String,
    /// Bei Klassen: von wem sie erbt.
    eltern: String,
    /// Bei Variablen ohne Typ: womit sie anfangen, siehe [`kette_lesen`]. Der Editor rechnet
    /// den Typ daraus selbst aus.
    kette: Vec<String>,
    /// Die Variable ist ein Element von `kette` (FIA JEDE x IN kette).
    jedes: bool,
    /// Die Zeilen (ab 0, beide eingeschlossen), in denen eine Variable gilt; bei Funktionen und
    /// Methoden aus der Datei im Editor die Zeilen, über die sie gehen.
    von: u32,
    bis: u32,
    /// Die Variable gehört zu einer Funktion (sonst zum Hauptprogramm, das Funktionen nicht sehen).
    innen: bool,
    /// Kommt der Name aus einer Datei, die mit `BRAUCH ... OIS w` dazugeholt ist: `w`. Dann
    /// heißt er im Programm `w.name`.
    raum: String,
    /// Ein Feld oder eine Methode mit GHEIM: nur für die Methoden der eigenen Klasse.
    gheim: bool,
}

/// Sammelt die Namen aus dem Text im Editor und aus allem, was er mit BRAUCH dazuholt.
///
/// Das geht absichtlich nur über die Wörter (Tokens) und nicht über den Syntaxbaum: Beim
/// Tippen ist die Zeile unter dem Cursor fast immer unfertig, und genau dann braucht man die
/// Vorschläge. Was zu einer Klasse, Struktur, Aufzählung oder Funktion gehört, wird an der
/// Einrückung erkannt (der Inhalt ist eingerückt; das nächste Wort, das so weit links steht
/// wie der Anfang, beendet sie).
fn alle_namen(tokens: &[Token], quelltext: &str, wb: &Woerterbuch, pfad: Option<&Path>) -> Vec<Eintrag> {
    let mut liste = Vec::new();
    let ordner = pfad.and_then(Path::parent).map(Path::to_path_buf).unwrap_or_default();
    // Was noch zu holen ist: (aus der Bibliothek?, Ziel, Ordner, der Name hinter OIS oder leer).
    let mut offen: std::collections::VecDeque<(bool, String, std::path::PathBuf, String)> =
        sammle_namen(tokens, quelltext, "", "", true, &mut liste)
            .into_iter()
            .map(|(b, ziel, raum)| (b, ziel, ordner.clone(), raum.unwrap_or_default()))
            .collect();
    let kennung = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()).to_string_lossy().to_lowercase();
    let mut geladen: std::collections::HashSet<String> = pfad.map(kennung).into_iter().collect();
    let eingebaut = Woerterbuch::eingebaut();

    while let Some((bibliothek, ziel, ordner, raum)) = offen.pop_front() {
        if geladen.len() > 200 {
            break;
        }
        let (text, woerter, name, daneben) = if bibliothek {
            let Some((teil, text)) = treiber::BIBLIOTHEK.iter().find(|(teil, _)| *teil == ziel) else { continue };
            if !geladen.insert(format!("oida:{teil}")) {
                continue;
            }
            (text.to_string(), &eingebaut, teil.to_string(), std::path::PathBuf::new())
        } else {
            let mut datei = ordner.join(&ziel);
            if datei.extension().is_none() {
                datei.set_extension("oida");
            }
            if !geladen.insert(kennung(&datei)) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&datei) else { continue };
            let name = datei.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            (text, wb, name, datei.parent().map(Path::to_path_buf).unwrap_or_default())
        };
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let (tokens, _) = lexer::zerlegen_nachsichtig(text, woerter);
        for (b, ziel, innen_raum) in sammle_namen(&tokens, text, &name, &raum, false, &mut liste) {
            // Was eine dazugeholte Datei selbst ohne OIS dazuholt, gilt überall. Was sie mit OIS
            // dazuholt, ist nur dort unter diesem Namen zu haben, also nicht in der Datei im Editor.
            if innen_raum.is_none() {
                offen.push_back((b, ziel, daneben.clone(), String::new()));
            }
        }
    }

    // Jeder Name nur einmal (eine Variable wird oft mehrmals angelegt); dieselbe Variable in
    // zwei Funktionen sind aber zwei.
    let mut gesehen = std::collections::HashSet::new();
    liste.retain(|e| gesehen.insert((e.art, e.name.clone(), e.zu.clone(), e.innen, e.bis)));
    liste
}

fn ist_wort(tokens: &[Token], i: usize, schluessel: Schluessel) -> bool {
    tokens.get(i).is_some_and(|t| t.art == TokenArt::Schluessel(schluessel))
}

/// `i` steht auf `(` oder `[`: die Stelle hinter der Klammer, die dazu passt.
fn hinter_klammer(tokens: &[Token], mut i: usize) -> usize {
    let mut tiefe = 0;
    while let Some(t) = tokens.get(i) {
        match t.art {
            TokenArt::KlammerAuf | TokenArt::EckigAuf => tiefe += 1,
            TokenArt::KlammerZu | TokenArt::EckigZu => tiefe -= 1,
            TokenArt::DateiEnde => return i,
            _ => {}
        }
        i += 1;
        if tiefe == 0 {
            break;
        }
    }
    i
}

/// Liest einen Typ ab der Stelle `i` und gibt ihn in einer kurzen Schreibweise zurück, die
/// nicht vom Wörterbuch abhängt, dazu die Stelle dahinter:
/// `Punkt` (Struktur, Klasse, Aufzählung), `?T` (VIELLEICHT T), `[T]` (LISTN VON T),
/// `{K:V}` (VAZEICHNIS VON K UND V), `INT`, `FLOAT`, `BOOLEAN`, `TEXT`, `ERROR`, `FN` (eine
/// Funktion). Steht dort kein Typ, ist der Text leer.
fn typ_lesen(tokens: &[Token], i: usize) -> (String, usize) {
    use Schluessel as S;
    let Some(token) = tokens.get(i) else { return (String::new(), i) };
    match &token.art {
        TokenArt::Schluessel(S::Maybe) => {
            let (t, n) = typ_lesen(tokens, i + 1);
            (format!("?{t}"), n)
        }
        TokenArt::Schluessel(S::List) if ist_wort(tokens, i + 1, S::From) => {
            let (t, n) = typ_lesen(tokens, i + 2);
            (format!("[{t}]"), n)
        }
        TokenArt::Schluessel(S::Map) if ist_wort(tokens, i + 1, S::From) => {
            let (k, n) = typ_lesen(tokens, i + 2);
            if ist_wort(tokens, n, S::And) {
                let (v, m) = typ_lesen(tokens, n + 1);
                (format!("{{{k}:{v}}}"), m)
            } else {
                (format!("{{{k}:}}"), n)
            }
        }
        TokenArt::Schluessel(S::Int) => ("INT".to_string(), i + 1),
        TokenArt::Schluessel(S::Float) => ("FLOAT".to_string(), i + 1),
        TokenArt::Schluessel(S::Boolean) => ("BOOLEAN".to_string(), i + 1),
        TokenArt::Schluessel(S::Text) => ("TEXT".to_string(), i + 1),
        TokenArt::Schluessel(S::Error) => ("ERROR".to_string(), i + 1),
        // Eine Funktion als Wert: FUNKTIAUN(ZOI) GIBT ZRUCK JONA
        TokenArt::Schluessel(S::Function) => {
            let mut n = i + 1;
            if tokens.get(n).is_some_and(|t| t.art == TokenArt::KlammerAuf) {
                n = hinter_klammer(tokens, n);
            }
            if ist_wort(tokens, n, S::Returns) {
                n = typ_lesen(tokens, n + 1).1;
            }
            ("FN".to_string(), n)
        }
        TokenArt::Name(name) => {
            // Eine Vorlage mit Angabe (Stapl VON ZOI): Für die Vorschläge zählt nur der Name.
            let mut n = i + 1;
            if ist_wort(tokens, n, S::From) {
                n = typ_lesen(tokens, n + 1).1;
                while ist_wort(tokens, n, S::And) {
                    n = typ_lesen(tokens, n + 1).1;
                }
            }
            (name.clone(), n)
        }
        _ => (String::new(), i),
    }
}

/// Liest ab der Stelle `i` eine Kette aus Namen, Punkten, Aufrufen und Stellen in Listen,
/// also den Anfang eines Ausdrucks wie `spü.gegna[0].nam` oder `Punkt(1, 2)`:
/// `["spü", "gegna", "[]", "nam"]` bzw. `["Punkt()"]`. `@` steht für MEI. Fängt der Ausdruck
/// anders an (eine Zahl, ein Text, eine Klammer), ist die Kette leer.
fn kette_lesen(tokens: &[Token], mut i: usize) -> Vec<String> {
    let mut kette = Vec::new();
    match tokens.get(i).map(|t| &t.art) {
        Some(TokenArt::Name(name)) => kette.push(name.clone()),
        Some(TokenArt::Schluessel(Schluessel::This)) => kette.push("@".to_string()),
        _ => return kette,
    }
    i += 1;
    loop {
        match tokens.get(i).map(|t| &t.art) {
            Some(TokenArt::KlammerAuf) => {
                i = hinter_klammer(tokens, i);
                match kette.last_mut() {
                    Some(letztes) if letztes != "[]" && !letztes.ends_with(')') => letztes.push_str("()"),
                    // Ein Aufruf von etwas, das selbst erst ausgerechnet wurde: Was herauskommt, weiß nur der Compiler.
                    _ => kette.push("?".to_string()),
                }
            }
            Some(TokenArt::EckigAuf) => {
                i = hinter_klammer(tokens, i);
                kette.push("[]".to_string());
            }
            Some(TokenArt::Punkt) => match tokens.get(i + 1).map(|t| &t.art) {
                Some(TokenArt::Name(name)) => {
                    kette.push(name.clone());
                    i += 2;
                }
                _ => break,
            },
            _ => break,
        }
    }
    kette
}

/// Die Zeile, in der der Block endet, der mit dem Token `i` anfängt: die nächste Zeile, deren
/// erstes Wort so weit links steht wie dieses Token oder weiter (das PASST SCHO dazu). Fehlt
/// sie noch, reicht der Block bis zum Ende.
fn ende_des_blocks(tokens: &[Token], i: usize) -> u32 {
    let anfang = &tokens[i];
    let mut letzte = anfang.pos.zeile;
    for j in i + 1..tokens.len() {
        let t = &tokens[j];
        if t.art == TokenArt::DateiEnde {
            break;
        }
        letzte = t.pos.zeile;
        if t.pos.zeile > anfang.pos.zeile && tokens[j - 1].pos.zeile < t.pos.zeile && t.pos.spalte <= anfang.pos.spalte {
            return t.pos.zeile;
        }
    }
    letzte
}

/// Die Namen aus einer Datei. `haupt` ist die Datei im Editor: Nur aus ihr kommen die
/// Variablen. Gibt zurück, was die Datei mit BRAUCH dazuholt: (aus der Bibliothek?, Ziel).
fn sammle_namen(
    tokens: &[Token],
    text: &str,
    aus: &str,
    raum: &str,
    haupt: bool,
    liste: &mut Vec<Eintrag>,
) -> Vec<(bool, String, Option<String>)> {
    use Schluessel as S;
    let zeilen: Vec<&str> = text.split('\n').collect();
    let letzte_zeile = zeilen.len().max(1) as u32;
    let zeile = |nr: u32| zeilen.get(nr as usize - 1).copied().unwrap_or("").trim_end_matches('\r');
    // Die Zeile ab einem Token, höchstens bis vor ein anderes.
    let ab = |von: &Token, bis: Option<&Token>| -> String {
        let ganz = zeile(von.pos.zeile);
        let ende = bis.map_or(usize::MAX, |b| b.pos.spalte as usize - 1);
        ganz.chars().take(ende).skip(von.pos.spalte as usize - 1).collect::<String>().trim().to_string()
    };
    // Die Kommentarzeilen direkt über einer Zeile.
    let doku = |nr: u32| -> String {
        let mut teile = Vec::new();
        let mut n = nr;
        while n > 1 {
            let Some(kommentar) = zeile(n - 1).trim().strip_prefix('#') else { break };
            teile.push(kommentar.strip_prefix(' ').unwrap_or(kommentar));
            n -= 1;
        }
        teile.reverse();
        teile.join("\n")
    };
    let name_bei = |i: usize| match tokens.get(i).map(|t| &t.art) {
        Some(TokenArt::Name(name)) => Some(name.clone()),
        _ => None,
    };
    let mut neu = |eintrag: Eintrag| liste.push(Eintrag { aus: aus.to_string(), raum: raum.to_string(), ..eintrag });

    // BRAUCH ... OIS raum: der Name hinter OIS, wenn dort einer steht.
    let raum_hinter = |i: usize| if ist_wort(tokens, i, S::As) { name_bei(i + 1) } else { None };
    let mut brauch = Vec::new();
    // Steckt, was gerade kommt, in etwas mit GHEIM aus einer anderen Datei? Dann geht es die Datei im Editor nichts an.
    let mut versteckt = false;
    // In welcher Klasse, Struktur oder Aufzählung wir gerade stecken.
    let mut drin: Option<(String, Schluessel)> = None;
    // Bis zu welcher Zeile die Funktion geht, in der wir gerade stecken.
    let mut funktion_bis: Option<u32> = None;
    for (i, token) in tokens.iter().enumerate() {
        let am_anfang = token.pos.spalte == 1;
        // GHEIM vor dem Wort: Dann fängt die Zeile mit GHEIM an, und dort beginnt auch der Block.
        let gheim = i > 0 && ist_wort(tokens, i - 1, S::Private) && tokens[i - 1].pos.zeile == token.pos.zeile;
        let anfang = if gheim { i - 1 } else { i };
        let erstes_der_zeile = anfang == 0 || tokens[anfang - 1].pos.zeile < tokens[anfang].pos.zeile;
        if am_anfang {
            drin = None;
            versteckt = false;
        }
        if versteckt && !matches!(token.art, TokenArt::Schluessel(S::Import)) {
            continue;
        }
        // Was in einer anderen Datei ganz außen GHEIM ist, gibt es hier nicht.
        let ganz_aussen_gheim = gheim && tokens[anfang].pos.spalte == 1;
        if ganz_aussen_gheim && !haupt {
            versteckt = true;
            continue;
        }
        if funktion_bis.is_some_and(|bis| token.pos.zeile > bis) {
            funktion_bis = None;
        }
        // Wo eine Variable gilt, die hier angelegt wird: (innen, von, bis), Zeilen ab 0.
        let gilt = match funktion_bis {
            Some(bis) => (true, token.pos.zeile - 1, bis - 1),
            None => (false, token.pos.zeile - 1, letzte_zeile - 1),
        };
        let variable = |name: String, text: String, typ: String, kette: Vec<String>, jedes: bool| Eintrag {
            name,
            art: "variable",
            text,
            typ,
            kette,
            jedes,
            innen: gilt.0,
            von: gilt.1,
            bis: gilt.2,
            ..Eintrag::default()
        };
        match &token.art {
            TokenArt::Schluessel(S::Import) => match tokens.get(i + 1).map(|t| &t.art) {
                Some(TokenArt::Name(ziel)) => brauch.push((true, ziel.clone(), raum_hinter(i + 2))),
                Some(TokenArt::Text(ziel)) => brauch.push((false, ziel.clone(), raum_hinter(i + 2))),
                _ => {}
            },
            TokenArt::Schluessel(art @ (S::Struct | S::Class | S::Enum)) => {
                if let Some(name) = name_bei(i + 1) {
                    // KLASSE Dackl ERBT VON Hund
                    let eltern = (i + 2..tokens.len())
                        .take_while(|j| tokens[*j].pos.zeile == token.pos.zeile)
                        .find(|j| ist_wort(tokens, *j, S::Extends))
                        .and_then(|j| name_bei(j + 1))
                        .unwrap_or_default();
                    neu(Eintrag {
                        name: name.clone(),
                        art: "typ",
                        text: ab(token, None),
                        doku: doku(token.pos.zeile),
                        eltern,
                        ..Eintrag::default()
                    });
                    drin = Some((name, *art));
                }
            }
            TokenArt::Schluessel(S::Function) => {
                let Some(name) = name_bei(i + 1) else { continue };
                // Bis zur Klammer (dazwischen kann "VON T" stehen), dann die Parameter: der Name
                // direkt hinter der Klammer und hinter jedem Beistrich auf der obersten Ebene.
                let mut j = i + 2;
                while tokens.get(j).is_some_and(|t| t.art != TokenArt::KlammerAuf && t.pos.zeile == token.pos.zeile) {
                    j += 1;
                }
                if tokens.get(j).is_none_or(|t| t.art != TokenArt::KlammerAuf) {
                    continue;
                }
                let mut parameter: Vec<(String, String)> = Vec::new();
                let mut tiefe = 0;
                while let Some(t) = tokens.get(j) {
                    match t.art {
                        TokenArt::KlammerAuf | TokenArt::EckigAuf => tiefe += 1,
                        TokenArt::KlammerZu | TokenArt::EckigZu => tiefe -= 1,
                        TokenArt::DateiEnde => break,
                        _ => {}
                    }
                    if tiefe == 0 {
                        break;
                    }
                    if tiefe == 1 && (t.art == TokenArt::KlammerAuf || t.art == TokenArt::Komma) {
                        if let Some(p) = name_bei(j + 1) {
                            parameter.push((p, typ_lesen(tokens, j + 2).0));
                        }
                    }
                    j += 1;
                }
                // Hinter der Klammer: was sie zurückgibt.
                let gibt = if ist_wort(tokens, j + 1, S::Returns) { typ_lesen(tokens, j + 2).0 } else { String::new() };
                // "VON DRAUSSN ..." (Funktionen aus der Laufzeit) gehört nicht zu dem, was man sehen will;
                // so eine Funktion hat auch keinen Inhalt.
                let draussen = tokens[i..]
                    .iter()
                    .take_while(|t| t.pos.zeile == token.pos.zeile)
                    .find(|t| t.art == TokenArt::Schluessel(S::Extern));
                let ende = if draussen.is_some() { token.pos.zeile } else { ende_des_blocks(tokens, anfang) };
                funktion_bis = Some(ende);
                let kopf = ab(&tokens[i + 1], draussen);
                if haupt {
                    for (p, typ) in &parameter {
                        neu(Eintrag {
                            name: p.clone(),
                            art: "variable",
                            text: format!("Parameter von {name}"),
                            typ: typ.clone(),
                            innen: true,
                            von: token.pos.zeile - 1,
                            bis: ende - 1,
                            ..Eintrag::default()
                        });
                    }
                }
                let (art, zu) = match &drin {
                    Some((klasse, S::Class)) if !am_anfang && !ganz_aussen_gheim => ("methode", klasse.clone()),
                    _ => ("funktion", String::new()),
                };
                neu(Eintrag {
                    name,
                    gheim: gheim && art == "methode",
                    art,
                    text: kopf,
                    doku: doku(token.pos.zeile),
                    zu,
                    parameter: parameter.into_iter().map(|(p, _)| p).collect(),
                    typ: gibt,
                    von: if haupt { token.pos.zeile - 1 } else { 0 },
                    bis: if haupt { ende - 1 } else { 0 },
                    ..Eintrag::default()
                });
            }
            TokenArt::Schluessel(S::Const) if am_anfang || haupt => {
                if let Some(name) = name_bei(i + 1) {
                    neu(Eintrag { name, art: "fix", text: ab(token, None), doku: doku(token.pos.zeile), ..Eintrag::default() });
                }
            }
            // SOG AU x = ausdruck   oder   SOG AU x Typ = ausdruck
            TokenArt::Schluessel(S::Declare) if haupt => {
                if let Some(name) = name_bei(i + 1) {
                    let (typ, kette) = if tokens.get(i + 2).is_some_and(|t| t.art == TokenArt::Gleich) {
                        // Fängt es mit einer Zahl, einem Text oder WOAHR/FOISCH an, ist der Typ klar.
                        let fest = match tokens.get(i + 3).map(|t| &t.art) {
                            Some(TokenArt::Zoi(_)) => "INT",
                            Some(TokenArt::Kommazoi(_)) => "FLOAT",
                            Some(TokenArt::Text(_)) => "TEXT",
                            Some(TokenArt::Schluessel(S::True | S::False)) => "BOOLEAN",
                            _ => "",
                        };
                        (fest.to_string(), kette_lesen(tokens, i + 3))
                    } else {
                        (typ_lesen(tokens, i + 2).0, Vec::new())
                    };
                    neu(variable(name, ab(token, None), typ, kette, false));
                }
            }
            // FIA i VON 1 BIS 3   oder   FIA JEDE x IN liste
            TokenArt::Schluessel(S::For) if haupt => {
                if ist_wort(tokens, i + 1, S::Each) {
                    if let Some(name) = name_bei(i + 2) {
                        let kette = if ist_wort(tokens, i + 3, S::In) { kette_lesen(tokens, i + 4) } else { Vec::new() };
                        neu(variable(name, ab(token, None), String::new(), kette, true));
                    }
                } else if let Some(name) = name_bei(i + 1) {
                    neu(variable(name, ab(token, None), "INT".to_string(), Vec::new(), false));
                }
            }
            // PROBIER x = ausdruck
            TokenArt::Schluessel(S::Try) if haupt => {
                if let (Some(name), true) = (name_bei(i + 1), tokens.get(i + 2).is_some_and(|t| t.art == TokenArt::Gleich)) {
                    neu(variable(name, ab(token, None), String::new(), kette_lesen(tokens, i + 3), false));
                }
            }
            // WONNS SCHIEFGEHT f
            TokenArt::Schluessel(S::Catch) if haupt => {
                if let Some(name) = name_bei(i + 1) {
                    neu(variable(name, ab(token, None), "ERROR".to_string(), Vec::new(), false));
                }
            }
            TokenArt::Name(name) if !am_anfang => match &drin {
                // Die Werte einer Aufzählung: jeder Name darin (nur nicht ihr eigener).
                Some((aufzaehlung, S::Enum)) if tokens[i - 1].art != TokenArt::Schluessel(S::Enum) => {
                    neu(Eintrag {
                        name: name.clone(),
                        art: "wert",
                        text: format!("{aufzaehlung}.{name}"),
                        zu: aufzaehlung.clone(),
                        ..Eintrag::default()
                    });
                }
                // Ein Feld: "name Typ" am Anfang einer Zeile, also ein Name und gleich dahinter ein Typ.
                Some((besitzer, S::Struct | S::Class)) if erstes_der_zeile && funktion_bis.is_none() => {
                    let (typ, dahinter) = typ_lesen(tokens, i + 1);
                    let selbe_zeile = tokens.get(i + 1).is_some_and(|t| t.pos.zeile == token.pos.zeile);
                    if selbe_zeile && dahinter > i + 1 {
                        neu(Eintrag {
                            name: name.clone(),
                            art: "feld",
                            gheim,
                            text: ab(token, None),
                            doku: doku(token.pos.zeile),
                            zu: besitzer.clone(),
                            typ,
                            ..Eintrag::default()
                        });
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    brauch
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
    let funktion = auswahl(wb, &[Schluessel::Function, Schluessel::Struct, Schluessel::Class]);
    let oeffner = auswahl(wb, &[Schluessel::Then, Schluessel::Do]);
    // (Auch die Zählschleife hört in der ersten Zeile mit TUA auf.)
    // SUNST und WONNS SCHIEFGEHT: Der Block davor hört auf, der nächste beginnt.
    let sunst = auswahl(wb, &[Schluessel::Else]);
    let auffangen = auswahl(wb, &[Schluessel::Catch]);
    let probier = auswahl(wb, &[Schluessel::Try]);
    let oida = auswahl(wb, &[Schluessel::Oida]);
    let nur_funktion = auswahl(wb, &[Schluessel::Function]);
    let zruck = auswahl(wb, &[Schluessel::Return]);
    let ende = auswahl(wb, &[Schluessel::End]);
    // Eine Zeile weiter einrücken nach: FUNKTIAUN ..., STRUKTUR ..., KLASSE ..., ... DAUN, ... TUA, SUNST
    // und nach PROBIER ..., wenn die Zeile nicht mit OIDA aufhört (dann folgt der Block),
    // und nach WONNS SCHIEFGEHT [name],
    // und nach einer Funktion ohne Namen mit Block: "... = FUNKTIAUN(x ZOI) GIBT ZRUCK ZOI"
    // (sie steht hinter =, einer Klammer, einem Beistrich oder GIB ZRUCK; kurz mit => und fertig mit OIDA zählt nicht).
    let ohne_nom = format!(
        r".*(?:[=(,]|{davor}{zruck})\s*{nur_funktion}\s*\((?!.*=>)(?!.*{davor}{oida}\s*(?:#.*)?$).*"
    );
    let mehr = format!(
        r"^\s*(?:{funktion}{danach}.*|.*{davor}{oeffner}|{sunst}|{auffangen}{danach}.*|{probier}{danach}(?!.*{davor}{oida}\s*(?:#.*)?$).*|{ohne_nom})\s*(?:#.*)?$"
    );
    // Zurückrücken bei: PASST SCHO, SUNST, WONNS SCHIEFGEHT
    let weniger = format!(r"^\s*(?:{ende}|{sunst}|{auffangen}){danach}");
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
            format!("{} {}", w(For), w(Each)),
            format!("{}{}", w(For), w(Each)),
            vec![
                format!("{} {} ${{1:x}} {} ${{2:liste}} {}", w(For), w(Each), w(In), w(Do)),
                "\t$0".into(),
                w(End).to_string(),
            ],
            "Schleife über eine Liste",
        ),
        (
            w(Struct).to_string(),
            w(Struct).to_string(),
            vec![
                format!("{} ${{1:Name}}", w(Struct)),
                format!("\t${{2:feld}} ${{3:{}}}", w(Int)),
                w(End).to_string(),
            ],
            "Struktur mit Feldern",
        ),
        (
            w(Class).to_string(),
            w(Class).to_string(),
            vec![
                format!("{} ${{1:Name}}", w(Class)),
                format!("\t${{2:feld}} ${{3:{}}}", w(Int)),
                String::new(),
                format!("\t{} ${{4:methode}}()", w(Function)),
                "\t\t$0".into(),
                format!("\t{}", w(End)),
                w(End).to_string(),
            ],
            "Klasse mit einem Feld und einer Methode",
        ),
        (
            format!("{} {}", w(Class), w(Extends)),
            format!("{}{}", w(Class), w(Extends).replace(' ', "")),
            vec![
                format!("{} ${{1:Name}} {} ${{2:Eltern}}", w(Class), w(Extends)),
                "\t$0".into(),
                w(End).to_string(),
            ],
            "Klasse, die von einer anderen erbt",
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
            format!("{} {}", w(Declare), w(Maybe)),
            format!("{}{}", w(Declare).replace(' ', ""), w(Maybe)),
            vec![format!("{} ${{1:name}} {} ${{2:{}}} = {} {}", w(Declare), w(Maybe), w(Int), w(Null), w(Oida))],
            "Variable, in der etwas fehlen darf",
        ),
        (
            format!("{} != {}", w(If), w(Null)),
            format!("{}{}", w(If), w(Null)),
            vec![format!("{} ${{1:name}} != {} {}", w(If), w(Null), w(Then)), "\t$0".into(), w(End).to_string()],
            "Nachschauen, ob etwas da ist",
        ),
        (
            w(Try).to_string(),
            w(Try).to_string(),
            vec![
                format!("{} ${{1:wert}} = ${{2:funktion()}}", w(Try)),
                "\t$3".into(),
                format!("{} ${{4:fehla}}", w(Catch)),
                "\t$0".into(),
                w(End).to_string(),
            ],
            "Etwas probieren und den Fehler auffangen",
        ),
        (
            format!("{} =>", w(Function)),
            format!("{}kurz", w(Function)),
            vec![format!("{}(${{1:x}} ${{2:{}}}) => ${{0:x}}", w(Function), w(Int))],
            "Kurze Funktion ohne Namen",
        ),
        (
            w(Import).to_string(),
            w(Import).to_string(),
            vec![format!(
                "{} ${{1|listen,texte,mathe,dateien,programm,json,netz,fensta,ton,\"datei.oida\"|}} {}",
                w(Import),
                w(Oida)
            )],
            "Bibliothek oder eigene Datei dazuholen",
        ),
        (
            format!("{} {} T", w(Function), w(From)),
            format!("{}{}", w(Function), w(From)),
            vec![
                format!(
                    "{} ${{1:name}} {} T(${{2:liste}} {} {} T) {} T",
                    w(Function),
                    w(From),
                    w(List),
                    w(From),
                    w(Returns)
                ),
                "\t$0".into(),
                w(End).to_string(),
            ],
            "Vorlage: eine Funktion für jeden Typ",
        ),
        (
            w(Map).to_string(),
            w(Map).to_string(),
            vec![format!(
                "{} ${{1:name}} = {} {} ${{2:{}}} {} ${{3:{}}}() {}",
                w(Declare),
                w(Map),
                w(From),
                w(Text),
                w(And),
                w(Int),
                w(Oida)
            )],
            "Verzeichnis: zu einem Schlüssel einen Wert merken",
        ),
        (
            w(Const).to_string(),
            w(Const).to_string(),
            vec![format!("{} ${{1:NAME}} = ${{2:0}} {}", w(Const), w(Oida))],
            "Fester Wert, der im ganzen Programm gilt",
        ),
        (
            "fensta".to_string(),
            "fensta".to_string(),
            vec![
                format!("{} fensta {}", w(Import), w(Oida)),
                String::new(),
                format!("fensta_auf(\"${{1:Mei Programm}}\", ${{2:800}}, ${{3:600}}) {}", w(Oida)),
                format!("{} fensta_offn() {}", w(While), w(Do)),
                format!("\tfoab(20, 20, 40) {}", w(Oida)),
                format!("\thintagrund() {}", w(Oida)),
                "\t$0".into(),
                w(End).to_string(),
            ],
            "Ein Fenster aufmachen und in einer Schleife zeichnen",
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

    /// Die Einstellungen für VS Code (Einrücken und Klammern) als Text.
    fn dateien_text(wb: &Woerterbuch) -> String {
        sprach_einstellungen(wb)
    }

    fn wb() -> Woerterbuch {
        Woerterbuch::eingebaut()
    }

    /// Holt ein Feld wie `"tokens":[...]` als Text aus dem JSON.
    fn feld<'a>(json: &'a str, name: &str) -> &'a str {
        let anfang = json.find(&format!("\"{name}\":[")).unwrap_or_else(|| panic!("{name} fehlt: {json}"));
        let rest = &json[anfang + name.len() + 3..];
        let ende = match name {
            "tokens" => rest.find("],\"woerter\"").unwrap() + 1,
            "woerter" => rest.find("],\"namen\"").unwrap() + 1,
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
    fn klassen_und_methoden() {
        let json = untersuche(
            "KLASSE Wesen\n lebn ZOI\n FUNKTIAUN sog()\n  SCHREIB(SÖWA.lebn) OIDA\n PASST SCHO\nPASST SCHO\n\
             SOG AU w = Wesen(1) OIDA\nw.sog() OIDA",
            &wb(),
        );
        assert_eq!(feld(&json, "fehler"), "[]");
        let tokens = feld(&json, "tokens");
        // Der Name der Klasse ist überall ein Typ.
        assert!(tokens.contains("[0,7,5,1]") && tokens.contains("[6,11,5,1]"), "{tokens}");
        // SÖWA ist ein Wort der Sprache, lebn dahinter ein Feld.
        assert!(tokens.contains("[3,10,4,0],[3,15,4,8]"), "{tokens}");
        // w.sog(): eine Variable und eine Methode.
        assert!(tokens.contains("[7,0,1,3],[7,2,3,2]"), "{tokens}");
    }

    #[test]
    fn nix_fehla_und_probier() {
        let json = untersuche(
            "FUNKTIAUN f() GIBT ZRUCK ZOI ODA FEHLA\n GIB ZRUCK 1 OIDA\nPASST SCHO\n\
             SOG AU x VIELLEICHT ZOI = NIX OIDA\nPROBIER q = f()\nWONNS SCHIEFGEHT\nPASST SCHO",
            &wb(),
        );
        assert_eq!(feld(&json, "fehler"), "[]");
        let tokens = feld(&json, "tokens");
        // ODA (Wort der Sprache) und FEHLA (Typ) in der ersten Zeile.
        assert!(tokens.contains("[0,29,3,0],[0,33,5,1]"), "{tokens}");
        // VIELLEICHT ist ein Wort der Sprache, NIX ein fester Wert.
        assert!(tokens.contains("[3,9,10,0],[3,20,3,1]") && tokens.contains("[3,26,3,6]"), "{tokens}");
        // PROBIER und WONNS SCHIEFGEHT (zwei Wörter, ein Stück).
        assert!(tokens.contains("[4,0,7,0]") && tokens.contains("[5,0,16,0]"), "{tokens}");
    }

    #[test]
    fn funktionen_vorlagen_und_verzeichnis() {
        let json = untersuche(
            "FUNKTIAUN erstes VON T(liste LISTN VON T) GIBT ZRUCK T\n GIB ZRUCK liste[0] OIDA\nPASST SCHO\n\
             SOG AU f = FUNKTIAUN(x ZOI) => x + erstes([1]) OIDA\nSOG AU v = VAZEICHNIS VON KETTN UND ZOI() OIDA\nv[\"a\"] = f(2) OIDA",
            &wb(),
        );
        assert_eq!(feld(&json, "fehler"), "[]");
        let tokens = feld(&json, "tokens");
        // FUNKTIAUN ohne Namen und VAZEICHNIS sind Wörter der Sprache, f(2) ist ein Aufruf.
        assert!(tokens.contains("[3,11,9,0]") && tokens.contains("[4,11,10,0]"), "{tokens}");
        assert!(tokens.contains("[5,9,1,2]"), "{tokens}");
    }

    #[test]
    fn brauch_holt_die_bibliothek() {
        let json = untersuche("BRAUCH listen OIDA\nSCHREIB(summe([1, 2])) OIDA", &wb());
        assert_eq!(feld(&json, "fehler"), "[]");
        // BRAUCH ist ein Wort der Sprache.
        assert!(feld(&json, "tokens").starts_with("[[0,0,6,0],"), "{json}");
        let json = untersuche("# nix\nBRAUCH gibtsned OIDA", &wb());
        assert!(feld(&json, "fehler").contains("\"zeile\":1,\"spalte\":0,\"laenge\":6"), "{json}");
        assert!(feld(&json, "fehler").contains("A Bibliothek 'gibtsned' gibt's ned."), "{json}");
    }

    #[test]
    fn fehler_in_einer_anderen_datei_steht_beim_brauch() {
        let ordner = std::env::temp_dir().join(format!("oida-editor-test-{}", std::process::id()));
        std::fs::create_dir_all(&ordner).unwrap();
        std::fs::write(
            ordner.join("teil.oida"),
            "KLASSE Wesen\n name KETTN\nPASST SCHO\nFUNKTIAUN hoib(x ZOI) GIBT ZRUCK ZOI\n GIB ZRUCK x + \"a\" OIDA\nPASST SCHO\n",
        )
        .unwrap();
        let haupt = ordner.join("haupt.oida");
        let json = untersuche_datei("# obn\nBRAUCH \"teil\" OIDA\nSOG AU w = Wesen(\"a\") OIDA", &wb(), Some(&haupt));
        let _ = std::fs::remove_dir_all(&ordner);
        // Der Fehler aus teil.oida, Zeile 5, wird am BRAUCH gezeigt (Zeile 1, ab 0 gezählt).
        let fehler = feld(&json, "fehler");
        assert!(fehler.contains("\"zeile\":1,\"spalte\":0,\"laenge\":6"), "{fehler}");
        assert!(fehler.contains("teil.oida, Zeile 5: A ZOI + a KETTN?"), "{fehler}");
        // Die Klasse aus der anderen Datei ist hier ein Typ.
        assert!(feld(&json, "tokens").contains("[2,11,5,1]"), "{json}");
    }

    #[test]
    fn einruecken_nach_probier() {
        let json = dateien_text(&wb());
        // Die Regeln stehen als reguläre Ausdrücke in der Datei; hier wird nur geprüft, dass die Wörter drin sind.
        assert!(json.contains("PROBIER|PROBIERS") || json.contains("PROBIERS|PROBIER"), "{json}");
        assert!(json.contains("WONNS\\\\s+SCHIEFGEHT"), "{json}");
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
    fn namen_fuer_vorschlaege() {
        // Die letzte Zeile ist unfertig, wie beim Tippen: Die Namen kommen trotzdem.
        let quelle = [
            "BRAUCH mathe OIDA",                                           // 0
            "BRAUCH programm OIDA",                                        // 1
            "",                                                            // 2
            "# Zählt zusammen.",                                           // 3
            "# Zweite Zeile.",                                             // 4
            "FUNKTIAUN zamm_zöhn(a ZOI, b LISTN VON ZOI) GIBT ZRUCK ZOI",  // 5
            "    GIB ZRUCK a OIDA",                                        // 6
            "PASST SCHO",                                                  // 7
            "",                                                            // 8
            "AUFZÖHLUNG Ampl",                                             // 9
            "    ROT, GRÜN",                                               // 10
            "PASST SCHO",                                                  // 11
            "",                                                            // 12
            "KLASSE Hund",                                                 // 13
            "    nam KETTN",                                               // 14
            "    oita ZOI = 3",                                            // 15
            "",                                                            // 16
            "    FUNKTIAUN bell(wia_oft ZOI) GIBT ZRUCK VIELLEICHT Hund",  // 17
            "        SOG AU drin = MEI.nam OIDA",                          // 18
            "        drin = 2 OIDA",                                       // 19
            "    PASST SCHO",                                              // 20
            "PASST SCHO",                                                  // 21
            "",                                                            // 22
            "KLASSE Dackl ERBT VON Hund",                                  // 23
            "    olle VAZEICHNIS VON KETTN UND LISTN VON Hund",            // 24
            "PASST SCHO",                                                  // 25
            "",                                                            // 26
            "FIX GRENZE = 3 OIDA",                                         // 27
            "SOG AU d Dackl = Dackl() OIDA",                               // 28
            "FIA JEDE viech IN d.olle[\"x\"] TUA",                         // 29
            "    SOG AU freind = viech.bell(1) OIDA",                      // 30
            "PASST SCHO",                                                  // 31
            "FIA i VON 1 BIS 3 TUA",                                       // 32
            "PASST SCHO",                                                  // 33
            "SOG AU zöhla = zamm_zöhn(1, ",                                // 34
        ]
        .join("\n");
        let json = untersuche(&quelle, &wb());
        let namen = feld(&json, "namen");
        for erwartet in [
            // Eine Funktion: Kopf, Erklärung, Parameter, was sie zurückgibt und über welche Zeilen sie geht.
            r#"{"name":"zamm_zöhn","art":"funktion","text":"zamm_zöhn(a ZOI, b LISTN VON ZOI) GIBT ZRUCK ZOI","doku":"Zählt zusammen.\nZweite Zeile.","aus":"","zu":"","parameter":["a","b"],"typ":"INT","eltern":"","kette":[],"jedes":false,"von":5,"bis":7,"innen":false,"raum":"","gheim":false}"#,
            // Ihre Parameter gelten nur in ihr.
            r#"{"name":"a","art":"variable","text":"Parameter von zamm_zöhn","doku":"","aus":"","zu":"","parameter":[],"typ":"INT","eltern":"","kette":[],"jedes":false,"von":5,"bis":7,"innen":true,"raum":"","gheim":false}"#,
            r#""name":"b","art":"variable","text":"Parameter von zamm_zöhn","doku":"","aus":"","zu":"","parameter":[],"typ":"[INT]""#,
            r#"{"name":"Ampl","art":"typ","text":"AUFZÖHLUNG Ampl","doku":"","aus":"","zu":"","parameter":[]"#,
            r#"{"name":"ROT","art":"wert","text":"Ampl.ROT","doku":"","aus":"","zu":"Ampl","parameter":[]"#,
            r#"{"name":"GRÜN","art":"wert","text":"Ampl.GRÜN","doku":"","aus":"","zu":"Ampl","parameter":[]"#,
            // Felder mit ihrem Typ.
            r#"{"name":"nam","art":"feld","text":"nam KETTN","doku":"","aus":"","zu":"Hund","parameter":[],"typ":"TEXT""#,
            r#"{"name":"oita","art":"feld","text":"oita ZOI = 3","doku":"","aus":"","zu":"Hund","parameter":[],"typ":"INT""#,
            r#"{"name":"olle","art":"feld","text":"olle VAZEICHNIS VON KETTN UND LISTN VON Hund","doku":"","aus":"","zu":"Dackl","parameter":[],"typ":"{TEXT:[Hund]}""#,
            // Eine Methode gehört zu ihrer Klasse; eine Klasse weiß, von wem sie erbt.
            r#"{"name":"bell","art":"methode","text":"bell(wia_oft ZOI) GIBT ZRUCK VIELLEICHT Hund","doku":"","aus":"","zu":"Hund","parameter":["wia_oft"],"typ":"?Hund","eltern":"","kette":[],"jedes":false,"von":17,"bis":20,"innen":false,"raum":"","gheim":false}"#,
            r#"{"name":"Dackl","art":"typ","text":"KLASSE Dackl ERBT VON Hund","doku":"","aus":"","zu":"","parameter":[],"typ":"","eltern":"Hund""#,
            r#"{"name":"GRENZE","art":"fix","text":"FIX GRENZE = 3 OIDA""#,
            // Variablen: in einer Methode (nur dort), mit Typ, als Element einer Liste, aus einem Aufruf.
            r#"{"name":"drin","art":"variable","text":"SOG AU drin = MEI.nam OIDA","doku":"","aus":"","zu":"","parameter":[],"typ":"","eltern":"","kette":["@","nam"],"jedes":false,"von":18,"bis":20,"innen":true,"raum":"","gheim":false}"#,
            r#""name":"wia_oft","art":"variable","text":"Parameter von bell","doku":"","aus":"","zu":"","parameter":[],"typ":"INT","eltern":"","kette":[],"jedes":false,"von":17,"bis":20,"innen":true,"raum":"","gheim":false}"#,
            r#""name":"d","art":"variable","text":"SOG AU d Dackl = Dackl() OIDA","doku":"","aus":"","zu":"","parameter":[],"typ":"Dackl","eltern":"","kette":[],"jedes":false,"von":28,"bis":34,"innen":false,"raum":"","gheim":false}"#,
            r#""name":"viech","art":"variable","text":"FIA JEDE viech IN d.olle[\"x\"] TUA","doku":"","aus":"","zu":"","parameter":[],"typ":"","eltern":"","kette":["d","olle","[]"],"jedes":true,"von":29,"bis":34,"innen":false,"raum":"","gheim":false}"#,
            r#""name":"freind","art":"variable","text":"SOG AU freind = viech.bell(1) OIDA","doku":"","aus":"","zu":"","parameter":[],"typ":"","eltern":"","kette":["viech","bell()"],"jedes":false"#,
            r#""name":"i","art":"variable","text":"FIA i VON 1 BIS 3 TUA","doku":"","aus":"","zu":"","parameter":[],"typ":"INT""#,
            r#""name":"zöhla","art":"variable","text":"SOG AU zöhla = zamm_zöhn(1,","doku":"","aus":"","zu":"","parameter":[],"typ":"","eltern":"","kette":["zamm_zöhn()"],"jedes":false,"von":34,"bis":34,"innen":false,"raum":"","gheim":false}"#,
            // Aus der Bibliothek, samt Erklärung; "VON DRAUSSN" wird nicht gezeigt.
            r#"{"name":"klemm","art":"funktion","text":"klemm VON T(wert T, untn T, obn T) GIBT ZRUCK T","doku":"Hält den Wert zwischen `untn` und `obn`: klemm(15, 0, 10) ist 10.\n(Für alles, was man mit < vergleichen kann.)","aus":"mathe","zu":"","parameter":["wert","untn","obn"],"typ":"T""#,
            r#""name":"temp_ordna","art":"funktion","text":"temp_ordna() GIBT ZRUCK KETTN","doku""#,
        ] {
            assert!(namen.contains(erwartet), "{erwartet}\nfehlt in\n{namen}");
        }
        // Der Name der Aufzählung ist keiner ihrer Werte, "drin = 2" kein Feld, und jede Variable kommt einmal.
        assert!(!namen.contains(r#""name":"Ampl","art":"wert""#) && !namen.contains(r#""name":"drin","art":"feld""#));
        assert_eq!(namen.matches(r#""name":"drin""#).count(), 1);
        // Was nicht dazugeholt ist, kommt auch nicht vor.
        assert!(!namen.contains(r#""name":"sortiert""#));
    }

    #[test]
    fn namen_kennen_gheim() {
        let quelle = [
            "GHEIM FUNKTIAUN hüf(a ZOI) GIBT ZRUCK ZOI", // 0
            "    SOG AU drin = a OIDA",                   // 1
            "    GIB ZRUCK drin OIDA",                    // 2
            "PASST SCHO",                                 // 3
            "KLASSE Konto",                               // 4
            "    inhaba KETTN",                           // 5
            "    GHEIM stand ZOI = 0",                    // 6
            "    GHEIM FUNKTIAUN prüf(b ZOI)",            // 7
            "        SOG AU innen = b OIDA",              // 8
            "    PASST SCHO",                             // 9
            "PASST SCHO",                                 // 10
            "BRAUCH mathe OIS m OIDA",                    // 11
        ]
        .join("\n");
        let json = untersuche(&quelle, &wb());
        let namen = feld(&json, "namen");
        for erwartet in [
            // In der eigenen Datei gibt es auch das GHEIME; die Funktion geht über die richtigen Zeilen.
            r#""name":"hüf","art":"funktion","text":"hüf(a ZOI) GIBT ZRUCK ZOI","doku":"","aus":"","zu":"","parameter":["a"],"typ":"INT","eltern":"","kette":[],"jedes":false,"von":0,"bis":3,"innen":false,"raum":"","gheim":false}"#,
            r#""name":"drin","art":"variable","text":"SOG AU drin = a OIDA","doku":"","aus":"","zu":"","parameter":[],"typ":"","eltern":"","kette":["a"],"jedes":false,"von":1,"bis":3,"innen":true"#,
            // Felder und Methoden merken sich, ob sie GHEIM sind.
            r#""name":"inhaba","art":"feld","text":"inhaba KETTN","doku":"","aus":"","zu":"Konto","parameter":[],"typ":"TEXT","eltern":"","kette":[],"jedes":false,"von":0,"bis":0,"innen":false,"raum":"","gheim":false}"#,
            r#""name":"stand","art":"feld","text":"stand ZOI = 0","doku":"","aus":"","zu":"Konto","parameter":[],"typ":"INT","eltern":"","kette":[],"jedes":false,"von":0,"bis":0,"innen":false,"raum":"","gheim":true}"#,
            r#""name":"prüf","art":"methode","text":"prüf(b ZOI)","doku":"","aus":"","zu":"Konto","parameter":["b"],"typ":"","eltern":"","kette":[],"jedes":false,"von":7,"bis":9,"innen":false,"raum":"","gheim":true}"#,
            r#""name":"innen","art":"variable","text":"SOG AU innen = b OIDA","doku":"","aus":"","zu":"","parameter":[],"typ":"","eltern":"","kette":["b"],"jedes":false,"von":8,"bis":9,"innen":true"#,
            // Was mit OIS dazugeholt ist, trägt den Namen des Raums.
            r#""name":"klemm","art":"funktion","text":"klemm VON T(wert T, untn T, obn T) GIBT ZRUCK T","#,
            r#""aus":"mathe","zu":"","parameter":["wert","untn","obn"],"typ":"T","eltern":"","kette":[],"jedes":false,"von":0,"bis":0,"innen":false,"raum":"m","gheim":false}"#,
        ] {
            assert!(namen.contains(erwartet), "{erwartet}\nfehlt in\n{namen}");
        }
    }

    #[test]
    fn dieselbe_variable_in_zwei_funktionen_sind_zwei() {
        let quelle = "FUNKTIAUN ans()\n    SOG AU x = 1 OIDA\nPASST SCHO\nFUNKTIAUN zwa()\n    SOG AU x = \"a\" OIDA\n    SOG AU x = \"b\" OIDA\nPASST SCHO\nSOG AU x = 2.5 OIDA\n";
        let json = untersuche(quelle, &wb());
        let namen = feld(&json, "namen");
        assert_eq!(namen.matches(r#""name":"x","art":"variable""#).count(), 3, "{namen}");
        for bereich in [r#""von":1,"bis":2,"innen":true"#, r#""von":4,"bis":6,"innen":true"#, r#""von":7,"bis":8,"innen":false"#] {
            assert!(namen.contains(bereich), "{bereich}\nfehlt in\n{namen}");
        }
    }

    #[test]
    fn typen_und_ketten_in_kurzer_schreibweise() {
        let lies = |text: &str| lexer::zerlegen_nachsichtig(text, &wb()).0;
        for (text, erwartet) in [
            ("ZOI", "INT"),
            ("VIELLEICHT Punkt", "?Punkt"),
            ("LISTN VON VIELLEICHT KETTN", "[?TEXT]"),
            ("VAZEICHNIS VON KETTN UND LISTN VON Punkt", "{TEXT:[Punkt]}"),
            ("Stapl VON ZOI", "Stapl"),
            ("FUNKTIAUN(ZOI, KETTN) GIBT ZRUCK JONA", "FN"),
            ("+ 3", ""),
        ] {
            assert_eq!(typ_lesen(&lies(text), 0).0, erwartet, "{text}");
        }
        for (text, erwartet) in [
            ("spü.gegna[0].nam OIDA", vec!["spü", "gegna", "[]", "nam"]),
            ("Punkt(1, [2, 3]) OIDA", vec!["Punkt()"]),
            ("MEI.hund.bell(3).nam + 1", vec!["@", "hund", "bell()", "nam"]),
            ("liste[0](1) OIDA", vec!["liste", "[]", "?"]),
            ("3 + a", vec![]),
        ] {
            assert_eq!(kette_lesen(&lies(text), 0), erwartet, "{text}");
        }
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
    fn strukturen_felder_und_listen() {
        // Punkt ist überall ein Typ (1), x hinter dem Punkt ein Feld (8).
        let json = untersuche("STRUKTUR Punkt\n x ZOI\nPASST SCHO\nSOG AU p = Punkt(1) OIDA\np.x = [1][0] OIDA", &wb());
        assert_eq!(
            feld(&json, "tokens"),
            "[[0,0,8,0],[0,9,5,1],[1,1,1,3],[1,3,3,1],[2,0,10,0],\
             [3,0,6,0],[3,7,1,3],[3,11,5,1],[3,17,1,4],[3,20,4,7],\
             [4,0,1,3],[4,2,1,8],[4,7,1,4],[4,10,1,4],[4,13,4,7]]"
        );
        assert_eq!(feld(&json, "fehler"), "[]");
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
