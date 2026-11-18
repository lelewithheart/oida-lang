//! IR-Erzeugung: macht aus dem geprüften Syntaxbaum LLVM-IR als Text.
//!
//! Jede Variable bekommt einen Platz am Stack (`alloca`), gelesen und
//! geschrieben wird mit `load`/`store`. Das ist einfach zu erzeugen, und
//! LLVM macht daraus beim Optimieren von selbst Register.
//!
//! Die Funktionen mit `oida_` am Anfang kommen aus der Laufzeitbibliothek
//! (`laufzeit/src/lib.rs`) und müssen zu den Deklarationen in [`KOPF`] passen.
//!
//! # Wie die Typen im Speicher aussehen
//!
//!  - `ZOI` ist `i64`, `KOMMAZOI` ist `double`, `JONA` ist `i1`.
//!  - Eine Struktur ist ein LLVM-Verbund `%sN` und liegt als Wert direkt in
//!    der Variable, im Feld oder in der Liste. Zuweisen heißt kopieren.
//!  - Ein Text und eine Liste sind Zeiger auf Speicher am Heap. Zuweisen
//!    heißt: Beide zeigen auf dasselbe.
//!  - Ein Objekt einer Klasse ist ein Zeiger auf einen Verbund `%kN` am Heap:
//!    Zähler, Zeiger auf die Angaben zur Klasse, dann die Felder (die
//!    geerbten zuerst; darum passt ein Objekt auch dort, wo die Elternklasse
//!    verlangt ist).
//!
//!  - `VIELLEICHT T`: Ist `T` ohnehin ein Zeiger (Text, Liste, Objekt), dann
//!    derselbe Zeiger, und NIX ist der Nullzeiger. Sonst ein Verbund
//!    `%vN { i1 ist_da, T wert }`.
//!  - `T ODA FEHLA` ist ein Verbund `%eN { ptr fehler, T wert }`: Ist `fehler`
//!    null, gilt der Wert, sonst zeigt er auf den Text mit der Meldung.
//!    Ein `FEHLA` allein ist dieser Text.
//!
//!  - Eine Funktion als Wert ist ein Zeiger auf eine Hülle am Heap, die wie
//!    ein Objekt beginnt: `%hN { i64 zaehler, ptr angaben, ptr code, ... }`.
//!    Dahinter liegen die Werte, die eine Funktion ohne Namen von außen
//!    mitgenommen hat. Der Code bekommt die Hülle als ersten Wert.
//!
//!  - Ein Verzeichnis ist ein Zeiger auf `%oida.Wb`: Zähler, Länge, die
//!    Schlüssel (je 8 Bytes: eine Zahl oder ein Zeiger auf einen Text) und
//!    die Werte dicht hintereinander. Suchen, anhängen und entfernen macht
//!    die Laufzeit; die Werte liest und schreibt der erzeugte Code selbst.
//!
//! # Klassen und Methoden
//!
//! Eine Methode ist eine Funktion, die das Objekt als ersten Wert bekommt
//! (`SÖWA`, nur geliehen). Zu jeder Klasse gibt es Angaben `@oida.klasse.N`:
//! wie man ein Objekt freigibt, wie man es ausgibt, und die Sprungtabelle.
//! In der Tabelle stehen nur Methoden, die irgendwo in der Familie
//! überschrieben werden; bei allen anderen (und überall, wo es darunter keine
//! andere Fassung gibt) steht schon beim Übersetzen fest, welche läuft.
//!
//! # Referenzzählung
//!
//! Texte und Listen tragen einen Zähler: wie viele Stellen sie gerade
//! brauchen. Fällt er auf 0, werden sie freigegeben. "Verwaltet" heißt ein
//! Typ, an dem so etwas hängt: Texte, Listen, Objekte und Strukturen, die
//! welche enthalten. Die Regeln:
//!
//!  - Ein Ausdruck liefert einen Wert, der dem Empfänger **gehört** (schon
//!    mitgezählt): [`FunktionsBau::ausdruck`].
//!  - Wer nur liest (vergleichen, zusammenhängen, ausgeben), kann **leihen**:
//!    [`FunktionsBau::geliehen`]. Eine Variable, ein Feld davon oder ein
//!    fester Text wird dann nicht gezählt. Aus einer Liste und aus dem Feld
//!    eines Objekts wird nie geliehen, weil eine aufgerufene Funktion die
//!    Liste oder das Objekt ändern könnte.
//!  - Eine Variable besitzt ihren Wert. Beim Zuweisen wird der alte
//!    losgelassen, am Ende des Blocks der letzte.
//!  - Eine Funktion besitzt ihre Parameter und lässt sie am Ende los. Was sie
//!    zurückgibt, gehört dem Aufrufer.
//!
//! Feste Texte aus dem Programm haben den Zähler -1 und werden nie gezählt.

use std::collections::HashMap;
use std::fmt::Write;

use crate::ast::*;
use crate::woerterbuch::{Schluessel, Woerterbuch};

/// Für dieses Ziel wird Code erzeugt (Windows, 64 Bit, llvm-mingw).
pub const ZIEL: &str = "x86_64-w64-windows-gnu";

const KOPF: &str = r#"; Erzeugt vom OIDA-Compiler. Nicht von Hand ändern.
target triple = "x86_64-w64-windows-gnu"

; Eine Liste am Heap: Zähler, Länge, Platz (für wie viele Elemente Speicher da ist), Daten.
%oida.Liste = type { i64, i64, i64, ptr }

declare void @oida_start()
declare void @oida_ende()
declare void @oida_schreib_zoi(i64)
declare void @oida_schreib_kommazoi(double)
declare void @oida_schreib_text(ptr)
declare void @oida_schreib_text_zitiert(ptr)
declare void @oida_schreib_leer()
declare void @oida_schreib_zeilenende()
declare ptr @oida_text_verbinden(ptr, ptr)
declare ptr @oida_text_verbinden_viele(ptr, i64)
declare i32 @oida_text_vergleich(ptr, ptr)
declare void @oida_text_frei(ptr)
declare ptr @oida_zoi_zu_text(i64)
declare ptr @oida_kommazoi_zu_text(double)
declare i64 @oida_text_zu_zoi(ptr, i64)
declare double @oida_text_zu_kommazoi(ptr, i64)
declare ptr @oida_text_als_zoi(ptr, ptr)
declare ptr @oida_text_als_kommazoi(ptr, ptr)
declare void @oida_fehler_durch_null(i64) noreturn
declare void @oida_fehler_schritt_null(i64) noreturn
declare void @oida_fehler_stelle(i64, i64, i64) noreturn
declare i64 @llvm.fptosi.sat.i64.f64(double)

; Listen
declare ptr @oida_liste_neu(i64, i64)
declare ptr @oida_liste_neuer_platz(ptr, i64)
declare void @oida_liste_raus(ptr, i64, i64, i64)
declare void @oida_liste_frei(ptr, i64)

; Verzeichnisse (Schlüssel -> Wert): Zähler, Länge, Schlüssel, Werte. (Dahinter kommt noch, was nur die Laufzeit braucht.)
%oida.Wb = type { i64, i64, ptr, ptr }
declare ptr @oida_wb_neu(i64)
declare i64 @oida_wb_finde(ptr, i64)
declare i64 @oida_wb_dazu(ptr, i64, i64)
declare void @oida_wb_raus(ptr, i64, i64)
declare ptr @oida_wb_schluessel(ptr)
declare void @oida_wb_frei(ptr, i64)

; Objekte
declare ptr @oida_objekt_neu(i64)
declare void @oida_objekt_frei(ptr, i64)

; Eingebaute Funktionen: Eingabe, Texte, Zufall
declare ptr @oida_lies(ptr)
declare i64 @oida_text_laenge(ptr)
declare ptr @oida_text_stueck(ptr, i64, i64, i64)
declare i64 @oida_text_suche(ptr, ptr)
declare ptr @oida_text_gross(ptr)
declare ptr @oida_text_klein(ptr)
declare ptr @oida_text_gestutzt(ptr)
declare ptr @oida_text_ersetzt(ptr, ptr, ptr, i64)
declare double @oida_zufall()
declare i64 @oida_zufall_zoi(i64, i64, i64)
declare void @oida_zufall_samen(i64)

; Eingebaute Funktionen: Rechnen (das meiste kann der Prozessor oder LLVM selbst)
declare double @llvm.sqrt.f64(double)
declare double @llvm.pow.f64(double, double)
declare double @llvm.sin.f64(double)
declare double @llvm.cos.f64(double)
declare double @llvm.exp.f64(double)
declare double @llvm.floor.f64(double)
declare double @llvm.ceil.f64(double)
declare double @llvm.round.f64(double)
declare double @llvm.fabs.f64(double)
declare double @llvm.minnum.f64(double, double)
declare double @llvm.maxnum.f64(double, double)
declare i64 @llvm.abs.i64(i64, i1)
declare i64 @llvm.smin.i64(i64, i64)
declare i64 @llvm.smax.i64(i64, i64)
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare double @atan2(double, double)

; Teilen von ganzen Zahlen: durch 0 gibt eine Meldung statt Unsinn,
; und "kleinste Zahl / -1" läuft nicht über.
define internal i64 @oida.durch(i64 %a, i64 %b, i64 %zeile) alwaysinline {
start:
  %null = icmp eq i64 %b, 0
  br i1 %null, label %fehler, label %ok
fehler:
  call void @oida_fehler_durch_null(i64 %zeile)
  unreachable
ok:
  %minus1 = icmp eq i64 %b, -1
  br i1 %minus1, label %negativ, label %normal
negativ:
  %n = sub i64 0, %a
  ret i64 %n
normal:
  %q = sdiv i64 %a, %b
  ret i64 %q
}

define internal i64 @oida.rest(i64 %a, i64 %b, i64 %zeile) alwaysinline {
start:
  %null = icmp eq i64 %b, 0
  br i1 %null, label %fehler, label %ok
fehler:
  call void @oida_fehler_durch_null(i64 %zeile)
  unreachable
ok:
  %minus1 = icmp eq i64 %b, -1
  br i1 %minus1, label %negativ, label %normal
negativ:
  ret i64 0
normal:
  %r = srem i64 %a, %b
  ret i64 %r
}

; Referenzzählung. Der Zähler steht bei Texten und Listen ganz vorne.
; Negativ heißt: fester Text aus dem Programm, nicht zählen.
define internal void @oida.behalt(ptr %ding) alwaysinline {
start:
  %z = load i64, ptr %ding
  %fest = icmp slt i64 %z, 0
  br i1 %fest, label %ende, label %zaehlen
zaehlen:
  %mehr = add i64 %z, 1
  store i64 %mehr, ptr %ding
  br label %ende
ende:
  ret void
}

; Einen Text loslassen. (Für Listen gibt es je Listentyp eine eigene
; Funktion, weil dabei auch die Elemente losgelassen werden.)
define internal void @oida.lass(ptr %text) alwaysinline {
start:
  %z = load i64, ptr %text
  %fest = icmp slt i64 %z, 0
  br i1 %fest, label %ende, label %zaehlen
zaehlen:
  %weniger = sub i64 %z, 1
  store i64 %weniger, ptr %text
  %keiner = icmp eq i64 %weniger, 0
  br i1 %keiner, label %frei, label %ende
frei:
  call void @oida_text_frei(ptr %text)
  br label %ende
ende:
  ret void
}

; Ein Objekt wird an einer weiteren Stelle gebraucht. Objekte liegen immer
; am Heap, der Zähler ist nie negativ.
define internal void @oida.behalt.objekt(ptr %objekt) alwaysinline {
start:
  %z = load i64, ptr %objekt
  %mehr = add i64 %z, 1
  store i64 %mehr, ptr %objekt
  ret void
}

; Ein Objekt loslassen, von dem erst beim Laufen feststeht, von welcher
; Klasse es genau ist. Wie es freigegeben wird, steht in den Angaben zu
; seiner Klasse ganz vorne.
define internal void @oida.lass.objekt(ptr %objekt) {
start:
  %z = load i64, ptr %objekt
  %weniger = sub i64 %z, 1
  store i64 %weniger, ptr %objekt
  %keiner = icmp eq i64 %weniger, 0
  br i1 %keiner, label %frei, label %ende
frei:
  %kp = getelementptr inbounds { i64, ptr }, ptr %objekt, i32 0, i32 1
  %klasse = load ptr, ptr %kp
  %freigeben = load ptr, ptr %klasse
  call void %freigeben(ptr %objekt)
  br label %ende
ende:
  ret void
}

; Ein Objekt ausgeben, so wie es seine Klasse vorgibt.
define internal void @oida.schreib.objekt(ptr %objekt) {
start:
  %kp = getelementptr inbounds { i64, ptr }, ptr %objekt, i32 0, i32 1
  %klasse = load ptr, ptr %kp
  %sp = getelementptr inbounds ptr, ptr %klasse, i64 1
  %schreiben = load ptr, ptr %sp
  call void %schreiben(ptr %objekt)
  ret void
}

; Wie viele Objekte gerade ineinander ausgegeben werden. Ab einer gewissen
; Tiefe steht nur noch <Name> da: Objekte können im Kreis aufeinander zeigen.
@oida.schreib.tiefe = internal global i64 0

; Die Daten einer Liste, nachdem geprüft ist, dass es die Stelle gibt.
define internal ptr @oida.stelle(ptr %liste, i64 %stelle, i64 %zeile) alwaysinline {
start:
  %lp = getelementptr inbounds %oida.Liste, ptr %liste, i32 0, i32 1
  %laenge = load i64, ptr %lp, !tbaa !3
  %drin = icmp ult i64 %stelle, %laenge
  br i1 %drin, label %gut, label %daneben
daneben:
  call void @oida_fehler_stelle(i64 %stelle, i64 %laenge, i64 %zeile)
  unreachable
gut:
  %dp = getelementptr inbounds %oida.Liste, ptr %liste, i32 0, i32 3
  %daten = load ptr, ptr %dp, !tbaa !3
  ret ptr %daten
}

"#;

/// Hinweise für LLVM, was sich im Speicher nicht überschneiden kann (TBAA):
/// Der Kopf einer Liste (Länge, Platz, Zeiger auf die Daten) ist anderer
/// Speicher als die Werte in Variablen und in Listen. Wer ein Element
/// schreibt, ändert also weder die Länge noch den Datenzeiger, und LLVM muss
/// beides in einer Schleife nicht bei jedem Zugriff neu lesen.
///
/// `!3` steht an Zugriffen auf den Kopf, `!4` an Zugriffen auf Werte.
const FUSS: &str = r#"
!0 = !{!"oida"}
!1 = !{!"oida listenkopf", !0}
!2 = !{!"oida wert", !0}
!3 = !{!1, !1, i64 0}
!4 = !{!2, !2, i64 0}
"#;

/// Der Zusatz für `load` und `store` eines Werts in einer Variable oder Liste
/// (siehe [`FUSS`]). Nur bei einfachen Werten; bei ganzen Strukturen bleibt er weg.
fn wert_hinweis(typ: Typ) -> &'static str {
    match typ {
        Typ::Struktur(_) | Typ::Vielleicht { zeiger: false, .. } | Typ::Ergebnis(_) => "",
        _ => ", !tbaa !4",
    }
}

/// Der Zusatz für das Lesen aus dem Kopf einer Liste.
const KOPF_HINWEIS: &str = ", !tbaa !3";

fn ir_typ(typ: Typ) -> String {
    match typ {
        Typ::Zoi => "i64".to_string(),
        Typ::Kommazoi => "double".to_string(),
        Typ::Jona => "i1".to_string(),
        Typ::Kettn | Typ::Liste(_) | Typ::LeereListe | Typ::Klasse(_) | Typ::Fehla => "ptr".to_string(),
        Typ::Funktion(_) | Typ::Verzeichnis(_) => "ptr".to_string(),
        Typ::Vielleicht { zeiger: true, .. } => "ptr".to_string(),
        Typ::Vielleicht { nummer, zeiger: false } => format!("%v{nummer}"),
        Typ::Ergebnis(n) => format!("%e{n}"),
        Typ::Struktur(n) => format!("%s{n}"),
        Typ::Nix => "void".to_string(),
        Typ::NixWert | Typ::Fehler => unreachable!("nach der Typprüfung gibt es keine offenen Typen mehr"),
    }
}

/// Wie viele Bytes ein Wert dieses Typs im Speicher braucht, als Konstante,
/// die LLVM ausrechnet (so muss der Compiler den Aufbau nicht selbst kennen).
fn groesse(typ: Typ) -> String {
    format!("ptrtoint (ptr getelementptr ({}, ptr null, i32 1) to i64)", ir_typ(typ))
}

/// Wie viele Bytes ein Objekt der Klasse am Heap braucht.
fn objekt_groesse(klasse: u32) -> String {
    format!("ptrtoint (ptr getelementptr (%k{klasse}, ptr null, i32 1) to i64)")
}

/// Der Aufbau der Angaben zu einer Klasse: freigeben, ausgeben, Sprungtabelle.
fn klassen_angaben(plaetze: usize) -> String {
    format!("{{ ptr, ptr, [{plaetze} x ptr] }}")
}

/// Kommt dieser Typ aus den Tabellen im fertigen Programm wirklich vor?
/// (Manche Einträge sind nur Zwischenstände der Typprüfung.)
fn brauchbar(typ: Typ) -> bool {
    !matches!(typ, Typ::Fehler | Typ::NixWert | Typ::LeereListe)
}

/// Gibt es eine Klasse, die von dieser erbt?
fn hat_kinder(typen: &Typen, klasse: u32) -> bool {
    typen.klassen.iter().any(|k| k.eltern == Some(klasse))
}

/// Gibt es unterhalb der Klasse eine eigene Fassung der Methode? Nur dann
/// muss beim Aufruf in der Sprungtabelle nachgeschaut werden.
fn weiter_unten_ersetzt(typen: &Typen, klasse: u32, name: &str) -> bool {
    typen.klassen.iter().enumerate().any(|(n, k)| {
        let n = n as u32;
        n != klasse && typen.erbt_von(n, klasse) && k.methoden.iter().any(|m| m.klasse == n && m.name == name)
    })
}

/// Der Befehl, der den Wert für eine weitere Stelle mitzählt (keiner, wenn es nichts zu zählen gibt).
fn behalt_befehl(typen: &Typen, typ: Typ, wert: &str) -> Option<String> {
    match typ {
        Typ::Kettn | Typ::Liste(_) | Typ::Fehla | Typ::Verzeichnis(_) => {
            Some(format!("call void @oida.behalt(ptr {wert})"))
        }
        // Die Hülle einer Funktion beginnt wie ein Objekt.
        Typ::Klasse(_) | Typ::Funktion(_) => Some(format!("call void @oida.behalt.objekt(ptr {wert})")),
        Typ::Vielleicht { nummer, .. } if typen.verwaltet(typ) => {
            Some(format!("call void @oida.behalt.v{nummer}({} {wert})", ir_typ(typ)))
        }
        Typ::Ergebnis(n) => Some(format!("call void @oida.behalt.e{n}(%e{n} {wert})")),
        Typ::Struktur(n) if typen.verwaltet(typ) => Some(format!("call void @oida.behalt.s{n}(%s{n} {wert})")),
        _ => None,
    }
}

/// Der Befehl, der den Wert loslässt.
fn lass_befehl(typen: &Typen, typ: Typ, wert: &str) -> Option<String> {
    match typ {
        Typ::Kettn | Typ::Fehla => Some(format!("call void @oida.lass(ptr {wert})")),
        Typ::Vielleicht { nummer, .. } if typen.verwaltet(typ) => {
            Some(format!("call void @oida.lass.v{nummer}({} {wert})", ir_typ(typ)))
        }
        Typ::Ergebnis(n) => Some(format!("call void @oida.lass.e{n}(%e{n} {wert})")),
        Typ::Liste(n) => Some(format!("call void @oida.lass.l{n}(ptr {wert})")),
        Typ::Funktion(_) => Some(format!("call void @oida.lass.objekt(ptr {wert})")),
        Typ::Verzeichnis(n) => Some(format!("call void @oida.lass.w{n}(ptr {wert})")),
        // Kann es nur genau diese Klasse sein, steht fest, wie freigegeben wird.
        Typ::Klasse(n) if hat_kinder(typen, n) => Some(format!("call void @oida.lass.objekt(ptr {wert})")),
        Typ::Klasse(n) => Some(format!("call void @oida.lass.k{n}(ptr {wert})")),
        Typ::Struktur(n) if typen.verwaltet(typ) => Some(format!("call void @oida.lass.s{n}(%s{n} {wert})")),
        _ => None,
    }
}

/// Der Name einer OIDA-Funktion im IR. Der Vorsatz verhindert, dass sich
/// eine OIDA-Funktion mit einer aus C (`main`, `printf` ...) verwechselt.
fn funktions_name(name: &str) -> String {
    ir_name("f", &[name])
}

/// Der Name einer Methode im IR.
fn methoden_name(klasse: &str, name: &str) -> String {
    ir_name("m", &[klasse, name])
}

fn ir_name(art: &str, teile: &[&str]) -> String {
    let mut aus = format!("@\"oida.{art}");
    for teil in teile {
        aus.push('.');
        for b in teil.bytes() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                aus.push(b as char);
            } else {
                let _ = write!(aus, "\\{b:02X}");
            }
        }
    }
    aus.push('"');
    aus
}

struct Modul<'t> {
    /// Feste Texte aus dem Programm, jeder nur einmal.
    texte: Vec<String>,
    text_nummer: HashMap<String, usize>,
    woahr: String,
    foisch: String,
    /// Die Wörter für NIX, FEHLA und FUNKTIAUN (fürs Ausgeben).
    nix: String,
    fehla: String,
    funktion_wort: String,
    /// Was beim Übersetzen der Funktionen noch dazukommt: der Code der
    /// Funktionen ohne Namen, ihre Hüllen und die Umwege für Funktionen mit Namen.
    zusatz: String,
    /// Die Funktionen ohne Namen, deren Code schon erzeugt ist.
    lambdas_gebaut: std::collections::HashSet<usize>,
    /// Die Funktionen mit Namen, für die es den Umweg als Wert schon gibt.
    werte_gebaut: std::collections::HashSet<String>,
    typen: &'t Typen,
    /// Die Klassen, wie sie im Programm stehen (für die Werte, mit denen Felder beginnen).
    klassen: &'t [KlassenDef],
}

impl Modul<'_> {
    /// Der IR-Name für einen festen Text.
    fn text(&mut self, inhalt: &str) -> String {
        let nummer = match self.text_nummer.get(inhalt) {
            Some(n) => *n,
            None => {
                let n = self.texte.len();
                self.texte.push(inhalt.to_string());
                self.text_nummer.insert(inhalt.to_string(), n);
                n
            }
        };
        format!("@.text.{nummer}")
    }

    /// Die Befehle, die einen Wert ausgeben. `innen` heißt: Der Wert steht in
    /// einer Liste oder Struktur; Texte bekommen dort Anführungszeichen.
    /// `name` macht den Zwischenwert eindeutig.
    fn schreib_befehle(&mut self, typ: Typ, wert: &str, innen: bool, name: &str) -> String {
        match typ {
            Typ::Zoi => format!("  call void @oida_schreib_zoi(i64 {wert})\n"),
            Typ::Kommazoi => format!("  call void @oida_schreib_kommazoi(double {wert})\n"),
            Typ::Kettn if innen => format!("  call void @oida_schreib_text_zitiert(ptr {wert})\n"),
            Typ::Kettn => format!("  call void @oida_schreib_text(ptr {wert})\n"),
            Typ::Jona => {
                let (woahr, foisch) = (self.woahr.clone(), self.foisch.clone());
                let (woahr, foisch) = (self.text(&woahr), self.text(&foisch));
                format!(
                    "  %{name} = select i1 {wert}, ptr {woahr}, ptr {foisch}\n  call void @oida_schreib_text(ptr %{name})\n"
                )
            }
            Typ::Struktur(n) => format!("  call void @oida.schreib.s{n}(%s{n} {wert})\n"),
            Typ::Liste(n) => format!("  call void @oida.schreib.l{n}(ptr {wert})\n"),
            Typ::Klasse(_) | Typ::Funktion(_) => format!("  call void @oida.schreib.objekt(ptr {wert})\n"),
            Typ::Verzeichnis(n) => format!("  call void @oida.schreib.w{n}(ptr {wert})\n"),
            Typ::Fehla => {
                let anfang = format!("{}(", self.fehla);
                let (anfang, ende) = (self.text(&anfang), self.text(")"));
                format!(
                    "  call void @oida_schreib_text(ptr {anfang})\n  call void @oida_schreib_text_zitiert(ptr {wert})\n  \
                     call void @oida_schreib_text(ptr {ende})\n"
                )
            }
            Typ::Vielleicht { nummer, .. } => format!("  call void @oida.schreib.v{nummer}({} {wert})\n", ir_typ(typ)),
            Typ::Ergebnis(n) => format!("  call void @oida.schreib.e{n}(%e{n} {wert})\n"),
            Typ::LeereListe | Typ::NixWert | Typ::Nix | Typ::Fehler => unreachable!("von der Typprüfung abgelehnt"),
        }
    }

    /// Die Hilfsfunktionen je Struktur, je Klasse und je Listentyp:
    /// mitzählen, loslassen und ausgeben.
    fn hilfsfunktionen(&mut self) -> String {
        let typen = self.typen;
        let mut aus = String::new();
        let zaehlen: [(&str, fn(&Typen, Typ, &str) -> Option<String>); 2] =
            [("behalt", behalt_befehl), ("lass", lass_befehl)];
        let nix = self.nix.clone();
        let nix = self.text(&nix);

        // Funktionen als Werte: ausgeben, und die Hülle einer Funktion mit Namen (ohne mitgenommene Werte).
        let funktion = format!("<{}>", self.funktion_wort);
        let funktion = self.text(&funktion);
        let _ = writeln!(
            aus,
            "define internal void @oida.schreib.funktion(ptr %huelle) {{\nstart:\n  \
             call void @oida_schreib_text(ptr {funktion})\n  ret void\n}}\n\n\
             define internal void @oida.frei.fest(ptr %huelle) {{\nstart:\n  \
             call void @oida_objekt_frei(ptr %huelle, i64 24)\n  ret void\n}}\n\n\
             @oida.huelle.fest = internal constant {{ ptr, ptr }} {{ ptr @oida.frei.fest, ptr @oida.schreib.funktion }}\n"
        );

        // Verzeichnisse: loslassen (samt den Werten) und ausgeben als {schlüssel: wert, ...}.
        for (n, (k, w)) in typen.verzeichnisse.iter().enumerate() {
            if !brauchbar(*k) || !brauchbar(*w) {
                continue;
            }
            let (kt, wt) = (ir_typ(*k), ir_typ(*w));
            let lesen = "  %lp = getelementptr inbounds %oida.Wb, ptr %v, i32 0, i32 1\n  %laenge = load i64, ptr %lp\n  \
                         %wp = getelementptr inbounds %oida.Wb, ptr %v, i32 0, i32 3\n  %werte = load ptr, ptr %wp\n";
            let _ = writeln!(aus, "define internal void @oida.lass.w{n}(ptr %v) {{\nstart:");
            aus.push_str(
                "  %z = load i64, ptr %v\n  %weniger = sub i64 %z, 1\n  store i64 %weniger, ptr %v\n  \
                 %keiner = icmp eq i64 %weniger, 0\n  br i1 %keiner, label %frei, label %ende\nfrei:\n",
            );
            if let Some(befehl) = lass_befehl(typen, *w, "%e") {
                aus.push_str(lesen);
                let _ = writeln!(
                    aus,
                    "  br label %kopf\nkopf:\n  %i = phi i64 [ 0, %frei ], [ %i2, %rumpf ]\n  \
                     %weiter = icmp slt i64 %i, %laenge\n  br i1 %weiter, label %rumpf, label %raus\nrumpf:\n  \
                     %p = getelementptr inbounds {wt}, ptr %werte, i64 %i\n  %e = load {wt}, ptr %p\n  {befehl}\n  \
                     %i2 = add i64 %i, 1\n  br label %kopf\nraus:"
                );
            }
            let _ = writeln!(
                aus,
                "  call void @oida_wb_frei(ptr %v, i64 {})\n  br label %ende\nende:\n  ret void\n}}\n",
                groesse(*w)
            );

            let (auf, komma, doppelpunkt, zu) = (self.text("{"), self.text(", "), self.text(": "), self.text("}"));
            let _ = writeln!(aus, "define internal void @oida.schreib.w{n}(ptr %v) {{\nstart:");
            let _ = writeln!(aus, "  call void @oida_schreib_text(ptr {auf})");
            aus.push_str(lesen);
            let _ = writeln!(
                aus,
                "  %sp = getelementptr inbounds %oida.Wb, ptr %v, i32 0, i32 2\n  %schluessel = load ptr, ptr %sp\n  \
                 br label %kopf\nkopf:\n  %i = phi i64 [ 0, %start ], [ %i2, %eintrag ]\n  \
                 %weiter = icmp slt i64 %i, %laenge\n  br i1 %weiter, label %rumpf, label %ende\nrumpf:\n  \
                 %erstes = icmp eq i64 %i, 0\n  br i1 %erstes, label %eintrag, label %komma\nkomma:\n  \
                 call void @oida_schreib_text(ptr {komma})\n  br label %eintrag\neintrag:\n  \
                 %kp = getelementptr inbounds i64, ptr %schluessel, i64 %i\n  %k = load {kt}, ptr %kp"
            );
            aus.push_str(&self.schreib_befehle(*k, "%k", true, "jk"));
            let _ = writeln!(
                aus,
                "  call void @oida_schreib_text(ptr {doppelpunkt})\n  \
                 %p = getelementptr inbounds {wt}, ptr %werte, i64 %i\n  %e = load {wt}, ptr %p"
            );
            aus.push_str(&self.schreib_befehle(*w, "%e", true, "jw"));
            let _ = writeln!(
                aus,
                "  %i2 = add i64 %i, 1\n  br label %kopf\nende:\n  call void @oida_schreib_text(ptr {zu})\n  ret void\n}}\n"
            );
        }

        // VIELLEICHT: nur etwas tun, wenn etwas da ist.
        for (n, innen) in typen.vielleicht.iter().enumerate() {
            if !brauchbar(*innen) || *innen == Typ::Nix {
                continue;
            }
            let zeiger = typen.zeiger(*innen);
            let t = ir_typ(Typ::Vielleicht { nummer: n as u32, zeiger });
            let da = if zeiger { "%da = icmp ne ptr %w, null".to_string() } else { format!("%da = extractvalue {t} %w, 0") };
            let (holen, wert) =
                if zeiger { (String::new(), "%w") } else { (format!("  %innen = extractvalue {t} %w, 1\n"), "%innen") };
            if typen.verwaltet(*innen) {
                for (wort, erzeuge) in zaehlen {
                    let befehl = erzeuge(typen, *innen, wert).expect("ein verwalteter Typ hat einen Befehl");
                    let _ = writeln!(
                        aus,
                        "define internal void @oida.{wort}.v{n}({t} %w) {{\nstart:\n  {da}\n  \
                         br i1 %da, label %ja, label %ende\nja:\n{holen}  {befehl}\n  br label %ende\nende:\n  ret void\n}}\n"
                    );
                }
            }
            let schreiben = self.schreib_befehle(*innen, wert, true, "j");
            let _ = writeln!(
                aus,
                "define internal void @oida.schreib.v{n}({t} %w) {{\nstart:\n  {da}\n  \
                 br i1 %da, label %ja, label %nix\nja:\n{holen}{schreiben}  ret void\nnix:\n  \
                 call void @oida_schreib_text(ptr {nix})\n  ret void\n}}\n"
            );
        }

        // ... ODA FEHLA: Es zählt entweder der Wert oder der Fehler.
        for (n, wert) in typen.ergebnisse.iter().enumerate() {
            if !brauchbar(*wert) {
                continue;
            }
            let anfang = format!(
                "start:\n  %f = extractvalue %e{n} %w, 0\n  %gut = icmp eq ptr %f, null\n  \
                 br i1 %gut, label %wert, label %fehler\nwert:"
            );
            for (wort, erzeuge) in zaehlen {
                let _ = writeln!(aus, "define internal void @oida.{wort}.e{n}(%e{n} %w) {{\n{anfang}");
                if *wert != Typ::Nix
                    && let Some(befehl) = erzeuge(typen, *wert, "%innen")
                {
                    let _ = writeln!(aus, "  %innen = extractvalue %e{n} %w, 1\n  {befehl}");
                }
                let fehler = erzeuge(typen, Typ::Fehla, "%f").expect("ein Fehler ist ein Text");
                let _ = writeln!(aus, "  ret void\nfehler:\n  {fehler}\n  ret void\n}}\n");
            }
            let _ = writeln!(aus, "define internal void @oida.schreib.e{n}(%e{n} %w) {{\n{anfang}");
            if *wert == Typ::Nix {
                let _ = writeln!(aus, "  call void @oida_schreib_text(ptr {nix})");
            } else {
                let _ = writeln!(aus, "  %innen = extractvalue %e{n} %w, 1");
                aus.push_str(&self.schreib_befehle(*wert, "%innen", true, "j"));
            }
            aus.push_str("  ret void\nfehler:\n");
            aus.push_str(&self.schreib_befehle(Typ::Fehla, "%f", true, "jf"));
            aus.push_str("  ret void\n}\n\n");
        }

        for (n, k) in typen.klassen.iter().enumerate() {
            if k.vorlage {
                continue; // Aus einer Vorlage entsteht selbst kein Code.
            }
            // Freigeben: erst loslassen, was in den Feldern steckt, dann das Objekt selbst.
            let _ = writeln!(aus, "define internal void @oida.frei.k{n}(ptr %objekt) {{\nstart:");
            for (i, feld) in k.felder.iter().enumerate() {
                if let Some(befehl) = lass_befehl(typen, feld.typ, &format!("%f{i}")) {
                    let _ = writeln!(
                        aus,
                        "  %p{i} = getelementptr inbounds %k{n}, ptr %objekt, i32 0, i32 {}\n  \
                         %f{i} = load {}, ptr %p{i}\n  {befehl}",
                        i + 2,
                        ir_typ(feld.typ)
                    );
                }
            }
            let _ = writeln!(
                aus,
                "  call void @oida_objekt_frei(ptr %objekt, i64 {})\n  ret void\n}}\n",
                objekt_groesse(n as u32)
            );

            // Loslassen, wenn feststeht, dass es genau diese Klasse ist.
            if !hat_kinder(typen, n as u32) {
                let _ = writeln!(
                    aus,
                    "define internal void @oida.lass.k{n}(ptr %objekt) {{\nstart:\n  \
                     %z = load i64, ptr %objekt\n  %weniger = sub i64 %z, 1\n  store i64 %weniger, ptr %objekt\n  \
                     %keiner = icmp eq i64 %weniger, 0\n  br i1 %keiner, label %frei, label %ende\nfrei:\n  \
                     call void @oida.frei.k{n}(ptr %objekt)\n  br label %ende\nende:\n  ret void\n}}\n"
                );
            }

            // Ausgeben: Name(feld, feld, ...), zu tief drinnen nur <Name>.
            let kurz = self.text(&format!("<{}>", k.name));
            let anfang = self.text(&format!("{}(", k.name));
            let (komma, ende) = (self.text(", "), self.text(")"));
            let _ = writeln!(
                aus,
                "define internal void @oida.schreib.k{n}(ptr %objekt) {{\nstart:\n  \
                 %tiefe = load i64, ptr @oida.schreib.tiefe\n  %zu_tief = icmp sgt i64 %tiefe, 3\n  \
                 br i1 %zu_tief, label %kurz, label %ganz\nkurz:\n  \
                 call void @oida_schreib_text(ptr {kurz})\n  ret void\nganz:\n  \
                 %tiefer = add i64 %tiefe, 1\n  store i64 %tiefer, ptr @oida.schreib.tiefe\n  \
                 call void @oida_schreib_text(ptr {anfang})"
            );
            for (i, feld) in k.felder.iter().enumerate() {
                if i > 0 {
                    let _ = writeln!(aus, "  call void @oida_schreib_text(ptr {komma})");
                }
                let _ = writeln!(
                    aus,
                    "  %p{i} = getelementptr inbounds %k{n}, ptr %objekt, i32 0, i32 {}\n  %f{i} = load {}, ptr %p{i}",
                    i + 2,
                    ir_typ(feld.typ)
                );
                aus.push_str(&self.schreib_befehle(feld.typ, &format!("%f{i}"), true, &format!("j{i}")));
            }
            let _ = writeln!(
                aus,
                "  call void @oida_schreib_text(ptr {ende})\n  store i64 %tiefe, ptr @oida.schreib.tiefe\n  ret void\n}}\n"
            );

            // Die Angaben zur Klasse. In der Sprungtabelle steht je Platz die
            // Fassung der Methode, die für diese Klasse gilt.
            let mut tabelle: Vec<String> = vec!["ptr null".to_string(); k.plaetze];
            for m in &k.methoden {
                if let Some(platz) = m.platz {
                    tabelle[platz] = format!("ptr {}", methoden_name(&typen.klassen[m.klasse as usize].name, &m.name));
                }
            }
            let tabelle =
                if tabelle.is_empty() { "zeroinitializer".to_string() } else { format!("[{}]", tabelle.join(", ")) };
            let _ = writeln!(
                aus,
                "@oida.klasse.{n} = internal constant {} {{ ptr @oida.frei.k{n}, ptr @oida.schreib.k{n}, [{} x ptr] {tabelle} }}\n",
                klassen_angaben(k.plaetze),
                k.plaetze
            );
        }

        for (n, s) in typen.strukturen.iter().enumerate() {
            let typ = Typ::Struktur(n as u32);
            // Mitzählen und loslassen: nur nötig, wenn Texte oder Listen drinstecken.
            if typen.verwaltet(typ) {
                let arten: [(&str, fn(&Typen, Typ, &str) -> Option<String>); 2] =
                    [("behalt", behalt_befehl), ("lass", lass_befehl)];
                for (wort, erzeuge) in arten {
                    let _ = writeln!(aus, "define internal void @oida.{wort}.s{n}(%s{n} %w) {{\nstart:");
                    for (i, (_, feld)) in s.felder.iter().enumerate() {
                        if let Some(befehl) = erzeuge(typen, *feld, &format!("%f{i}")) {
                            let _ = writeln!(aus, "  %f{i} = extractvalue %s{n} %w, {i}\n  {befehl}");
                        }
                    }
                    aus.push_str("  ret void\n}\n\n");
                }
            }
            // Ausgeben: Name(feld, feld, ...)
            let anfang = self.text(&format!("{}(", s.name));
            let (komma, ende) = (self.text(", "), self.text(")"));
            let _ = writeln!(aus, "define internal void @oida.schreib.s{n}(%s{n} %w) {{\nstart:");
            let _ = writeln!(aus, "  call void @oida_schreib_text(ptr {anfang})");
            for (i, (_, feld)) in s.felder.iter().enumerate() {
                if i > 0 {
                    let _ = writeln!(aus, "  call void @oida_schreib_text(ptr {komma})");
                }
                let _ = writeln!(aus, "  %f{i} = extractvalue %s{n} %w, {i}");
                aus.push_str(&self.schreib_befehle(*feld, &format!("%f{i}"), true, &format!("j{i}")));
            }
            let _ = writeln!(aus, "  call void @oida_schreib_text(ptr {ende})\n  ret void\n}}\n");
        }

        for (n, element) in typen.listen.iter().enumerate() {
            if matches!(element, Typ::LeereListe | Typ::Fehler | Typ::Nix) {
                continue; // nur ein Zwischenstand der Typprüfung, kommt im Programm nicht vor
            }
            let t = ir_typ(*element);
            let lesen = "  %lp = getelementptr inbounds %oida.Liste, ptr %liste, i32 0, i32 1\n  \
                         %laenge = load i64, ptr %lp\n  \
                         %dp = getelementptr inbounds %oida.Liste, ptr %liste, i32 0, i32 3\n  \
                         %daten = load ptr, ptr %dp\n";

            // Loslassen: Braucht die Liste niemand mehr, werden erst die Elemente losgelassen.
            let _ = writeln!(aus, "define internal void @oida.lass.l{n}(ptr %liste) {{\nstart:");
            aus.push_str(
                "  %z = load i64, ptr %liste\n  %weniger = sub i64 %z, 1\n  store i64 %weniger, ptr %liste\n  \
                 %keiner = icmp eq i64 %weniger, 0\n  br i1 %keiner, label %frei, label %ende\nfrei:\n",
            );
            if let Some(befehl) = lass_befehl(typen, *element, "%e") {
                aus.push_str(lesen);
                let _ = writeln!(
                    aus,
                    "  br label %kopf\nkopf:\n  %i = phi i64 [ 0, %frei ], [ %i2, %rumpf ]\n  \
                     %weiter = icmp slt i64 %i, %laenge\n  br i1 %weiter, label %rumpf, label %raus\nrumpf:\n  \
                     %p = getelementptr inbounds {t}, ptr %daten, i64 %i\n  %e = load {t}, ptr %p\n  {befehl}\n  \
                     %i2 = add i64 %i, 1\n  br label %kopf\nraus:"
                );
            }
            let _ = writeln!(
                aus,
                "  call void @oida_liste_frei(ptr %liste, i64 {})\n  br label %ende\nende:\n  ret void\n}}\n",
                groesse(*element)
            );

            // Ausgeben: [a, b, c]
            let (auf, komma, zu) = (self.text("["), self.text(", "), self.text("]"));
            let _ = writeln!(aus, "define internal void @oida.schreib.l{n}(ptr %liste) {{\nstart:");
            let _ = writeln!(aus, "  call void @oida_schreib_text(ptr {auf})");
            aus.push_str(lesen);
            let _ = writeln!(
                aus,
                "  br label %kopf\nkopf:\n  %i = phi i64 [ 0, %start ], [ %i2, %element ]\n  \
                 %weiter = icmp slt i64 %i, %laenge\n  br i1 %weiter, label %rumpf, label %ende\nrumpf:\n  \
                 %erstes = icmp eq i64 %i, 0\n  br i1 %erstes, label %element, label %komma\nkomma:\n  \
                 call void @oida_schreib_text(ptr {komma})\n  br label %element\nelement:\n  \
                 %p = getelementptr inbounds {t}, ptr %daten, i64 %i\n  %e = load {t}, ptr %p"
            );
            aus.push_str(&self.schreib_befehle(*element, "%e", true, "j"));
            let _ = writeln!(
                aus,
                "  %i2 = add i64 %i, 1\n  br label %kopf\nende:\n  call void @oida_schreib_text(ptr {zu})\n  ret void\n}}\n"
            );
        }
        aus
    }
}

/// Erzeugt den Code für eine Funktion (oder das Hauptprogramm).
struct FunktionsBau<'m, 't> {
    modul: &'m mut Modul<'t>,
    /// Die `alloca`-Zeilen; sie kommen alle ganz an den Anfang der Funktion.
    plaetze: String,
    code: String,
    naechste_nummer: usize,
    /// Der Block, in den gerade geschrieben wird (für `phi`).
    block: String,
    /// Die verwalteten Variablen (ihr Platz und ihr Typ) je offenem Block, der
    /// innerste hinten. Am Ende eines Blocks und bei GIB ZRUCK werden sie losgelassen.
    verwaltet: Vec<Vec<(String, Typ)>>,
    /// Die Schleifen, in denen wir gerade stecken, die innerste hinten.
    schleifen: Vec<Schleife>,
    /// In einer Methode: die Nummer von SÖWA (sein Platz heißt `%v<nummer>`).
    selbst: Option<usize>,
    /// Was die Funktion zurückgibt (`Nix` auch im Hauptprogramm).
    rueckgabe: Typ,
}

/// Was HAU OB und WEIDA über ihre Schleife wissen müssen.
#[derive(Clone)]
struct Schleife {
    /// Hierhin springt WEIDA.
    weiter: String,
    /// Hierhin springt HAU OB.
    ende: String,
    /// Wie viele Blöcke offen waren, als der Rumpf begonnen hat. Die Werte
    /// aus allen Blöcken darüber werden beim Hinausspringen losgelassen.
    bloecke: usize,
}

/// Der Wert, wenn der Ausdruck eine feste ganze Zahl ist (auch mit Minus davor).
fn feste_zoi(ausdruck: &Ausdruck) -> Option<i64> {
    match &ausdruck.art {
        AusdruckArt::Zoi(n) => Some(*n),
        AusdruckArt::Unaer { op: UnaerOp::Minus, innen } => match innen.art {
            AusdruckArt::Zoi(n) => Some(n.wrapping_neg()),
            _ => None,
        },
        _ => None,
    }
}

/// Ist das eine Variable (oder SÖWA) oder ein Feld (von einem Feld ...) einer
/// Variable mit einer Struktur? Daraus darf geliehen werden: Niemand außer
/// dieser Funktion kommt an ihre Variablen. Das Feld eines Objekts zählt
/// nicht dazu: Das Objekt kann auch jemand anderer in der Hand haben.
fn feld_einer_variable(ausdruck: &Ausdruck) -> bool {
    match &ausdruck.art {
        AusdruckArt::Variable { .. } | AusdruckArt::Selbst { .. } => true,
        AusdruckArt::Feld { objekt, .. } => matches!(objekt.typ, Typ::Struktur(_)) && feld_einer_variable(objekt),
        // Eine Variable, von der feststeht, dass sie nicht NIX ist.
        AusdruckArt::Sicher { innen } => feld_einer_variable(innen),
        _ => false,
    }
}

impl<'m, 't> FunktionsBau<'m, 't> {
    fn neu(modul: &'m mut Modul<'t>) -> FunktionsBau<'m, 't> {
        FunktionsBau {
            modul,
            plaetze: String::new(),
            code: String::new(),
            naechste_nummer: 0,
            block: "start".to_string(),
            verwaltet: vec![Vec::new()],
            schleifen: Vec::new(),
            selbst: None,
            rueckgabe: Typ::Nix,
        }
    }

    fn zeile(&mut self, text: &str) {
        self.code.push_str("  ");
        self.code.push_str(text);
        self.code.push('\n');
    }

    fn temp(&mut self) -> String {
        self.naechste_nummer += 1;
        format!("%t{}", self.naechste_nummer)
    }

    fn marke(&mut self, name: &str) -> String {
        self.naechste_nummer += 1;
        format!("{name}{}", self.naechste_nummer)
    }

    fn beginne_block(&mut self, marke: &str) {
        let _ = writeln!(self.code, "{marke}:");
        self.block = marke.to_string();
    }

    /// Legt den Platz für eine Variable an. Verwaltete Variablen werden
    /// gemerkt, damit ihr Wert am Ende des Blocks losgelassen wird.
    fn platz(&mut self, id: usize, typ: Typ) {
        let name = format!("%v{id}");
        let _ = writeln!(self.plaetze, "  {name} = alloca {}", ir_typ(typ));
        if self.modul.typen.verwaltet(typ) {
            self.verwaltet.last_mut().expect("es gibt immer einen Block").push((name, typ));
        }
    }

    /// Ein Platz am Stack, den es im OIDA-Programm nicht gibt (für Zwischenwerte).
    fn versteckter_platz(&mut self, typ: &str) -> String {
        self.naechste_nummer += 1;
        let name = format!("%h{}", self.naechste_nummer);
        let _ = writeln!(self.plaetze, "  {name} = alloca {typ}");
        name
    }

    // ------------------------------------------------ mitzählen, loslassen

    /// Der Wert wird ab jetzt an einer weiteren Stelle gebraucht.
    fn behalt(&mut self, wert: &str, typ: Typ) {
        if let Some(befehl) = behalt_befehl(self.modul.typen, typ, wert) {
            self.zeile(&befehl);
        }
    }

    /// Der Wert wird hier nicht mehr gebraucht.
    fn lass(&mut self, wert: &str, typ: Typ) {
        if let Some(befehl) = lass_befehl(self.modul.typen, typ, wert) {
            self.zeile(&befehl);
        }
    }

    /// Lässt den Wert los, wenn er uns gehört (siehe [`Self::geliehen`]).
    fn lass_wenn_eigen(&mut self, wert: &str, typ: Typ, eigen: bool) {
        if eigen {
            self.lass(wert, typ);
        }
    }

    /// Lässt die Werte der Variablen los, die in den innersten `bloecke`
    /// Blöcken angesagt wurden.
    fn lass_variablen(&mut self, bloecke: usize) {
        let anfang = self.verwaltet.len() - bloecke;
        let variablen: Vec<(String, Typ)> = self.verwaltet[anfang..].iter().flatten().cloned().collect();
        // In umgekehrter Reihenfolge, wie beim Aufräumen üblich.
        for (platz, typ) in variablen.into_iter().rev() {
            let t = self.temp();
            self.zeile(&format!("{t} = load {}, ptr {platz}", ir_typ(typ)));
            self.lass(&t, typ);
        }
    }

    // ------------------------------------------------------- Anweisungen

    /// Die Anweisungen eines eigenen Blocks (WENN, SOLONG): Variablen, die
    /// darin angesagt werden, gelten nur bis zu seinem Ende.
    fn block(&mut self, anweisungen: &[Anweisung]) {
        self.verwaltet.push(Vec::new());
        for a in anweisungen {
            self.anweisung(a);
        }
        self.lass_variablen(1);
        self.verwaltet.pop();
    }

    fn anweisung(&mut self, anweisung: &Anweisung) {
        let zeile = anweisung.pos.zeile;
        match &anweisung.art {
            AnweisungArt::Deklaration { wert, id, .. } => {
                let w = self.ausdruck(wert);
                self.platz(*id, wert.typ);
                self.zeile(&format!("store {} {w}, ptr %v{id}{}", ir_typ(wert.typ), wert_hinweis(wert.typ)));
            }
            AnweisungArt::Zuweisung { ziel, op: None, wert } => {
                if let AusdruckArt::Stelle { liste, index } = &ziel.art
                    && matches!(liste.typ, Typ::Verzeichnis(_))
                {
                    self.wb_schreiben(liste, index, wert);
                    return;
                }
                // Erst den Wert ausrechnen, dann erst das Ziel suchen: Beim
                // Ausrechnen könnte sich eine Liste ändern, in der das Ziel liegt.
                let w = self.ausdruck(wert);
                let (zeiger, wurzel) = self.ort(ziel);
                let (t, hinweis) = (ir_typ(ziel.typ), wert_hinweis(ziel.typ));
                if self.modul.typen.verwaltet(ziel.typ) {
                    // Der alte Wert wird nicht mehr gebraucht. Erst nach dem
                    // Speichern loslassen, falls es derselbe ist (s = s).
                    let alt = self.temp();
                    self.zeile(&format!("{alt} = load {t}, ptr {zeiger}{hinweis}"));
                    self.zeile(&format!("store {t} {w}, ptr {zeiger}{hinweis}"));
                    self.lass(&alt, ziel.typ);
                } else {
                    self.zeile(&format!("store {t} {w}, ptr {zeiger}{hinweis}"));
                }
                self.lass_wurzel(wurzel);
            }
            AnweisungArt::Zuweisung { ziel, op: Some(op), wert } => {
                // ziel += wert: Das Ziel wird nur einmal gesucht.
                let (r, r_eigen) = self.geliehen(wert);
                let (zeiger, wurzel) = self.ort(ziel);
                let (t, hinweis) = (ir_typ(ziel.typ), wert_hinweis(ziel.typ));
                let alt = self.temp();
                self.zeile(&format!("{alt} = load {t}, ptr {zeiger}{hinweis}"));
                if ziel.typ == Typ::Kettn {
                    let neu = self.temp();
                    self.zeile(&format!("{neu} = call ptr @oida_text_verbinden(ptr {alt}, ptr {r})"));
                    self.zeile(&format!("store ptr {neu}, ptr {zeiger}{hinweis}"));
                    self.lass(&alt, Typ::Kettn);
                } else {
                    let neu = self.binaer(*op, ziel.typ, &alt, &r, zeile);
                    self.zeile(&format!("store {t} {neu}, ptr {zeiger}{hinweis}"));
                }
                self.lass_wenn_eigen(&r, wert.typ, r_eigen);
                self.lass_wurzel(wurzel);
            }
            AnweisungArt::Wenn { bedingung, dann, sunst } => {
                let b = self.ausdruck(bedingung);
                let m_dann = self.marke("dann");
                let m_ende = self.marke("wenn_ende");
                let m_sunst = if sunst.is_empty() { m_ende.clone() } else { self.marke("sunst") };
                self.zeile(&format!("br i1 {b}, label %{m_dann}, label %{m_sunst}"));
                self.beginne_block(&m_dann);
                self.block(dann);
                self.zeile(&format!("br label %{m_ende}"));
                if !sunst.is_empty() {
                    self.beginne_block(&m_sunst);
                    self.block(sunst);
                    self.zeile(&format!("br label %{m_ende}"));
                }
                self.beginne_block(&m_ende);
            }
            AnweisungArt::Solong { bedingung, rumpf } => {
                let m_kopf = self.marke("solong");
                let m_rumpf = self.marke("tua");
                let m_ende = self.marke("solong_ende");
                self.zeile(&format!("br label %{m_kopf}"));
                self.beginne_block(&m_kopf);
                let b = self.ausdruck(bedingung);
                self.zeile(&format!("br i1 {b}, label %{m_rumpf}, label %{m_ende}"));
                self.beginne_block(&m_rumpf);
                self.schleifen.push(Schleife {
                    weiter: m_kopf.clone(),
                    ende: m_ende.clone(),
                    bloecke: self.verwaltet.len(),
                });
                self.block(rumpf);
                self.schleifen.pop();
                self.zeile(&format!("br label %{m_kopf}"));
                self.beginne_block(&m_ende);
            }
            AnweisungArt::Fia { id, von, bis, schritt, rumpf, .. } => {
                // Anfang, Ende und Schritt werden einmal vor der Schleife ausgerechnet.
                let anfang = self.ausdruck(von);
                self.platz(*id, Typ::Zoi);
                self.zeile(&format!("store i64 {anfang}, ptr %v{id}"));
                let ende = self.ausdruck(bis);
                let fest = match schritt {
                    None => Some(1),
                    Some(schritt) => feste_zoi(schritt).filter(|n| *n != 0),
                };
                let weite = match schritt {
                    None => "1".to_string(),
                    Some(schritt) => self.ausdruck(schritt),
                };
                if fest.is_none() {
                    // Der Schritt steht erst beim Laufen fest: 0 wäre eine Schleife ohne Ende.
                    let null = self.temp();
                    let m_null = self.marke("schritt_null");
                    let m_ok = self.marke("schritt_ok");
                    self.zeile(&format!("{null} = icmp eq i64 {weite}, 0"));
                    self.zeile(&format!("br i1 {null}, label %{m_null}, label %{m_ok}"));
                    self.beginne_block(&m_null);
                    self.zeile(&format!("call void @oida_fehler_schritt_null(i64 {zeile})"));
                    self.zeile("unreachable");
                    self.beginne_block(&m_ok);
                }

                let m_kopf = self.marke("fia");
                let m_rumpf = self.marke("tua");
                let m_weiter = self.marke("fia_weida");
                let m_ende = self.marke("fia_ende");
                self.zeile(&format!("br label %{m_kopf}"));
                self.beginne_block(&m_kopf);
                let i = self.temp();
                self.zeile(&format!("{i} = load i64, ptr %v{id}"));
                let weiter = self.temp();
                match fest {
                    Some(n) if n > 0 => self.zeile(&format!("{weiter} = icmp sle i64 {i}, {ende}")),
                    Some(_) => self.zeile(&format!("{weiter} = icmp sge i64 {i}, {ende}")),
                    None => {
                        // Aufwärts zählen heißt "bis höchstens", abwärts "bis mindestens".
                        let (aufwaerts, hoechstens, mindestens) = (self.temp(), self.temp(), self.temp());
                        self.zeile(&format!("{aufwaerts} = icmp sgt i64 {weite}, 0"));
                        self.zeile(&format!("{hoechstens} = icmp sle i64 {i}, {ende}"));
                        self.zeile(&format!("{mindestens} = icmp sge i64 {i}, {ende}"));
                        self.zeile(&format!("{weiter} = select i1 {aufwaerts}, i1 {hoechstens}, i1 {mindestens}"));
                    }
                }
                self.zeile(&format!("br i1 {weiter}, label %{m_rumpf}, label %{m_ende}"));

                self.beginne_block(&m_rumpf);
                self.schleifen.push(Schleife {
                    weiter: m_weiter.clone(),
                    ende: m_ende.clone(),
                    bloecke: self.verwaltet.len(),
                });
                self.block(rumpf);
                self.schleifen.pop();
                self.zeile(&format!("br label %{m_weiter}"));

                // Weiterzählen. Läuft die Zahl dabei über, ist die Schleife zu Ende.
                self.beginne_block(&m_weiter);
                let (alt, paar, neu, ueber) = (self.temp(), self.temp(), self.temp(), self.temp());
                self.zeile(&format!("{alt} = load i64, ptr %v{id}"));
                self.zeile(&format!("{paar} = call {{ i64, i1 }} @llvm.sadd.with.overflow.i64(i64 {alt}, i64 {weite})"));
                self.zeile(&format!("{neu} = extractvalue {{ i64, i1 }} {paar}, 0"));
                self.zeile(&format!("{ueber} = extractvalue {{ i64, i1 }} {paar}, 1"));
                self.zeile(&format!("store i64 {neu}, ptr %v{id}"));
                self.zeile(&format!("br i1 {ueber}, label %{m_ende}, label %{m_kopf}"));
                self.beginne_block(&m_ende);
            }
            AnweisungArt::FiaJede { id, liste, rumpf, .. } => self.fia_jede(*id, liste, rumpf),
            AnweisungArt::HauOb | AnweisungArt::Weida => {
                let schleife = self.schleifen.last().expect("die Typprüfung lässt das nur in Schleifen zu").clone();
                // Die Werte aus allen Blöcken, die dabei verlassen werden.
                self.lass_variablen(self.verwaltet.len() - schleife.bloecke);
                let ziel = if matches!(anweisung.art, AnweisungArt::HauOb) { schleife.ende } else { schleife.weiter };
                self.zeile(&format!("br label %{ziel}"));
                // Was danach noch kommt, wird nie erreicht, braucht aber einen eigenen Block.
                let m = self.marke("nach_sprung");
                self.beginne_block(&m);
            }
            AnweisungArt::GibZruck { wert } => {
                // Erst den Wert ausrechnen (er gehört dann dem Aufrufer),
                // dann alle verwalteten Variablen der Funktion loslassen.
                let w = wert.as_ref().map(|wert| (self.ausdruck(wert), wert.typ));
                self.lass_variablen(self.verwaltet.len());
                match w {
                    Some((w, typ)) => self.zeile(&format!("ret {} {w}", ir_typ(typ))),
                    // NIX ODA FEHLA: kein Fehler, also hat es geklappt.
                    None if matches!(self.rueckgabe, Typ::Ergebnis(_)) => {
                        self.zeile(&format!("ret {} zeroinitializer", ir_typ(self.rueckgabe)));
                    }
                    None => self.zeile("ret void"),
                }
                // Was danach noch kommt, wird nie erreicht, braucht aber einen eigenen Block.
                let m = self.marke("nach_zruck");
                self.beginne_block(&m);
            }
            AnweisungArt::Schreib { argumente } => {
                for (i, argument) in argumente.iter().enumerate() {
                    let (w, eigen) = self.geliehen(argument);
                    if i > 0 {
                        self.zeile("call void @oida_schreib_leer()");
                    }
                    let name = self.marke("j");
                    let befehle = self.modul.schreib_befehle(argument.typ, &w, false, &name);
                    self.code.push_str(&befehle);
                    self.lass_wenn_eigen(&w, argument.typ, eigen);
                }
                self.zeile("call void @oida_schreib_zeilenende()");
            }
            AnweisungArt::Ausdruck(ausdruck) => {
                // Das Ergebnis will niemand haben.
                let w = self.ausdruck(ausdruck);
                self.lass(&w, ausdruck.typ);
            }
            AnweisungArt::Probier { name, id, wert, dann, fehler_name, fehler_id, sunst } => {
                let wert_typ = self.modul.typen.wert_von(wert.typ).expect("die Typprüfung verlangt ... ODA FEHLA");
                let e = self.ausdruck(wert);
                let et = ir_typ(wert.typ);
                let (f, gut) = (self.temp(), self.temp());
                let m_gut = self.marke("probier_guat");
                let m_schief = self.marke("probier_schief");
                let m_ende = self.marke("probier_ende");
                self.zeile(&format!("{f} = extractvalue {et} {e}, 0"));
                self.zeile(&format!("{gut} = icmp eq ptr {f}, null"));
                self.zeile(&format!("br i1 {gut}, label %{m_gut}, label %{m_schief}"));

                // Geklappt: Der Wert gehört der neuen Variable (oder niemandem).
                self.beginne_block(&m_gut);
                self.verwaltet.push(Vec::new());
                if wert_typ != Typ::Nix {
                    let w = self.temp();
                    self.zeile(&format!("{w} = extractvalue {et} {e}, 1"));
                    if name.is_some() {
                        self.platz(*id, wert_typ);
                        self.zeile(&format!("store {} {w}, ptr %v{id}{}", ir_typ(wert_typ), wert_hinweis(wert_typ)));
                    } else {
                        self.lass(&w, wert_typ);
                    }
                }
                for a in dann {
                    self.anweisung(a);
                }
                self.lass_variablen(1);
                self.verwaltet.pop();
                self.zeile(&format!("br label %{m_ende}"));

                // Schiefgegangen: Der Fehler gehört der neuen Variable (oder niemandem).
                self.beginne_block(&m_schief);
                self.verwaltet.push(Vec::new());
                if fehler_name.is_some() {
                    self.platz(*fehler_id, Typ::Fehla);
                    self.zeile(&format!("store ptr {f}, ptr %v{fehler_id}, !tbaa !4"));
                } else {
                    self.lass(&f, Typ::Fehla);
                }
                for a in sunst {
                    self.anweisung(a);
                }
                self.lass_variablen(1);
                self.verwaltet.pop();
                self.zeile(&format!("br label %{m_ende}"));
                self.beginne_block(&m_ende);
            }
        }
    }

    /// `FIA JEDE x IN liste`: geht die Liste der Reihe nach durch. `x` ist in
    /// jedem Durchlauf eine Kopie des Elements und gilt nur in diesem Durchlauf.
    fn fia_jede(&mut self, id: usize, liste: &Ausdruck, rumpf: &[Anweisung]) {
        let typen = self.modul.typen;
        // Bei einem Verzeichnis geht es über die Schlüssel: Die Laufzeit gibt
        // sie als neue Liste her, die danach wie jede Liste durchgegangen wird.
        let (l, element, listen_typ) = match typen.verzeichnis(liste.typ) {
            Some((k, _)) => {
                let (v, v_eigen) = self.geliehen(liste);
                let l = self.temp();
                self.zeile(&format!("{l} = call ptr @oida_wb_schluessel(ptr {v})"));
                self.lass_wenn_eigen(&v, liste.typ, v_eigen);
                let nummer = typen.listen.iter().position(|t| *t == k).expect("die Typprüfung legt den Listentyp an");
                (l, k, Typ::Liste(nummer as u32))
            }
            None => {
                let element = typen.element(liste.typ).expect("die Typprüfung verlangt eine Liste");
                (self.ausdruck(liste), element, liste.typ)
            }
        };
        let t = ir_typ(element);

        // Die Liste gehört für die Dauer der Schleife uns, auch wenn die
        // Variable inzwischen eine andere Liste bekommt. Sie liegt in einem
        // versteckten Platz, damit HAU OB und GIB ZRUCK sie mit loslassen.
        self.verwaltet.push(Vec::new());
        let l_platz = self.versteckter_platz("ptr");
        self.zeile(&format!("store ptr {l}, ptr {l_platz}"));
        self.verwaltet.last_mut().expect("gerade angelegt").push((l_platz, listen_typ));
        let i_platz = self.versteckter_platz("i64");
        self.zeile(&format!("store i64 0, ptr {i_platz}"));

        let m_kopf = self.marke("jede");
        let m_rumpf = self.marke("tua");
        let m_weiter = self.marke("jede_weida");
        let m_ende = self.marke("jede_ende");
        self.zeile(&format!("br label %{m_kopf}"));

        // Die Länge wird jedes Mal neu gelesen: Die Liste darf in der Schleife kürzer werden.
        self.beginne_block(&m_kopf);
        let (i, lp, laenge, weiter) = (self.temp(), self.temp(), self.temp(), self.temp());
        self.zeile(&format!("{i} = load i64, ptr {i_platz}"));
        self.zeile(&format!("{lp} = getelementptr inbounds %oida.Liste, ptr {l}, i32 0, i32 1"));
        self.zeile(&format!("{laenge} = load i64, ptr {lp}{KOPF_HINWEIS}"));
        self.zeile(&format!("{weiter} = icmp slt i64 {i}, {laenge}"));
        self.zeile(&format!("br i1 {weiter}, label %{m_rumpf}, label %{m_ende}"));

        self.beginne_block(&m_rumpf);
        self.schleifen.push(Schleife { weiter: m_weiter.clone(), ende: m_ende.clone(), bloecke: self.verwaltet.len() });
        self.verwaltet.push(Vec::new());
        let (dp, daten, p, e) = (self.temp(), self.temp(), self.temp(), self.temp());
        self.zeile(&format!("{dp} = getelementptr inbounds %oida.Liste, ptr {l}, i32 0, i32 3"));
        self.zeile(&format!("{daten} = load ptr, ptr {dp}{KOPF_HINWEIS}"));
        self.zeile(&format!("{p} = getelementptr inbounds {t}, ptr {daten}, i64 {i}"));
        self.zeile(&format!("{e} = load {t}, ptr {p}{}", wert_hinweis(element)));
        self.behalt(&e, element);
        self.platz(id, element);
        self.zeile(&format!("store {t} {e}, ptr %v{id}"));
        for a in rumpf {
            self.anweisung(a);
        }
        self.lass_variablen(1);
        self.verwaltet.pop();
        self.schleifen.pop();
        self.zeile(&format!("br label %{m_weiter}"));

        self.beginne_block(&m_weiter);
        let (alt, neu) = (self.temp(), self.temp());
        self.zeile(&format!("{alt} = load i64, ptr {i_platz}"));
        self.zeile(&format!("{neu} = add i64 {alt}, 1"));
        self.zeile(&format!("store i64 {neu}, ptr {i_platz}"));
        self.zeile(&format!("br label %{m_kopf}"));

        self.beginne_block(&m_ende);
        self.lass_variablen(1);
        self.verwaltet.pop();
    }

    // -------------------------------------------- Felder und Stellen finden

    /// Wo eine Variable, ein Feld (`p.x`) oder ein Element (`liste[i]`) im Speicher liegt.
    ///
    /// Beginnt der Ausdruck nicht bei einer Variable (etwa `gib_liste()[0]`),
    /// wird der Anfang ausgerechnet und zwischengespeichert; er kommt dann
    /// als zweiter Wert zurück und muss nach dem Lesen oder Schreiben mit
    /// [`Self::lass_wurzel`] losgelassen werden.
    fn ort(&mut self, ausdruck: &Ausdruck) -> (String, Option<(String, Typ)>) {
        // Den Weg von außen nach innen aufrollen: wurzel . feld [stelle] . feld ...
        let mut schritte: Vec<&Ausdruck> = Vec::new();
        let mut wurzel = ausdruck;
        loop {
            match &wurzel.art {
                AusdruckArt::Feld { objekt, .. } => {
                    schritte.push(wurzel);
                    wurzel = objekt;
                }
                AusdruckArt::Stelle { liste, .. } => {
                    schritte.push(wurzel);
                    wurzel = liste;
                }
                AusdruckArt::Sicher { innen } => {
                    schritte.push(wurzel);
                    wurzel = innen;
                }
                _ => break,
            }
        }
        schritte.reverse();

        let (mut zeiger, eigen) = match &wurzel.art {
            AusdruckArt::Variable { id, .. } | AusdruckArt::Selbst { id } => (format!("%v{id}"), None),
            _ => {
                let w = self.ausdruck(wurzel);
                let t = ir_typ(wurzel.typ);
                let platz = self.versteckter_platz(&t);
                self.zeile(&format!("store {t} {w}, ptr {platz}"));
                (platz, Some((w, wurzel.typ)))
            }
        };

        // Erst alle Stellen ausrechnen: Dabei können Funktionen laufen, die
        // Listen ändern. Danach geht es ohne Unterbrechung zum Ziel.
        let stellen: Vec<Option<String>> = schritte
            .iter()
            .map(|schritt| match &schritt.art {
                AusdruckArt::Stelle { index, .. } => Some(self.ausdruck(index)),
                _ => None,
            })
            .collect();
        for (schritt, stelle) in schritte.iter().zip(stellen) {
            match &schritt.art {
                // Die Meldung eines Fehlers: Der Fehler ist selbst dieser Text.
                AusdruckArt::Feld { objekt, .. } if objekt.typ == Typ::Fehla => {}
                // Sicher nicht NIX: Steht ein Merker davor, liegt der Wert dahinter.
                AusdruckArt::Sicher { innen } => {
                    if let Typ::Vielleicht { zeiger: false, .. } = innen.typ {
                        let t = self.temp();
                        self.zeile(&format!(
                            "{t} = getelementptr inbounds {}, ptr {zeiger}, i32 0, i32 1",
                            ir_typ(innen.typ)
                        ));
                        zeiger = t;
                    }
                }
                AusdruckArt::Feld { objekt, nummer, .. } => {
                    let t = self.temp();
                    if let Typ::Klasse(k) = objekt.typ {
                        // Am Platz steht nur der Zeiger auf das Objekt; die Felder
                        // liegen dort hinter dem Zähler und den Angaben zur Klasse.
                        let o = self.temp();
                        self.zeile(&format!("{o} = load ptr, ptr {zeiger}, !tbaa !4"));
                        self.zeile(&format!("{t} = getelementptr inbounds %k{k}, ptr {o}, i32 0, i32 {}", nummer + 2));
                    } else {
                        self.zeile(&format!(
                            "{t} = getelementptr inbounds {}, ptr {zeiger}, i32 0, i32 {nummer}",
                            ir_typ(objekt.typ)
                        ));
                    }
                    zeiger = t;
                }
                AusdruckArt::Stelle { .. } => {
                    let stelle = stelle.expect("oben ausgerechnet");
                    let (l, daten, t) = (self.temp(), self.temp(), self.temp());
                    self.zeile(&format!("{l} = load ptr, ptr {zeiger}, !tbaa !4"));
                    self.zeile(&format!(
                        "{daten} = call ptr @oida.stelle(ptr {l}, i64 {stelle}, i64 {})",
                        schritt.pos.zeile
                    ));
                    self.zeile(&format!(
                        "{t} = getelementptr inbounds {}, ptr {daten}, i64 {stelle}",
                        ir_typ(schritt.typ)
                    ));
                    zeiger = t;
                }
                _ => unreachable!(),
            }
        }
        (zeiger, eigen)
    }

    /// Lässt den Anfang los, den [`Self::ort`] zwischengespeichert hat.
    fn lass_wurzel(&mut self, wurzel: Option<(String, Typ)>) {
        if let Some((wert, typ)) = wurzel {
            self.lass(&wert, typ);
        }
    }

    // --------------------------------------------------------- Ausdrücke

    /// Wie [`Self::ausdruck`], aber für Stellen, die den Wert nur lesen.
    /// Gibt dazu zurück, ob der Wert uns gehört und nach dem Lesen
    /// losgelassen werden muss. Eine Variable, ein Feld einer Variable und
    /// ein fester Text werden nur geliehen: Solange gelesen wird, kann sie
    /// niemand ändern.
    fn geliehen(&mut self, ausdruck: &Ausdruck) -> (String, bool) {
        if !self.modul.typen.verwaltet(ausdruck.typ) {
            return (self.ausdruck(ausdruck), false);
        }
        match &ausdruck.art {
            AusdruckArt::Text(inhalt) => (self.modul.text(inhalt), false),
            AusdruckArt::Variable { .. }
            | AusdruckArt::Feld { .. }
            | AusdruckArt::Selbst { .. }
            | AusdruckArt::Sicher { .. }
                if feld_einer_variable(ausdruck) =>
            {
                let (zeiger, _) = self.ort(ausdruck);
                let t = self.temp();
                self.zeile(&format!(
                    "{t} = load {}, ptr {zeiger}{}",
                    ir_typ(ausdruck.typ),
                    wert_hinweis(ausdruck.typ)
                ));
                (t, false)
            }
            _ => (self.ausdruck(ausdruck), true),
        }
    }

    /// Erzeugt den Code für einen Ausdruck und gibt zurück, wo der Wert steht
    /// (ein Register wie `%t3` oder gleich eine Konstante).
    /// Ein verwalteter Wert gehört danach dem Empfänger.
    fn ausdruck(&mut self, ausdruck: &Ausdruck) -> String {
        match &ausdruck.art {
            AusdruckArt::Zoi(wert) => wert.to_string(),
            // Als Bitmuster, damit die Zahl genau so ankommt.
            AusdruckArt::Kommazoi(wert) => format!("0x{:016X}", wert.to_bits()),
            AusdruckArt::Jona(wert) => wert.to_string(),
            AusdruckArt::Text(inhalt) => self.modul.text(inhalt),
            AusdruckArt::Stelle { liste, index } if matches!(liste.typ, Typ::Verzeichnis(_)) => {
                self.wb_lesen(liste, index, ausdruck.typ)
            }
            AusdruckArt::Variable { .. }
            | AusdruckArt::Feld { .. }
            | AusdruckArt::Stelle { .. }
            | AusdruckArt::Selbst { .. }
            | AusdruckArt::Sicher { .. } => {
                let (zeiger, wurzel) = self.ort(ausdruck);
                let t = self.temp();
                self.zeile(&format!(
                    "{t} = load {}, ptr {zeiger}{}",
                    ir_typ(ausdruck.typ),
                    wert_hinweis(ausdruck.typ)
                ));
                // Jetzt braucht ihn außer der Variable (oder der Liste) noch jemand.
                self.behalt(&t, ausdruck.typ);
                self.lass_wurzel(wurzel);
                t
            }
            AusdruckArt::Unaer { op, innen } => {
                let w = self.ausdruck(innen);
                let t = self.temp();
                match (op, innen.typ) {
                    (UnaerOp::Minus, Typ::Zoi) => self.zeile(&format!("{t} = sub i64 0, {w}")),
                    (UnaerOp::Minus, _) => self.zeile(&format!("{t} = fneg double {w}")),
                    (UnaerOp::Ned, _) => self.zeile(&format!("{t} = xor i1 {w}, true")),
                }
                t
            }
            AusdruckArt::Binaer { op: op @ (BinaerOp::Und | BinaerOp::Oda), links, rechts } => {
                self.und_oda(*op == BinaerOp::Und, links, rechts)
            }
            AusdruckArt::Binaer { op: BinaerOp::Plus, .. } if ausdruck.typ == Typ::Kettn => {
                self.verbinden(ausdruck)
            }
            AusdruckArt::Binaer { op, links, rechts } => {
                // Beim Vergleichen werden Texte nur gelesen.
                let (l, l_eigen) = self.geliehen(links);
                let (r, r_eigen) = self.geliehen(rechts);
                let t = self.binaer(*op, links.typ, &l, &r, ausdruck.pos.zeile);
                self.lass_wenn_eigen(&l, links.typ, l_eigen);
                self.lass_wenn_eigen(&r, rechts.typ, r_eigen);
                t
            }
            AusdruckArt::Aufruf { name, argumente } => {
                // Die Argumente gehören danach der aufgerufenen Funktion.
                let werte: Vec<String> = argumente
                    .iter()
                    .map(|a| {
                        let w = self.ausdruck(a);
                        format!("{} {w}", ir_typ(a.typ))
                    })
                    .collect();
                let aufruf = format!("call {} {}({})", ir_typ(ausdruck.typ), funktions_name(name), werte.join(", "));
                if ausdruck.typ == Typ::Nix {
                    self.zeile(&aufruf);
                    String::new()
                } else {
                    let t = self.temp();
                    self.zeile(&format!("{t} = {aufruf}"));
                    t
                }
            }
            AusdruckArt::Eingebaut { funktion, argumente } => {
                self.eingebaut(*funktion, argumente, ausdruck.typ, ausdruck.pos.zeile)
            }
            AusdruckArt::Umwandlung { ziel, innen } => {
                // Derselbe Wert unter einem anderen Typ: Im Speicher ändert sich nichts.
                let gleich = innen.typ == *ziel
                    || matches!(ziel, Typ::Klasse(_) | Typ::Vielleicht { zeiger: true, .. })
                    || matches!((innen.typ, *ziel), (Typ::Kettn, Typ::Fehla) | (Typ::Fehla, Typ::Kettn));
                if gleich {
                    return self.ausdruck(innen);
                }
                let (it, zt) = (ir_typ(innen.typ), ir_typ(*ziel));
                match *ziel {
                    // Ein Wert, wo etwas fehlen darf: den Merker "ist da" dazu.
                    Typ::Vielleicht { .. } => {
                        let w = self.ausdruck(innen);
                        let (a, b) = (self.temp(), self.temp());
                        self.zeile(&format!("{a} = insertvalue {zt} zeroinitializer, i1 true, 0"));
                        self.zeile(&format!("{b} = insertvalue {zt} {a}, {it} {w}, 1"));
                        return b;
                    }
                    // Ein Wert oder ein Fehler, wo beides sein darf.
                    Typ::Ergebnis(_) => {
                        let w = self.ausdruck(innen);
                        let t = self.temp();
                        let stelle = if innen.typ == Typ::Fehla { 0 } else { 1 };
                        self.zeile(&format!("{t} = insertvalue {zt} zeroinitializer, {it} {w}, {stelle}"));
                        return t;
                    }
                    _ => {}
                }
                // Aus einem Text wird beim Umwandeln nur gelesen.
                let (w, eigen) = self.geliehen(innen);
                let t = self.umwandlung(innen.typ, *ziel, &w, ausdruck.pos.zeile);
                self.lass_wenn_eigen(&w, innen.typ, eigen);
                t
            }
            AusdruckArt::Bau { argumente, .. } => {
                // Die Felder der Reihe nach in den Verbund setzen. Sie gehören danach dem neuen Wert.
                let t = ir_typ(ausdruck.typ);
                let mut verbund = if argumente.is_empty() { "zeroinitializer".to_string() } else { "poison".to_string() };
                for (i, argument) in argumente.iter().enumerate() {
                    let w = self.ausdruck(argument);
                    let naechster = self.temp();
                    self.zeile(&format!(
                        "{naechster} = insertvalue {t} {verbund}, {} {w}, {i}",
                        ir_typ(argument.typ)
                    ));
                    verbund = naechster;
                }
                verbund
            }
            AusdruckArt::ListenWert { elemente } => {
                let element = self.modul.typen.element(ausdruck.typ).expect("die Typprüfung kennt den Typ der Liste");
                let (t, bytes) = (ir_typ(element), groesse(element));
                let l = self.temp();
                self.zeile(&format!("{l} = call ptr @oida_liste_neu(i64 {bytes}, i64 {})", elemente.len()));
                for e in elemente {
                    let w = self.ausdruck(e);
                    let p = self.temp();
                    self.zeile(&format!("{p} = call ptr @oida_liste_neuer_platz(ptr {l}, i64 {bytes})"));
                    self.zeile(&format!("store {t} {w}, ptr {p}{}", wert_hinweis(element)));
                }
                l
            }
            AusdruckArt::Neu { klasse, argumente } => {
                let (typen, klassen) = (self.modul.typen, self.modul.klassen);
                let o = self.temp();
                self.zeile(&format!("{o} = call ptr @oida_objekt_neu(i64 {})", objekt_groesse(*klasse)));
                let kp = self.temp();
                self.zeile(&format!("{kp} = getelementptr inbounds %k{klasse}, ptr {o}, i32 0, i32 1"));
                self.zeile(&format!("store ptr @oida.klasse.{klasse}, ptr {kp}"));
                // Die Felder der Reihe nach: entweder der Wert, der bei der
                // Klasse dabeisteht, oder der nächste angegebene.
                let mut angegeben = argumente.iter();
                for (i, feld) in typen.klassen[*klasse as usize].felder.iter().enumerate() {
                    let wert = if feld.hat_standard {
                        klassen[feld.klasse as usize].felder[feld.stelle]
                            .standard
                            .as_ref()
                            .expect("die Typprüfung hat sich gemerkt, dass ein Wert dabeisteht")
                    } else {
                        angegeben.next().expect("die Typprüfung zählt die Werte nach")
                    };
                    let w = self.ausdruck(wert);
                    let p = self.temp();
                    self.zeile(&format!("{p} = getelementptr inbounds %k{klasse}, ptr {o}, i32 0, i32 {}", i + 2));
                    self.zeile(&format!("store {} {w}, ptr {p}{}", ir_typ(feld.typ), wert_hinweis(feld.typ)));
                }
                o
            }
            AusdruckArt::Methode { objekt, name, argumente, nummer } => {
                let typen = self.modul.typen;
                let Typ::Klasse(k) = objekt.typ else { unreachable!("die Typprüfung verlangt ein Objekt") };
                let methode = &typen.klassen[k as usize].methoden[*nummer];
                // Das Objekt wird der Methode nur geliehen.
                let (o, o_eigen) = self.geliehen(objekt);
                let mut werte = vec![format!("ptr {o}")];
                for a in argumente {
                    let w = self.ausdruck(a);
                    werte.push(format!("{} {w}", ir_typ(a.typ)));
                }
                let ziel = match methode.platz {
                    // Welche Fassung läuft, hängt davon ab, was für ein Objekt es wirklich ist.
                    Some(platz) if weiter_unten_ersetzt(typen, k, name) => {
                        let (kp, klasse, mp, m) = (self.temp(), self.temp(), self.temp(), self.temp());
                        self.zeile(&format!("{kp} = getelementptr inbounds {{ i64, ptr }}, ptr {o}, i32 0, i32 1"));
                        self.zeile(&format!("{klasse} = load ptr, ptr {kp}"));
                        self.zeile(&format!(
                            "{mp} = getelementptr inbounds {}, ptr {klasse}, i32 0, i32 2, i64 {platz}",
                            klassen_angaben(typen.klassen[k as usize].plaetze)
                        ));
                        self.zeile(&format!("{m} = load ptr, ptr {mp}"));
                        m
                    }
                    _ => methoden_name(&typen.klassen[methode.klasse as usize].name, name),
                };
                let aufruf = format!("call {} {ziel}({})", ir_typ(ausdruck.typ), werte.join(", "));
                let t = if ausdruck.typ == Typ::Nix {
                    self.zeile(&aufruf);
                    String::new()
                } else {
                    let t = self.temp();
                    self.zeile(&format!("{t} = {aufruf}"));
                    t
                };
                self.lass_wenn_eigen(&o, objekt.typ, o_eigen);
                t
            }
            AusdruckArt::Lambda { funktion, fang, nummer, .. } => {
                let n = *nummer;
                if self.modul.lambdas_gebaut.insert(n) {
                    // Die Hülle: wie ein Objekt, dann der Code, dann die mitgenommenen Werte.
                    let typen = self.modul.typen;
                    let felder: String = fang.iter().map(|f| format!(", {}", ir_typ(f.typ))).collect();
                    let mut text = format!("%h{n} = type {{ i64, ptr, ptr{felder} }}\n\n");
                    let _ = writeln!(text, "define internal void @oida.frei.h{n}(ptr %huelle) {{\nstart:");
                    for (i, f) in fang.iter().enumerate() {
                        if let Some(befehl) = lass_befehl(typen, f.typ, &format!("%f{i}")) {
                            let _ = writeln!(
                                text,
                                "  %p{i} = getelementptr inbounds %h{n}, ptr %huelle, i32 0, i32 {}\n  \
                                 %f{i} = load {}, ptr %p{i}\n  {befehl}",
                                i + 3,
                                ir_typ(f.typ)
                            );
                        }
                    }
                    let _ = writeln!(
                        text,
                        "  call void @oida_objekt_frei(ptr %huelle, i64 {})\n  ret void\n}}\n\n\
                         @oida.huelle.{n} = internal constant {{ ptr, ptr }} {{ ptr @oida.frei.h{n}, ptr @oida.schreib.funktion }}\n",
                        huellen_groesse(n)
                    );
                    let code = baue_funktion(&mut *self.modul, funktion, FunktionsArt::Lambda { nummer: n, fang });
                    self.modul.zusatz.push_str(&text);
                    self.modul.zusatz.push_str(&code);
                }
                let h = self.temp();
                self.zeile(&format!("{h} = call ptr @oida_objekt_neu(i64 {})", huellen_groesse(n)));
                let (ap, cp) = (self.temp(), self.temp());
                self.zeile(&format!("{ap} = getelementptr inbounds %h{n}, ptr {h}, i32 0, i32 1"));
                self.zeile(&format!("store ptr @oida.huelle.{n}, ptr {ap}"));
                self.zeile(&format!("{cp} = getelementptr inbounds %h{n}, ptr {h}, i32 0, i32 2"));
                self.zeile(&format!("store ptr @oida.l.{n}, ptr {cp}"));
                // Was die Funktion von außen braucht, wird jetzt kopiert und gehört dann der Hülle.
                for (i, f) in fang.iter().enumerate() {
                    let w = self.ausdruck(&f.wert);
                    let p = self.temp();
                    self.zeile(&format!("{p} = getelementptr inbounds %h{n}, ptr {h}, i32 0, i32 {}", i + 3));
                    self.zeile(&format!("store {} {w}, ptr {p}{}", ir_typ(f.typ), wert_hinweis(f.typ)));
                }
                h
            }
            AusdruckArt::FunktionsWert { name } => {
                let f = self.modul.typen.funktions_typ(ausdruck.typ).expect("die Typprüfung kennt den Typ").clone();
                let umweg = ir_name("fw", &[name]);
                if self.modul.werte_gebaut.insert(name.clone()) {
                    // Der Umweg nimmt die Hülle entgegen (und braucht sie nicht) und ruft die Funktion.
                    let parameter: String =
                        f.parameter.iter().enumerate().map(|(i, t)| format!(", {} %p{i}", ir_typ(*t))).collect();
                    let werte: Vec<String> =
                        f.parameter.iter().enumerate().map(|(i, t)| format!("{} %p{i}", ir_typ(*t))).collect();
                    let rt = ir_typ(f.rueckgabe);
                    let aufruf = format!("call {rt} {}({})", funktions_name(name), werte.join(", "));
                    let rumpf = if f.rueckgabe == Typ::Nix {
                        format!("  {aufruf}\n  ret void")
                    } else {
                        format!("  %r = {aufruf}\n  ret {rt} %r")
                    };
                    let _ = writeln!(
                        self.modul.zusatz,
                        "define internal {rt} {umweg}(ptr %huelle{parameter}) {{\nstart:\n{rumpf}\n}}\n"
                    );
                }
                let (h, ap, cp) = (self.temp(), self.temp(), self.temp());
                self.zeile(&format!("{h} = call ptr @oida_objekt_neu(i64 24)"));
                self.zeile(&format!("{ap} = getelementptr inbounds {{ i64, ptr, ptr }}, ptr {h}, i32 0, i32 1"));
                self.zeile(&format!("store ptr @oida.huelle.fest, ptr {ap}"));
                self.zeile(&format!("{cp} = getelementptr inbounds {{ i64, ptr, ptr }}, ptr {h}, i32 0, i32 2"));
                self.zeile(&format!("store ptr {umweg}, ptr {cp}"));
                h
            }
            AusdruckArt::WertAufruf { funktion, argumente } => {
                // Die Hülle wird der Funktion nur geliehen.
                let (h, h_eigen) = self.geliehen(funktion);
                let mut werte = vec![format!("ptr {h}")];
                for a in argumente {
                    let w = self.ausdruck(a);
                    werte.push(format!("{} {w}", ir_typ(a.typ)));
                }
                let (cp, code) = (self.temp(), self.temp());
                self.zeile(&format!("{cp} = getelementptr inbounds {{ i64, ptr, ptr }}, ptr {h}, i32 0, i32 2"));
                self.zeile(&format!("{code} = load ptr, ptr {cp}"));
                let aufruf = format!("call {} {code}({})", ir_typ(ausdruck.typ), werte.join(", "));
                let t = if ausdruck.typ == Typ::Nix {
                    self.zeile(&aufruf);
                    String::new()
                } else {
                    let t = self.temp();
                    self.zeile(&format!("{t} = {aufruf}"));
                    t
                };
                self.lass_wenn_eigen(&h, funktion.typ, h_eigen);
                t
            }
            AusdruckArt::Erzeug { .. } => {
                // Bei Strukturen und Klassen hat die Typprüfung daraus einen Bau oder
                // ein neues Objekt gemacht; übrig bleibt das leere Verzeichnis.
                let (k, _) = self.modul.typen.verzeichnis(ausdruck.typ).expect("nur Verzeichnisse kommen hier an");
                let t = self.temp();
                self.zeile(&format!("{t} = call ptr @oida_wb_neu(i64 {})", (k == Typ::Kettn) as i64));
                t
            }
            AusdruckArt::Nix => match ausdruck.typ {
                Typ::Vielleicht { zeiger: true, .. } => "null".to_string(),
                _ => "zeroinitializer".to_string(),
            },
            AusdruckArt::IstNix { innen, ist } => {
                let (w, eigen) = self.geliehen(innen);
                let da = self.ist_da(&w, innen.typ);
                let t = if *ist {
                    let t = self.temp();
                    self.zeile(&format!("{t} = xor i1 {da}, true"));
                    t
                } else {
                    da
                };
                self.lass_wenn_eigen(&w, innen.typ, eigen);
                t
            }
            AusdruckArt::OdaHoid { links, rechts } => self.oda_hoid(links, rechts, ausdruck.typ),
            AusdruckArt::Probier { innen } => self.probier(innen),
            AusdruckArt::ElternAufruf { name, argumente, klasse } => {
                let typen = self.modul.typen;
                let selbst = self.selbst.expect("die Typprüfung lässt das nur in Methoden zu");
                let o = self.temp();
                self.zeile(&format!("{o} = load ptr, ptr %v{selbst}, !tbaa !4"));
                let mut werte = vec![format!("ptr {o}")];
                for a in argumente {
                    let w = self.ausdruck(a);
                    werte.push(format!("{} {w}", ir_typ(a.typ)));
                }
                let ziel = methoden_name(&typen.klassen[*klasse as usize].name, name);
                let aufruf = format!("call {} {ziel}({})", ir_typ(ausdruck.typ), werte.join(", "));
                if ausdruck.typ == Typ::Nix {
                    self.zeile(&aufruf);
                    String::new()
                } else {
                    let t = self.temp();
                    self.zeile(&format!("{t} = {aufruf}"));
                    t
                }
            }
        }
    }

    /// Der Schlüssel, wie ihn die Laufzeit haben will: 8 Bytes, bei Texten der Zeiger als Zahl.
    fn wb_schluessel(&mut self, wert: &str, typ: Typ) -> String {
        if typ != Typ::Kettn {
            return wert.to_string();
        }
        let t = self.temp();
        self.zeile(&format!("{t} = ptrtoint ptr {wert} to i64"));
        t
    }

    /// Wo im Verzeichnis `v` der Wert an der Stelle `stelle` liegt.
    fn wb_wert_platz(&mut self, v: &str, stelle: &str, wert_typ: Typ) -> String {
        let (wp, werte, p) = (self.temp(), self.temp(), self.temp());
        self.zeile(&format!("{wp} = getelementptr inbounds %oida.Wb, ptr {v}, i32 0, i32 3"));
        self.zeile(&format!("{werte} = load ptr, ptr {wp}"));
        self.zeile(&format!("{p} = getelementptr inbounds {}, ptr {werte}, i64 {stelle}", ir_typ(wert_typ)));
        p
    }

    /// `verzeichnis[schlüssel]`: der Wert, oder NIX, wenn es den Schlüssel nicht gibt.
    fn wb_lesen(&mut self, verzeichnis: &Ausdruck, schluessel: &Ausdruck, ergebnis: Typ) -> String {
        let (k_typ, w_typ) = self.modul.typen.verzeichnis(verzeichnis.typ).expect("von der Typprüfung geprüft");
        let (v, v_eigen) = self.geliehen(verzeichnis);
        let (k, k_eigen) = self.geliehen(schluessel);
        let roh = self.wb_schluessel(&k, k_typ);
        let (stelle, da) = (self.temp(), self.temp());
        let m_da = self.marke("wb_da");
        let m_ende = self.marke("wb_ende");
        self.zeile(&format!("{stelle} = call i64 @oida_wb_finde(ptr {v}, i64 {roh})"));
        self.zeile(&format!("{da} = icmp sge i64 {stelle}, 0"));
        let von_vorne = self.block.clone();
        self.zeile(&format!("br i1 {da}, label %{m_da}, label %{m_ende}"));

        self.beginne_block(&m_da);
        let p = self.wb_wert_platz(&v, &stelle, w_typ);
        let w = self.temp();
        let (wt, et) = (ir_typ(w_typ), ir_typ(ergebnis));
        self.zeile(&format!("{w} = load {wt}, ptr {p}"));
        self.behalt(&w, w_typ);
        // Als VIELLEICHT: ein Zeiger bleibt, wie er ist; sonst kommt der Merker "ist da" dazu.
        let gefunden = if matches!(ergebnis, Typ::Vielleicht { zeiger: true, .. }) {
            w
        } else {
            let (a, b) = (self.temp(), self.temp());
            self.zeile(&format!("{a} = insertvalue {et} zeroinitializer, i1 true, 0"));
            self.zeile(&format!("{b} = insertvalue {et} {a}, {wt} {w}, 1"));
            b
        };
        self.zeile(&format!("br label %{m_ende}"));

        self.beginne_block(&m_ende);
        let nix = if matches!(ergebnis, Typ::Vielleicht { zeiger: true, .. }) { "null" } else { "zeroinitializer" };
        let t = self.temp();
        self.zeile(&format!("{t} = phi {et} [ {gefunden}, %{m_da} ], [ {nix}, %{von_vorne} ]"));
        self.lass_wenn_eigen(&k, k_typ, k_eigen);
        self.lass_wenn_eigen(&v, verzeichnis.typ, v_eigen);
        t
    }

    /// `verzeichnis[schlüssel] = wert`: ersetzt den Wert oder legt einen neuen Eintrag an.
    fn wb_schreiben(&mut self, verzeichnis: &Ausdruck, schluessel: &Ausdruck, wert: &Ausdruck) {
        let (k_typ, w_typ) = self.modul.typen.verzeichnis(verzeichnis.typ).expect("von der Typprüfung geprüft");
        let wt = ir_typ(w_typ);
        // Erst den Wert ausrechnen (er gehört danach dem Verzeichnis), dann erst suchen.
        let w = self.ausdruck(wert);
        let (v, v_eigen) = self.geliehen(verzeichnis);
        let (k, k_eigen) = self.geliehen(schluessel);
        let roh = self.wb_schluessel(&k, k_typ);
        let (stelle, da) = (self.temp(), self.temp());
        let m_da = self.marke("wb_ersetzen");
        let m_neu = self.marke("wb_neu");
        let m_ende = self.marke("wb_ende");
        self.zeile(&format!("{stelle} = call i64 @oida_wb_finde(ptr {v}, i64 {roh})"));
        self.zeile(&format!("{da} = icmp sge i64 {stelle}, 0"));
        self.zeile(&format!("br i1 {da}, label %{m_da}, label %{m_neu}"));

        // Den Schlüssel gibt es schon: Der alte Wert wird nicht mehr gebraucht.
        self.beginne_block(&m_da);
        let p = self.wb_wert_platz(&v, &stelle, w_typ);
        if self.modul.typen.verwaltet(w_typ) {
            let alt = self.temp();
            self.zeile(&format!("{alt} = load {wt}, ptr {p}"));
            self.zeile(&format!("store {wt} {w}, ptr {p}"));
            self.lass(&alt, w_typ);
        } else {
            self.zeile(&format!("store {wt} {w}, ptr {p}"));
        }
        self.zeile(&format!("br label %{m_ende}"));

        // Ein neuer Eintrag: Die Laufzeit macht Platz (und merkt sich den Schlüssel).
        self.beginne_block(&m_neu);
        let neu = self.temp();
        self.zeile(&format!("{neu} = call i64 @oida_wb_dazu(ptr {v}, i64 {roh}, i64 {})", groesse(w_typ)));
        let p = self.wb_wert_platz(&v, &neu, w_typ);
        self.zeile(&format!("store {wt} {w}, ptr {p}"));
        self.zeile(&format!("br label %{m_ende}"));

        self.beginne_block(&m_ende);
        self.lass_wenn_eigen(&k, k_typ, k_eigen);
        self.lass_wenn_eigen(&v, verzeichnis.typ, v_eigen);
    }

    /// Ist bei einem `VIELLEICHT`-Wert etwas da?
    fn ist_da(&mut self, wert: &str, typ: Typ) -> String {
        let t = self.temp();
        match typ {
            Typ::Vielleicht { zeiger: true, .. } => self.zeile(&format!("{t} = icmp ne ptr {wert}, null")),
            _ => self.zeile(&format!("{t} = extractvalue {} {wert}, 0", ir_typ(typ))),
        }
        t
    }

    /// `links ODA HOID rechts`: Rechts wird nur ausgerechnet, wenn links NIX oder ein Fehler ist.
    fn oda_hoid(&mut self, links: &Ausdruck, rechts: &Ausdruck, typ: Typ) -> String {
        let l = self.ausdruck(links);
        let lt = ir_typ(links.typ);
        let m_da = self.marke("hoid_da");
        let m_sunst = self.marke("hoid_sunst");
        let m_ende = self.marke("hoid_ende");
        // Was links da ist, wandert in das Ergebnis; es bleibt gleich oft gezählt.
        let wert_da = if matches!(links.typ, Typ::Ergebnis(_)) {
            let (f, gut, w) = (self.temp(), self.temp(), self.temp());
            self.zeile(&format!("{f} = extractvalue {lt} {l}, 0"));
            self.zeile(&format!("{gut} = icmp eq ptr {f}, null"));
            self.zeile(&format!("br i1 {gut}, label %{m_da}, label %{m_sunst}"));
            self.beginne_block(&m_da);
            self.zeile(&format!("{w} = extractvalue {lt} {l}, 1"));
            self.zeile(&format!("br label %{m_ende}"));
            self.beginne_block(&m_sunst);
            // Den Fehler will hier niemand mehr.
            self.lass(&f, Typ::Fehla);
            w
        } else {
            let da = self.ist_da(&l, links.typ);
            self.zeile(&format!("br i1 {da}, label %{m_da}, label %{m_sunst}"));
            self.beginne_block(&m_da);
            let w = if let Typ::Vielleicht { zeiger: false, .. } = links.typ {
                let w = self.temp();
                self.zeile(&format!("{w} = extractvalue {lt} {l}, 1"));
                w
            } else {
                l.clone()
            };
            self.zeile(&format!("br label %{m_ende}"));
            self.beginne_block(&m_sunst);
            w
        };
        let r = self.ausdruck(rechts);
        let von_rechts = self.block.clone();
        self.zeile(&format!("br label %{m_ende}"));
        self.beginne_block(&m_ende);
        let t = self.temp();
        self.zeile(&format!("{t} = phi {} [ {wert_da}, %{m_da} ], [ {r}, %{von_rechts} ]", ir_typ(typ)));
        t
    }

    /// `PROBIER ausdruck`: Bei einem Fehler hört die Funktion hier auf und
    /// gibt ihn weiter. Die Typprüfung lässt das nur ganz vorne in einer
    /// Anweisung zu, also hängt hier kein Zwischenwert in der Luft.
    fn probier(&mut self, innen: &Ausdruck) -> String {
        let e = self.ausdruck(innen);
        let et = ir_typ(innen.typ);
        let (f, gut) = (self.temp(), self.temp());
        let m_weiter = self.marke("probier_weida");
        let m_raus = self.marke("probier_raus");
        self.zeile(&format!("{f} = extractvalue {et} {e}, 0"));
        self.zeile(&format!("{gut} = icmp eq ptr {f}, null"));
        self.zeile(&format!("br i1 {gut}, label %{m_weiter}, label %{m_raus}"));

        // Aufräumen wie bei GIB ZRUCK, dann den Fehler als Ergebnis dieser Funktion zurückgeben.
        self.beginne_block(&m_raus);
        self.lass_variablen(self.verwaltet.len());
        let rt = ir_typ(self.rueckgabe);
        let weiter = self.temp();
        self.zeile(&format!("{weiter} = insertvalue {rt} zeroinitializer, ptr {f}, 0"));
        self.zeile(&format!("ret {rt} {weiter}"));

        self.beginne_block(&m_weiter);
        if self.modul.typen.wert_von(innen.typ) == Some(Typ::Nix) {
            return String::new();
        }
        let w = self.temp();
        self.zeile(&format!("{w} = extractvalue {et} {e}, 1"));
        w
    }

    /// WOAHR oder FOISCH als Text. Das sind feste Texte, die niemand zählt.
    fn jona_als_text(&mut self, wert: &str) -> String {
        let woahr = self.modul.woahr.clone();
        let foisch = self.modul.foisch.clone();
        let (woahr, foisch) = (self.modul.text(&woahr), self.modul.text(&foisch));
        let t = self.temp();
        self.zeile(&format!("{t} = select i1 {wert}, ptr {woahr}, ptr {foisch}"));
        t
    }

    /// Aufruf einer eingebauten Funktion. Texte und Listen werden dabei nur
    /// gelesen; kommt ein Text heraus, ist er neu und gehört dem Empfänger.
    fn eingebaut(&mut self, funktion: Eingebaut, argumente: &[Ausdruck], ergebnis: Typ, zeile: u32) -> String {
        match funktion {
            Eingebaut::Dazu => {
                let element = argumente[1].typ;
                let (l, l_eigen) = self.geliehen(&argumente[0]);
                // Erst den Wert ausrechnen, dann erst Platz holen (siehe Zuweisung).
                let w = self.ausdruck(&argumente[1]);
                let p = self.temp();
                self.zeile(&format!("{p} = call ptr @oida_liste_neuer_platz(ptr {l}, i64 {})", groesse(element)));
                self.zeile(&format!("store {} {w}, ptr {p}{}", ir_typ(element), wert_hinweis(element)));
                self.lass_wenn_eigen(&l, argumente[0].typ, l_eigen);
                return String::new();
            }
            Eingebaut::Raus if matches!(argumente[0].typ, Typ::Verzeichnis(_)) => {
                // Den Eintrag mit dem Schlüssel entfernen; gibt es ihn nicht, passiert nichts.
                let (k_typ, w_typ) = self.modul.typen.verzeichnis(argumente[0].typ).expect("gerade geprüft");
                let (v, v_eigen) = self.geliehen(&argumente[0]);
                let (k, k_eigen) = self.geliehen(&argumente[1]);
                let roh = self.wb_schluessel(&k, k_typ);
                let (stelle, da) = (self.temp(), self.temp());
                let m_da = self.marke("wb_raus");
                let m_ende = self.marke("wb_ende");
                self.zeile(&format!("{stelle} = call i64 @oida_wb_finde(ptr {v}, i64 {roh})"));
                self.zeile(&format!("{da} = icmp sge i64 {stelle}, 0"));
                self.zeile(&format!("br i1 {da}, label %{m_da}, label %{m_ende}"));
                self.beginne_block(&m_da);
                if self.modul.typen.verwaltet(w_typ) {
                    let p = self.wb_wert_platz(&v, &stelle, w_typ);
                    let alt = self.temp();
                    self.zeile(&format!("{alt} = load {}, ptr {p}", ir_typ(w_typ)));
                    self.lass(&alt, w_typ);
                }
                self.zeile(&format!("call void @oida_wb_raus(ptr {v}, i64 {stelle}, i64 {})", groesse(w_typ)));
                self.zeile(&format!("br label %{m_ende}"));
                self.beginne_block(&m_ende);
                self.lass_wenn_eigen(&k, k_typ, k_eigen);
                self.lass_wenn_eigen(&v, argumente[0].typ, v_eigen);
                return String::new();
            }
            Eingebaut::Raus => {
                let element = self.modul.typen.element(argumente[0].typ).expect("von der Typprüfung geprüft");
                let (l, l_eigen) = self.geliehen(&argumente[0]);
                let stelle = self.ausdruck(&argumente[1]);
                if self.modul.typen.verwaltet(element) {
                    // Was an der Stelle liegt, braucht danach niemand mehr.
                    let (daten, p, alt) = (self.temp(), self.temp(), self.temp());
                    let t = ir_typ(element);
                    self.zeile(&format!("{daten} = call ptr @oida.stelle(ptr {l}, i64 {stelle}, i64 {zeile})"));
                    self.zeile(&format!("{p} = getelementptr inbounds {t}, ptr {daten}, i64 {stelle}"));
                    self.zeile(&format!("{alt} = load {t}, ptr {p}"));
                    self.lass(&alt, element);
                }
                self.zeile(&format!(
                    "call void @oida_liste_raus(ptr {l}, i64 {stelle}, i64 {}, i64 {zeile})",
                    groesse(element)
                ));
                self.lass_wenn_eigen(&l, argumente[0].typ, l_eigen);
                return String::new();
            }
            Eingebaut::ZoiAus | Eingebaut::KommazoiAus => {
                // Die Laufzeit schreibt die Zahl in den Platz und gibt den Fehler zurück (null: keiner).
                let (zahl, funktion) = if funktion == Eingebaut::ZoiAus {
                    ("i64", "oida_text_als_zoi")
                } else {
                    ("double", "oida_text_als_kommazoi")
                };
                let (text, eigen) = self.geliehen(&argumente[0]);
                let platz = self.versteckter_platz(zahl);
                let (f, w, a, b) = (self.temp(), self.temp(), self.temp(), self.temp());
                let et = ir_typ(ergebnis);
                self.zeile(&format!("{f} = call ptr @{funktion}(ptr {text}, ptr {platz})"));
                self.zeile(&format!("{w} = load {zahl}, ptr {platz}"));
                self.zeile(&format!("{a} = insertvalue {et} zeroinitializer, ptr {f}, 0"));
                self.zeile(&format!("{b} = insertvalue {et} {a}, {zahl} {w}, 1"));
                self.lass_wenn_eigen(&text, Typ::Kettn, eigen);
                return b;
            }
            // Bei einem Verzeichnis steht die Länge an derselben Stelle wie bei einer Liste.
            Eingebaut::Laenge if matches!(argumente[0].typ, Typ::Liste(_) | Typ::Verzeichnis(_)) => {
                let (l, l_eigen) = self.geliehen(&argumente[0]);
                let (lp, t) = (self.temp(), self.temp());
                self.zeile(&format!("{lp} = getelementptr inbounds %oida.Liste, ptr {l}, i32 0, i32 1"));
                self.zeile(&format!("{t} = load i64, ptr {lp}{KOPF_HINWEIS}"));
                self.lass_wenn_eigen(&l, argumente[0].typ, l_eigen);
                return t;
            }
            _ => {}
        }

        let werte: Vec<(String, bool)> = argumente.iter().map(|a| self.geliehen(a)).collect();
        let w = |i: usize| werte[i].0.as_str();
        // Viele Rechenfunktionen gibt es als fertigen LLVM-Befehl für Kommazahlen.
        let rechnen = |name: &str| format!("call double @llvm.{name}.f64(double {})", w(0));
        let ganz = ergebnis == Typ::Zoi;
        let befehl = match funktion {
            Eingebaut::Lies if werte.is_empty() => "call ptr @oida_lies(ptr null)".to_string(),
            Eingebaut::Lies => format!("call ptr @oida_lies(ptr {})", w(0)),
            Eingebaut::Laenge => format!("call i64 @oida_text_laenge(ptr {})", w(0)),
            // Ohne Anzahl: bis zum Ende (-1).
            Eingebaut::Stueck if werte.len() == 2 => {
                format!("call ptr @oida_text_stueck(ptr {}, i64 {}, i64 -1, i64 {zeile})", w(0), w(1))
            }
            Eingebaut::Stueck => {
                format!("call ptr @oida_text_stueck(ptr {}, i64 {}, i64 {}, i64 {zeile})", w(0), w(1), w(2))
            }
            Eingebaut::Suche => format!("call i64 @oida_text_suche(ptr {}, ptr {})", w(0), w(1)),
            Eingebaut::Gross => format!("call ptr @oida_text_gross(ptr {})", w(0)),
            Eingebaut::Klein => format!("call ptr @oida_text_klein(ptr {})", w(0)),
            Eingebaut::Stutz => format!("call ptr @oida_text_gestutzt(ptr {})", w(0)),
            Eingebaut::Ersetz => {
                format!("call ptr @oida_text_ersetzt(ptr {}, ptr {}, ptr {}, i64 {zeile})", w(0), w(1), w(2))
            }
            Eingebaut::Wurzel => rechnen("sqrt"),
            Eingebaut::Sinus => rechnen("sin"),
            Eingebaut::Kosinus => rechnen("cos"),
            Eingebaut::Exp => rechnen("exp"),
            Eingebaut::Abrunden => rechnen("floor"),
            Eingebaut::Aufrunden => rechnen("ceil"),
            Eingebaut::Runden => rechnen("round"),
            Eingebaut::Hoch => format!("call double @llvm.pow.f64(double {}, double {})", w(0), w(1)),
            Eingebaut::Winkel => format!("call double @atan2(double {}, double {})", w(0), w(1)),
            Eingebaut::Betrag if ganz => format!("call i64 @llvm.abs.i64(i64 {}, i1 false)", w(0)),
            Eingebaut::Betrag => rechnen("fabs"),
            Eingebaut::Min if ganz => format!("call i64 @llvm.smin.i64(i64 {}, i64 {})", w(0), w(1)),
            Eingebaut::Min => format!("call double @llvm.minnum.f64(double {}, double {})", w(0), w(1)),
            Eingebaut::Max if ganz => format!("call i64 @llvm.smax.i64(i64 {}, i64 {})", w(0), w(1)),
            Eingebaut::Max => format!("call double @llvm.maxnum.f64(double {}, double {})", w(0), w(1)),
            Eingebaut::Zufall if werte.is_empty() => "call double @oida_zufall()".to_string(),
            Eingebaut::Zufall => format!("call i64 @oida_zufall_zoi(i64 {}, i64 {}, i64 {zeile})", w(0), w(1)),
            Eingebaut::Samen => format!("call void @oida_zufall_samen(i64 {})", w(0)),
            Eingebaut::Dazu | Eingebaut::Raus | Eingebaut::ZoiAus | Eingebaut::KommazoiAus => {
                unreachable!("oben erledigt")
            }
        };
        let t = if ergebnis == Typ::Nix {
            self.zeile(&befehl);
            String::new()
        } else {
            let t = self.temp();
            self.zeile(&format!("{t} = {befehl}"));
            t
        };
        for (argument, (wert, eigen)) in argumente.iter().zip(&werte) {
            self.lass_wenn_eigen(wert, argument.typ, *eigen);
        }
        t
    }

    /// Texte zusammenhängen. Eine ganze Kette `a + b + c` wird in einem
    /// Schritt gebaut statt mit einem Zwischentext je Plus.
    fn verbinden(&mut self, ausdruck: &Ausdruck) -> String {
        fn sammeln<'a>(a: &'a Ausdruck, teile: &mut Vec<&'a Ausdruck>) {
            match &a.art {
                AusdruckArt::Binaer { op: BinaerOp::Plus, links, rechts } if a.typ == Typ::Kettn => {
                    sammeln(links, teile);
                    sammeln(rechts, teile);
                }
                _ => teile.push(a),
            }
        }
        let mut teile = Vec::new();
        sammeln(ausdruck, &mut teile);

        let werte: Vec<(String, bool)> = teile.iter().map(|teil| self.geliehen(teil)).collect();
        let t = self.temp();
        if let [(a, _), (b, _)] = &werte[..] {
            self.zeile(&format!("{t} = call ptr @oida_text_verbinden(ptr {a}, ptr {b})"));
        } else {
            // Die Teile kommen in eine kleine Liste am Stack.
            let n = werte.len();
            let liste = self.versteckter_platz(&format!("[{n} x ptr]"));
            for (i, (wert, _)) in werte.iter().enumerate() {
                let platz = self.temp();
                self.zeile(&format!("{platz} = getelementptr inbounds [{n} x ptr], ptr {liste}, i64 0, i64 {i}"));
                self.zeile(&format!("store ptr {wert}, ptr {platz}"));
            }
            self.zeile(&format!("{t} = call ptr @oida_text_verbinden_viele(ptr {liste}, i64 {n})"));
        }
        for (wert, eigen) in &werte {
            self.lass_wenn_eigen(wert, Typ::Kettn, *eigen);
        }
        t
    }

    /// UND und ODA rechnen die rechte Seite nur aus, wenn es nötig ist.
    fn und_oda(&mut self, und: bool, links: &Ausdruck, rechts: &Ausdruck) -> String {
        let l = self.ausdruck(links);
        let von_links = self.block.clone();
        let m_rechts = self.marke(if und { "und_rechts" } else { "oda_rechts" });
        let m_ende = self.marke(if und { "und_ende" } else { "oda_ende" });
        if und {
            self.zeile(&format!("br i1 {l}, label %{m_rechts}, label %{m_ende}"));
        } else {
            self.zeile(&format!("br i1 {l}, label %{m_ende}, label %{m_rechts}"));
        }
        self.beginne_block(&m_rechts);
        let r = self.ausdruck(rechts);
        let von_rechts = self.block.clone();
        self.zeile(&format!("br label %{m_ende}"));
        self.beginne_block(&m_ende);
        let t = self.temp();
        let abkuerzung = if und { "false" } else { "true" };
        self.zeile(&format!("{t} = phi i1 [ {abkuerzung}, %{von_links} ], [ {r}, %{von_rechts} ]"));
        t
    }

    /// Rechnen und Vergleichen. `typ` ist der Typ der beiden Seiten (nach der
    /// Typprüfung sind sie gleich). Texte zusammenhängen macht [`Self::verbinden`].
    fn binaer(&mut self, op: BinaerOp, typ: Typ, l: &str, r: &str, zeile: u32) -> String {
        let t = self.temp();
        let befehl = match (typ, op) {
            (Typ::Zoi, BinaerOp::Plus) => format!("add i64 {l}, {r}"),
            (Typ::Zoi, BinaerOp::Minus) => format!("sub i64 {l}, {r}"),
            (Typ::Zoi, BinaerOp::Mal) => format!("mul i64 {l}, {r}"),
            (Typ::Zoi, BinaerOp::Durch) => format!("call i64 @oida.durch(i64 {l}, i64 {r}, i64 {zeile})"),
            (Typ::Zoi, BinaerOp::Rest) => format!("call i64 @oida.rest(i64 {l}, i64 {r}, i64 {zeile})"),
            (Typ::Kommazoi, BinaerOp::Plus) => format!("fadd double {l}, {r}"),
            (Typ::Kommazoi, BinaerOp::Minus) => format!("fsub double {l}, {r}"),
            (Typ::Kommazoi, BinaerOp::Mal) => format!("fmul double {l}, {r}"),
            (Typ::Kommazoi, BinaerOp::Durch) => format!("fdiv double {l}, {r}"),
            (Typ::Kommazoi, BinaerOp::Rest) => format!("frem double {l}, {r}"),
            (Typ::Zoi, _) => format!("icmp {} i64 {l}, {r}", vergleich_ganz(op)),
            (Typ::Jona, _) => format!("icmp {} i1 {l}, {r}", vergleich_ganz(op)),
            (Typ::Kommazoi, _) => format!("fcmp {} double {l}, {r}", vergleich_komma(op)),
            // Zwei Objekte sind gleich, wenn es dasselbe Objekt ist.
            (Typ::Klasse(_) | Typ::Vielleicht { zeiger: true, .. }, _) => {
                format!("icmp {} ptr {l}, {r}", vergleich_ganz(op))
            }
            (Typ::Kettn, _) => {
                let v = self.temp();
                self.zeile(&format!("{v} = call i32 @oida_text_vergleich(ptr {l}, ptr {r})"));
                format!("icmp {} i32 {v}, 0", vergleich_ganz(op))
            }
            _ => unreachable!("von der Typprüfung abgelehnt"),
        };
        self.zeile(&format!("{t} = {befehl}"));
        t
    }

    /// Wandelt `w` vom Typ `von` in den Typ `ziel` um (die beiden sind verschieden).
    fn umwandlung(&mut self, von: Typ, ziel: Typ, w: &str, zeile: u32) -> String {
        if (von, ziel) == (Typ::Jona, Typ::Kettn) {
            return self.jona_als_text(w);
        }
        let t = self.temp();
        let befehl = match (von, ziel) {
            (Typ::Zoi, Typ::Kommazoi) => format!("sitofp i64 {w} to double"),
            // "sat": zu große Kommazahlen werden zur größten ZOI statt zu Unsinn.
            (Typ::Kommazoi, Typ::Zoi) => format!("call i64 @llvm.fptosi.sat.i64.f64(double {w})"),
            (Typ::Jona, Typ::Zoi) => format!("zext i1 {w} to i64"),
            (Typ::Kettn, Typ::Zoi) => format!("call i64 @oida_text_zu_zoi(ptr {w}, i64 {zeile})"),
            (Typ::Kettn, Typ::Kommazoi) => format!("call double @oida_text_zu_kommazoi(ptr {w}, i64 {zeile})"),
            (Typ::Zoi, Typ::Kettn) => format!("call ptr @oida_zoi_zu_text(i64 {w})"),
            (Typ::Kommazoi, Typ::Kettn) => format!("call ptr @oida_kommazoi_zu_text(double {w})"),
            _ => unreachable!("von der Typprüfung abgelehnt: {von:?} -> {ziel:?}"),
        };
        self.zeile(&format!("{t} = {befehl}"));
        t
    }
}

fn vergleich_ganz(op: BinaerOp) -> &'static str {
    match op {
        BinaerOp::Gleich => "eq",
        BinaerOp::Ungleich => "ne",
        BinaerOp::Kleiner => "slt",
        BinaerOp::KleinerGleich => "sle",
        BinaerOp::Groesser => "sgt",
        BinaerOp::GroesserGleich => "sge",
        _ => unreachable!("kein Vergleich: {op:?}"),
    }
}

fn vergleich_komma(op: BinaerOp) -> &'static str {
    match op {
        BinaerOp::Gleich => "oeq",
        BinaerOp::Ungleich => "une",
        BinaerOp::Kleiner => "olt",
        BinaerOp::KleinerGleich => "ole",
        BinaerOp::Groesser => "ogt",
        BinaerOp::GroesserGleich => "oge",
        _ => unreachable!("kein Vergleich: {op:?}"),
    }
}

/// Ein fester Text als IR-Konstante: Zähler (-1 = nie zählen), Länge, Bytes.
fn text_konstante(nummer: usize, inhalt: &str) -> String {
    let mut bytes = String::new();
    for b in inhalt.bytes() {
        if (b' '..=b'~').contains(&b) && b != b'"' && b != b'\\' {
            bytes.push(b as char);
        } else {
            let _ = write!(bytes, "\\{b:02X}");
        }
    }
    let n = inhalt.len();
    format!(
        "@.text.{nummer} = private unnamed_addr constant {{ i64, i64, [{n} x i8] }} {{ i64 -1, i64 {n}, [{n} x i8] c\"{bytes}\" }}\n"
    )
}

/// Wie viele Bytes die Hülle einer Funktion ohne Namen braucht.
fn huellen_groesse(nummer: usize) -> String {
    format!("ptrtoint (ptr getelementptr (%h{nummer}, ptr null, i32 1) to i64)")
}

/// Wozu eine Funktion gehört.
enum FunktionsArt<'a> {
    /// Eine Funktion mit Namen, ganz außen.
    Frei,
    /// Eine Methode der Klasse mit diesem Namen.
    Methode(&'a str),
    /// Eine Funktion ohne Namen mit dem, was sie von außen mitnimmt.
    Lambda { nummer: usize, fang: &'a [Fang] },
}

/// Der Typ, wie ihn eine Funktion der Laufzeit bekommt (`rueckgabe` falsch)
/// oder zurückgibt. Ein Wahrheitswert reist als ganzes Byte, wie `bool` in C
/// und Rust; LLVM will den Hinweis darauf beim Parameter hinter dem Typ, bei
/// der Rückgabe davor.
fn aussen_typ(typ: Typ, rueckgabe: bool) -> String {
    match (typ, rueckgabe) {
        (Typ::Jona, true) => "zeroext i1".to_string(),
        (Typ::Jona, false) => "i1 zeroext".to_string(),
        _ => ir_typ(typ),
    }
}

/// `FUNKTIAUN f(...) VON DRAUSSN "oida_x"`: Die OIDA-Funktion ruft nur die
/// Funktion der Laufzeit auf. Texte und Listen sind ihr dabei geliehen;
/// danach lässt die OIDA-Funktion sie los, wie jede Funktion ihre Parameter.
fn baue_aussen(modul: &mut Modul, f: &Funktion, symbol: &str) -> String {
    let typen = modul.typen;
    let rt = ir_typ(f.rueckgabe);
    if modul.werte_gebaut.insert(format!("draussen:{symbol}")) {
        let parameter: Vec<String> = f.parameter.iter().map(|p| aussen_typ(p.typ, false)).collect();
        let _ = writeln!(
            modul.zusatz,
            "declare {} @{symbol}({})\n",
            aussen_typ(f.rueckgabe, true),
            parameter.join(", ")
        );
    }
    let kopf: Vec<String> = f.parameter.iter().enumerate().map(|(i, p)| format!("{} %p{i}", ir_typ(p.typ))).collect();
    let werte: Vec<String> =
        f.parameter.iter().enumerate().map(|(i, p)| format!("{} %p{i}", aussen_typ(p.typ, false))).collect();
    let mut code = String::new();
    let aufruf = format!("call {} @{symbol}({})", aussen_typ(f.rueckgabe, true), werte.join(", "));
    if f.rueckgabe == Typ::Nix {
        let _ = writeln!(code, "  {aufruf}");
    } else {
        let _ = writeln!(code, "  %r = {aufruf}");
    }
    for (i, p) in f.parameter.iter().enumerate() {
        if let Some(befehl) = lass_befehl(typen, p.typ, &format!("%p{i}")) {
            let _ = writeln!(code, "  {befehl}");
        }
    }
    let ende = if f.rueckgabe == Typ::Nix { "ret void".to_string() } else { format!("ret {rt} %r") };
    format!("define internal {rt} {}({}) {{\nstart:\n{code}  {ende}\n}}\n\n", funktions_name(&f.name), kopf.join(", "))
}

/// Erzeugt den Code einer Funktion.
fn baue_funktion(modul: &mut Modul, f: &Funktion, art: FunktionsArt) -> String {
    if let Some(symbol) = &f.aussen {
        return baue_aussen(modul, f, symbol);
    }
    let typen = modul.typen;
    let mut bau = FunktionsBau::neu(modul);
    bau.rueckgabe = f.rueckgabe;
    let mut kopf = Vec::new();
    let name = match art {
        FunktionsArt::Frei => funktions_name(&f.name),
        FunktionsArt::Methode(klasse) => {
            // SÖWA ist nur geliehen: Wer die Methode aufruft, hält das Objekt fest.
            kopf.push("ptr %selbst".to_string());
            let _ = writeln!(bau.plaetze, "  %v{} = alloca ptr", f.selbst_id);
            bau.zeile(&format!("store ptr %selbst, ptr %v{}", f.selbst_id));
            bau.selbst = Some(f.selbst_id);
            methoden_name(klasse, &f.name)
        }
        FunktionsArt::Lambda { nummer, fang } => {
            // Was die Funktion mitgenommen hat, liegt in der Hülle. Solange die
            // Funktion läuft, hält der Aufrufer die Hülle fest: Die Werte sind nur geliehen.
            kopf.push("ptr %huelle".to_string());
            for (i, wert) in fang.iter().enumerate() {
                let t = ir_typ(wert.typ);
                let _ = writeln!(bau.plaetze, "  %v{} = alloca {t}", wert.id);
                let (p, w) = (bau.temp(), bau.temp());
                bau.zeile(&format!("{p} = getelementptr inbounds %h{nummer}, ptr %huelle, i32 0, i32 {}", i + 3));
                bau.zeile(&format!("{w} = load {t}, ptr {p}{}", wert_hinweis(wert.typ)));
                bau.zeile(&format!("store {t} {w}, ptr %v{}{}", wert.id, wert_hinweis(wert.typ)));
            }
            format!("@oida.l.{nummer}")
        }
    };
    for (i, p) in f.parameter.iter().enumerate() {
        kopf.push(format!("{} %p{i}", ir_typ(p.typ)));
        // Die Funktion besitzt ihre Parameter wie eigene Variablen.
        bau.platz(p.id, p.typ);
        bau.zeile(&format!("store {} %p{i}, ptr %v{}", ir_typ(p.typ), p.id));
    }
    for a in &f.rumpf {
        bau.anweisung(a);
    }
    if f.rueckgabe == Typ::Nix {
        bau.lass_variablen(1);
        bau.zeile("ret void");
    } else if typen.wert_von(f.rueckgabe) == Some(Typ::Nix) {
        // NIX ODA FEHLA: Wer bis hierher kommt, hat es geschafft.
        bau.lass_variablen(1);
        bau.zeile(&format!("ret {} zeroinitializer", ir_typ(f.rueckgabe)));
    } else {
        // Die Typprüfung stellt sicher, dass jeder Weg mit GIB ZRUCK aufhört.
        bau.zeile("unreachable");
    }
    format!(
        "define internal {} {name}({}) {{\nstart:\n{}{}}}\n\n",
        ir_typ(f.rueckgabe),
        kopf.join(", "),
        bau.plaetze,
        bau.code
    )
}

/// Erzeugt das LLVM-IR für ein Programm, das die Typprüfung bestanden hat.
pub fn erzeugen(programm: &Programm, wb: &Woerterbuch) -> String {
    let mut modul = Modul {
        texte: Vec::new(),
        text_nummer: HashMap::new(),
        woahr: wb.wort(Schluessel::True).to_string(),
        foisch: wb.wort(Schluessel::False).to_string(),
        nix: wb.wort(Schluessel::Null).to_string(),
        fehla: wb.wort(Schluessel::Error).to_string(),
        funktion_wort: wb.wort(Schluessel::Function).to_string(),
        zusatz: String::new(),
        lambdas_gebaut: std::collections::HashSet::new(),
        werte_gebaut: std::collections::HashSet::new(),
        typen: &programm.typen,
        klassen: &programm.klassen,
    };
    let mut funktionen = String::new();

    // Erst die Funktionen (ohne Klasse), dann die Methoden jeder Klasse.
    for f in &programm.funktionen {
        let code = baue_funktion(&mut modul, f, FunktionsArt::Frei);
        funktionen.push_str(&code);
    }
    for (n, k) in programm.klassen.iter().enumerate() {
        if programm.typen.klassen[n].vorlage {
            continue; // Aus einer Vorlage entsteht selbst kein Code.
        }
        for m in &k.methoden {
            let code = baue_funktion(&mut modul, m, FunktionsArt::Methode(&k.name));
            funktionen.push_str(&code);
        }
    }

    let mut bau = FunktionsBau::neu(&mut modul);
    bau.zeile("call void @oida_start()");
    for a in &programm.haupt {
        bau.anweisung(a);
    }
    bau.lass_variablen(1);
    bau.zeile("call void @oida_ende()");
    bau.zeile("ret i32 0");
    let _ = write!(funktionen, "define i32 @main() {{\nstart:\n{}{}}}\n", bau.plaetze, bau.code);

    // Die Hilfsfunktionen je Typ kommen zuletzt dran, weil sie noch feste Texte anlegen.
    let hilfen = modul.hilfsfunktionen();

    let mut aus = String::from(KOPF);
    for (n, s) in programm.typen.strukturen.iter().enumerate() {
        let felder: Vec<String> = s.felder.iter().map(|(_, typ)| ir_typ(*typ)).collect();
        let _ = writeln!(aus, "%s{n} = type {{ {} }}", felder.join(", "));
    }
    // VIELLEICHT (wo der Wert kein Zeiger ist): Merker "ist da", dann der Wert.
    for (n, innen) in programm.typen.vielleicht.iter().enumerate() {
        if brauchbar(*innen) && *innen != Typ::Nix && !programm.typen.zeiger(*innen) {
            let _ = writeln!(aus, "%v{n} = type {{ i1, {} }}", ir_typ(*innen));
        }
    }
    // ... ODA FEHLA: der Fehler (null: keiner), dann der Wert.
    for (n, wert) in programm.typen.ergebnisse.iter().enumerate() {
        if *wert == Typ::Nix {
            let _ = writeln!(aus, "%e{n} = type {{ ptr }}");
        } else if brauchbar(*wert) {
            let _ = writeln!(aus, "%e{n} = type {{ ptr, {} }}", ir_typ(*wert));
        }
    }
    // Ein Objekt: Zähler, Angaben zur Klasse, dann die Felder.
    for (n, k) in programm.typen.klassen.iter().enumerate() {
        if k.vorlage {
            continue;
        }
        let felder: String = k.felder.iter().map(|feld| format!(", {}", ir_typ(feld.typ))).collect();
        let _ = writeln!(aus, "%k{n} = type {{ i64, ptr{felder} }}");
    }
    aus.push('\n');
    for (nummer, inhalt) in modul.texte.iter().enumerate() {
        aus.push_str(&text_konstante(nummer, inhalt));
    }
    aus.push('\n');
    aus.push_str(&hilfen);
    aus.push_str(&modul.zusatz);
    aus.push_str(&funktionen);
    aus.push_str(FUSS);
    aus
}

#[cfg(test)]
mod tests {
    use crate::treiber::zu_ir;
    use crate::woerterbuch::Woerterbuch;

    fn ir(quelltext: &str) -> String {
        zu_ir(quelltext, &Woerterbuch::eingebaut()).unwrap_or_else(|f| panic!("unerwartete Fehler: {f:?}"))
    }

    const FAMILIE: &str = "KLASSE Wesen\n name KETTN\n \
        FUNKTIAUN sog() GIBT ZRUCK KETTN\n  GIB ZRUCK SÖWA.name OIDA\n PASST SCHO\n \
        FUNKTIAUN lang() GIBT ZRUCK ZOI\n  GIB ZRUCK LÄNGN(SÖWA.name) OIDA\n PASST SCHO\nPASST SCHO\n\
        KLASSE Gegner ERBT VON Wesen\n \
        FUNKTIAUN sog() GIBT ZRUCK KETTN\n  GIB ZRUCK \"Grr\" OIDA\n PASST SCHO\nPASST SCHO\n";

    /// Die Zeilen des Hauptprogramms.
    fn haupt(ir: &str) -> &str {
        let anfang = ir.find("define i32 @main()").expect("jedes Programm hat ein main");
        let ende = ir[anfang..].find("\n}\n").expect("main hört auf");
        &ir[anfang..anfang + ende]
    }

    #[test]
    fn aufbau_von_objekt_und_klasse() {
        let ir = ir(&format!("{FAMILIE}SOG AU g = Gegner(\"Sepp\") OIDA"));
        // Zähler, Angaben zur Klasse, dann die Felder.
        assert!(ir.contains("%k0 = type { i64, ptr, ptr }"), "{ir}");
        assert!(ir.contains("%k1 = type { i64, ptr, ptr }"), "{ir}");
        // Ein Platz in der Sprungtabelle (für sog); jede Klasse trägt ihre Fassung ein.
        assert!(ir.contains(
            "@oida.klasse.0 = internal constant { ptr, ptr, [1 x ptr] } \
             { ptr @oida.frei.k0, ptr @oida.schreib.k0, [1 x ptr] [ptr @\"oida.m.Wesen.sog\"] }"
        ));
        assert!(ir.contains("[1 x ptr] [ptr @\"oida.m.Gegner.sog\"] }"));
        assert!(ir.contains("define internal ptr @\"oida.m.Gegner.sog\"(ptr %selbst)"));
        assert!(haupt(&ir).contains("store ptr @oida.klasse.1, ptr"));
    }

    #[test]
    fn nur_ueberschriebene_methoden_gehen_ueber_die_tabelle() {
        let ir = ir(&format!(
            "{FAMILIE}SOG AU w Wesen = Gegner(\"a\") OIDA\nSOG AU g = Gegner(\"b\") OIDA\n\
             SCHREIB(w.sog(), w.lang(), g.sog()) OIDA"
        ));
        let haupt = haupt(&ir);
        // w könnte ein Wesen oder ein Gegner sein: nachschauen.
        assert!(haupt.contains("getelementptr inbounds { ptr, ptr, [1 x ptr] }"), "{haupt}");
        assert_eq!(haupt.matches("getelementptr inbounds { ptr, ptr, [1 x ptr] }").count(), 1, "{haupt}");
        // lang() gibt es nur einmal, und unter Gegner gibt es keine andere Fassung von sog().
        assert!(haupt.contains("call i64 @\"oida.m.Wesen.lang\"(ptr"), "{haupt}");
        assert!(haupt.contains("call ptr @\"oida.m.Gegner.sog\"(ptr"), "{haupt}");
        // Bei w steht erst beim Laufen fest, wie freigegeben wird; bei g schon jetzt.
        assert!(haupt.contains("call void @oida.lass.objekt(ptr"), "{haupt}");
        assert!(haupt.contains("call void @oida.lass.k1(ptr"), "{haupt}");
    }

    #[test]
    fn vielleicht_im_speicher() {
        let ir = ir("KLASSE Wesen\n name KETTN\nPASST SCHO\n\
             SOG AU w VIELLEICHT Wesen = NIX OIDA\nSOG AU z VIELLEICHT ZOI = NIX OIDA\nSOG AU t VIELLEICHT KETTN = NIX OIDA\n\
             SCHREIB(w == NIX, z != NIX, z ODA HOID 3) OIDA\nz = 5 OIDA\nSCHREIB(z + 1) OIDA");
        // Ein Objekt oder Text, der fehlen kann: derselbe Zeiger, NIX ist null.
        let haupt = haupt(&ir);
        assert!(haupt.contains("store ptr null, ptr %v"), "{haupt}");
        assert!(haupt.contains("icmp ne ptr"), "{haupt}");
        // Eine Zahl, die fehlen kann: ein Merker davor.
        assert!(ir.contains(" = type { i1, i64 }"), "{ir}");
        assert!(!ir.contains(" = type { i1, ptr }"), "{ir}");
        assert!(haupt.contains("zeroinitializer, ptr %v"), "{haupt}");
        // Nach z = 5 steht fest, dass z da ist: Es wird ohne Nachschauen gelesen.
        assert!(haupt.contains("i32 0, i32 1\n"), "{haupt}");
    }

    #[test]
    fn fehler_im_speicher() {
        let ir = ir("FUNKTIAUN teil(a ZOI, b ZOI) GIBT ZRUCK ZOI ODA FEHLA\n \
             WENN b == 0 DAUN\n  GIB ZRUCK FEHLA(\"na\") OIDA\n PASST SCHO\n GIB ZRUCK a / b OIDA\nPASST SCHO\n\
             FUNKTIAUN zwamoi(a ZOI) GIBT ZRUCK NIX ODA FEHLA\n SOG AU q = PROBIER teil(a, 2) OIDA\n SCHREIB(q) OIDA\nPASST SCHO\n\
             PROBIER zwamoi(4)\nWONNS SCHIEFGEHT f\n SCHREIB(f.meldung) OIDA\nPASST SCHO");
        // Der Fehler (null: keiner), dann der Wert; ohne Wert nur der Fehler.
        assert!(ir.contains("%e0 = type { ptr, i64 }") && ir.contains("%e1 = type { ptr }"), "{ir}");
        assert!(ir.contains("define internal %e0 @\"oida.f.teil\"(i64 %p0, i64 %p1)"), "{ir}");
        // PROBIER: Bei einem Fehler hört zwamoi auf und gibt ihn in seinem eigenen Typ weiter.
        let anfang = ir.find("define internal %e1 @\"oida.f.zwamoi\"").expect("zwamoi gibt es");
        let zwamoi = &ir[anfang..anfang + ir[anfang..].find("\n}\n").unwrap()];
        assert!(zwamoi.contains("insertvalue %e1 zeroinitializer, ptr %t"), "{zwamoi}");
        // Am Ende ohne GIB ZRUCK: hat geklappt.
        assert!(zwamoi.trim_end().ends_with("ret %e1 zeroinitializer"), "{zwamoi}");
    }

    #[test]
    fn funktionen_als_werte_im_speicher() {
        let ir = ir("FUNKTIAUN doppelt(x ZOI) GIBT ZRUCK ZOI\n GIB ZRUCK x * 2 OIDA\nPASST SCHO\n\
             SOG AU a = 5 OIDA\nSOG AU t = \"x\" OIDA\nSOG AU f = FUNKTIAUN(x ZOI) => x + a + LÄNGN(t) OIDA\n\
             SOG AU g = doppelt OIDA\nSCHREIB(f(1), g(2)) OIDA");
        // Die Hülle: wie ein Objekt, dann der Code, dann was mitgenommen wurde (eine Zahl, ein Text).
        assert!(ir.contains("%h0 = type { i64, ptr, ptr, i64, ptr }"), "{ir}");
        assert!(ir.contains("define internal i64 @oida.l.0(ptr %huelle, i64 %p0)"), "{ir}");
        // Beim Freigeben der Hülle wird der Text losgelassen, die Zahl braucht nichts.
        let anfang = ir.find("define internal void @oida.frei.h0").unwrap();
        let frei = &ir[anfang..anfang + ir[anfang..].find("\n}\n").unwrap()];
        assert_eq!(frei.matches("call void @oida.lass(").count(), 1, "{frei}");
        // Eine Funktion mit Namen bekommt einen Umweg, der die Hülle entgegennimmt.
        assert!(ir.contains("define internal i64 @\"oida.fw.doppelt\"(ptr %huelle, i64 %p0)"), "{ir}");
        assert!(haupt(&ir).contains("store ptr @\"oida.fw.doppelt\", ptr"), "{ir}");
    }

    #[test]
    fn vorlagen_ergeben_nur_code_fuer_ihre_fassungen() {
        let ir = ir("FUNKTIAUN erstes VON T(liste LISTN VON T) GIBT ZRUCK T\n GIB ZRUCK liste[0] OIDA\nPASST SCHO\n\
             KLASSE Stopl VON T\n drin LISTN VON T = []\n FUNKTIAUN wievü() GIBT ZRUCK ZOI\n  GIB ZRUCK LÄNGN(SÖWA.drin) OIDA\n PASST SCHO\nPASST SCHO\n\
             SCHREIB(erstes([1]), erstes([\"a\"]), Stopl VON ZOI().wievü()) OIDA");
        assert!(ir.contains("define internal i64 @\"oida.f.erstes\\20VON\\20ZOI\"(ptr %p0)"), "{ir}");
        assert!(ir.contains("define internal ptr @\"oida.f.erstes\\20VON\\20KETTN\"(ptr %p0)"), "{ir}");
        assert!(!ir.contains("@\"oida.f.erstes\""), "{ir}");
        assert!(ir.contains("define internal i64 @\"oida.m.Stopl\\20VON\\20ZOI.wiev\\C3\\BC\"(ptr %selbst)"), "{ir}");
        // Für die Vorlage selbst (Klasse 0) gibt es weder einen Typ noch Angaben.
        assert!(!ir.contains("%k0 = type") && ir.contains("%k1 = type { i64, ptr, ptr }"), "{ir}");
        assert!(!ir.contains("@oida.klasse.0 ") && ir.contains("@oida.klasse.1 "), "{ir}");
    }

    #[test]
    fn verzeichnis_im_speicher() {
        let ir = ir("SOG AU v = VAZEICHNIS VON KETTN UND ZOI() OIDA\nv[\"a\"] = 1 OIDA\nSCHREIB(v[\"a\"] ODA HOID 0, LÄNGN(v)) OIDA\n\
             SOG AU w = VAZEICHNIS VON ZOI UND KETTN() OIDA\nw[1] = \"x\" OIDA");
        let haupt = haupt(&ir);
        // Texte als Schlüssel (1) und Zahlen als Schlüssel (0).
        assert!(haupt.contains("call ptr @oida_wb_neu(i64 1)") && haupt.contains("call ptr @oida_wb_neu(i64 0)"), "{haupt}");
        // Schreiben: erst suchen, dann ersetzen oder anhängen.
        assert_eq!(haupt.matches("call i64 @oida_wb_dazu(").count(), 2, "{haupt}");
        assert!(haupt.matches("call i64 @oida_wb_finde(").count() >= 3, "{haupt}");
        // Beim Ersetzen eines Texts wird der alte losgelassen, bei einer Zahl nicht.
        assert_eq!(haupt.matches("call void @oida.lass(").count(), 1, "{haupt}");
    }

    #[test]
    fn namen_mit_umlauten() {
        let ir = ir("KLASSE Bär\n FUNKTIAUN grüß()\n PASST SCHO\nPASST SCHO\nBär().grüß() OIDA");
        assert!(ir.contains("define internal void @\"oida.m.B\\C3\\A4r.gr\\C3\\BC\\C3\\9F\"(ptr %selbst)"), "{ir}");
    }
}
