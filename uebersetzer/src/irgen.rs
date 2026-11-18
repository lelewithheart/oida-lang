//! IR-Erzeugung: macht aus dem geprüften Syntaxbaum LLVM-IR als Text.
//!
//! Jede Variable bekommt einen Platz am Stack (`alloca`), gelesen und
//! geschrieben wird mit `load`/`store`. Das ist einfach zu erzeugen, und
//! LLVM macht daraus beim Optimieren von selbst Register.
//!
//! Die Funktionen mit `oida_` am Anfang kommen aus der Laufzeitbibliothek
//! (`laufzeit/src/lib.rs`) und müssen zu den Deklarationen in [`KOPF`] passen.

use std::collections::HashMap;
use std::fmt::Write;

use crate::ast::*;
use crate::woerterbuch::{Schluessel, Woerterbuch};

/// Für dieses Ziel wird Code erzeugt (Windows, 64 Bit, llvm-mingw).
pub const ZIEL: &str = "x86_64-w64-windows-gnu";

const KOPF: &str = r#"; Erzeugt vom OIDA-Compiler. Nicht von Hand ändern.
target triple = "x86_64-w64-windows-gnu"

declare void @oida_start()
declare void @oida_ende()
declare void @oida_schreib_zoi(i64)
declare void @oida_schreib_kommazoi(double)
declare void @oida_schreib_text(ptr)
declare void @oida_schreib_leer()
declare void @oida_schreib_zeilenende()
declare ptr @oida_text_verbinden(ptr, ptr)
declare i32 @oida_text_vergleich(ptr, ptr)
declare ptr @oida_zoi_zu_text(i64)
declare ptr @oida_kommazoi_zu_text(double)
declare i64 @oida_text_zu_zoi(ptr, i64)
declare double @oida_text_zu_kommazoi(ptr, i64)
declare void @oida_fehler_durch_null(i64) noreturn
declare i64 @llvm.fptosi.sat.i64.f64(double)

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

"#;

fn ir_typ(typ: Typ) -> &'static str {
    match typ {
        Typ::Zoi => "i64",
        Typ::Kommazoi => "double",
        Typ::Jona => "i1",
        Typ::Kettn => "ptr",
        Typ::Nix => "void",
        Typ::Fehler => unreachable!("nach der Typprüfung gibt es keine Fehler-Typen mehr"),
    }
}

/// Der Name einer OIDA-Funktion im IR. Der Vorsatz verhindert, dass sich
/// eine OIDA-Funktion mit einer aus C (`main`, `printf` ...) verwechselt.
fn funktions_name(name: &str) -> String {
    let mut aus = String::from("@\"oida.f.");
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b == b'_' {
            aus.push(b as char);
        } else {
            let _ = write!(aus, "\\{b:02X}");
        }
    }
    aus.push('"');
    aus
}

struct Modul {
    /// Feste Texte aus dem Programm, jeder nur einmal.
    texte: Vec<String>,
    text_nummer: HashMap<String, usize>,
    woahr: String,
    foisch: String,
}

impl Modul {
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
}

/// Erzeugt den Code für eine Funktion (oder das Hauptprogramm).
struct FunktionsBau<'m> {
    modul: &'m mut Modul,
    /// Die `alloca`-Zeilen; sie kommen alle ganz an den Anfang der Funktion.
    plaetze: String,
    code: String,
    naechste_nummer: usize,
    /// Der Block, in den gerade geschrieben wird (für `phi`).
    block: String,
}

impl FunktionsBau<'_> {
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

    fn platz(&mut self, id: usize, typ: Typ) {
        let _ = writeln!(self.plaetze, "  %v{id} = alloca {}", ir_typ(typ));
    }

    // ------------------------------------------------------- Anweisungen

    fn anweisungen(&mut self, anweisungen: &[Anweisung]) {
        for a in anweisungen {
            self.anweisung(a);
        }
    }

    fn anweisung(&mut self, anweisung: &Anweisung) {
        match &anweisung.art {
            AnweisungArt::Deklaration { wert, id, .. } => {
                let w = self.ausdruck(wert);
                self.platz(*id, wert.typ);
                self.zeile(&format!("store {} {w}, ptr %v{id}", ir_typ(wert.typ)));
            }
            AnweisungArt::Zuweisung { wert, id, .. } => {
                let w = self.ausdruck(wert);
                self.zeile(&format!("store {} {w}, ptr %v{id}", ir_typ(wert.typ)));
            }
            AnweisungArt::Wenn { bedingung, dann, sunst } => {
                let b = self.ausdruck(bedingung);
                let m_dann = self.marke("dann");
                let m_ende = self.marke("wenn_ende");
                let m_sunst = if sunst.is_empty() { m_ende.clone() } else { self.marke("sunst") };
                self.zeile(&format!("br i1 {b}, label %{m_dann}, label %{m_sunst}"));
                self.beginne_block(&m_dann);
                self.anweisungen(dann);
                self.zeile(&format!("br label %{m_ende}"));
                if !sunst.is_empty() {
                    self.beginne_block(&m_sunst);
                    self.anweisungen(sunst);
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
                self.anweisungen(rumpf);
                self.zeile(&format!("br label %{m_kopf}"));
                self.beginne_block(&m_ende);
            }
            AnweisungArt::GibZruck { wert } => {
                match wert {
                    Some(wert) => {
                        let w = self.ausdruck(wert);
                        self.zeile(&format!("ret {} {w}", ir_typ(wert.typ)));
                    }
                    None => self.zeile("ret void"),
                }
                // Was danach noch kommt, wird nie erreicht, braucht aber einen eigenen Block.
                let m = self.marke("nach_zruck");
                self.beginne_block(&m);
            }
            AnweisungArt::Schreib { argumente } => {
                for (i, argument) in argumente.iter().enumerate() {
                    let w = self.ausdruck(argument);
                    if i > 0 {
                        self.zeile("call void @oida_schreib_leer()");
                    }
                    match argument.typ {
                        Typ::Zoi => self.zeile(&format!("call void @oida_schreib_zoi(i64 {w})")),
                        Typ::Kommazoi => self.zeile(&format!("call void @oida_schreib_kommazoi(double {w})")),
                        Typ::Kettn => self.zeile(&format!("call void @oida_schreib_text(ptr {w})")),
                        Typ::Jona => {
                            let text = self.jona_als_text(&w);
                            self.zeile(&format!("call void @oida_schreib_text(ptr {text})"));
                        }
                        Typ::Nix | Typ::Fehler => unreachable!("von der Typprüfung abgelehnt"),
                    }
                }
                self.zeile("call void @oida_schreib_zeilenende()");
            }
            AnweisungArt::Ausdruck(ausdruck) => {
                self.ausdruck(ausdruck);
            }
        }
    }

    // --------------------------------------------------------- Ausdrücke

    fn jona_als_text(&mut self, wert: &str) -> String {
        let woahr = self.modul.woahr.clone();
        let foisch = self.modul.foisch.clone();
        let (woahr, foisch) = (self.modul.text(&woahr), self.modul.text(&foisch));
        let t = self.temp();
        self.zeile(&format!("{t} = select i1 {wert}, ptr {woahr}, ptr {foisch}"));
        t
    }

    /// Erzeugt den Code für einen Ausdruck und gibt zurück, wo der Wert steht
    /// (ein Register wie `%t3` oder gleich eine Konstante).
    fn ausdruck(&mut self, ausdruck: &Ausdruck) -> String {
        match &ausdruck.art {
            AusdruckArt::Zoi(wert) => wert.to_string(),
            // Als Bitmuster, damit die Zahl genau so ankommt.
            AusdruckArt::Kommazoi(wert) => format!("0x{:016X}", wert.to_bits()),
            AusdruckArt::Jona(wert) => wert.to_string(),
            AusdruckArt::Text(inhalt) => self.modul.text(inhalt),
            AusdruckArt::Variable { id, .. } => {
                let t = self.temp();
                self.zeile(&format!("{t} = load {}, ptr %v{id}", ir_typ(ausdruck.typ)));
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
            AusdruckArt::Binaer { op, links, rechts } => {
                let l = self.ausdruck(links);
                let r = self.ausdruck(rechts);
                self.binaer(*op, links.typ, &l, &r, ausdruck.pos.zeile)
            }
            AusdruckArt::Aufruf { name, argumente } => {
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
            AusdruckArt::Umwandlung { ziel, innen } => {
                let w = self.ausdruck(innen);
                self.umwandlung(innen.typ, *ziel, w, ausdruck.pos.zeile)
            }
        }
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

    /// `typ` ist der Typ der beiden Seiten (nach der Typprüfung sind sie gleich).
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
            (Typ::Kettn, BinaerOp::Plus) => format!("call ptr @oida_text_verbinden(ptr {l}, ptr {r})"),
            (Typ::Zoi, _) => format!("icmp {} i64 {l}, {r}", vergleich_ganz(op)),
            (Typ::Jona, _) => format!("icmp {} i1 {l}, {r}", vergleich_ganz(op)),
            (Typ::Kommazoi, _) => format!("fcmp {} double {l}, {r}", vergleich_komma(op)),
            (Typ::Kettn, _) => {
                let v = self.temp();
                self.zeile(&format!("{v} = call i32 @oida_text_vergleich(ptr {l}, ptr {r})"));
                format!("icmp {} i32 {v}, 0", vergleich_ganz(op))
            }
            (Typ::Nix | Typ::Fehler, _) => unreachable!("von der Typprüfung abgelehnt"),
        };
        self.zeile(&format!("{t} = {befehl}"));
        t
    }

    fn umwandlung(&mut self, von: Typ, ziel: Typ, w: String, zeile: u32) -> String {
        if von == ziel {
            return w;
        }
        if (von, ziel) == (Typ::Jona, Typ::Kettn) {
            return self.jona_als_text(&w);
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

/// Ein fester Text als IR-Konstante: Zähler (-1 = nie freigeben), Länge, Bytes.
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

/// Erzeugt das LLVM-IR für ein Programm, das die Typprüfung bestanden hat.
pub fn erzeugen(programm: &Programm, wb: &Woerterbuch) -> String {
    let mut modul = Modul {
        texte: Vec::new(),
        text_nummer: HashMap::new(),
        woahr: wb.wort(Schluessel::True).to_string(),
        foisch: wb.wort(Schluessel::False).to_string(),
    };
    let mut funktionen = String::new();

    for f in &programm.funktionen {
        let mut bau = FunktionsBau {
            modul: &mut modul,
            plaetze: String::new(),
            code: String::new(),
            naechste_nummer: 0,
            block: "start".to_string(),
        };
        let mut kopf = Vec::new();
        for (i, p) in f.parameter.iter().enumerate() {
            kopf.push(format!("{} %p{i}", ir_typ(p.typ)));
            bau.platz(p.id, p.typ);
            bau.zeile(&format!("store {} %p{i}, ptr %v{}", ir_typ(p.typ), p.id));
        }
        bau.anweisungen(&f.rumpf);
        // Die Typprüfung stellt sicher, dass man hier nur ankommt, wenn nichts zurückzugeben ist.
        bau.zeile(if f.rueckgabe == Typ::Nix { "ret void" } else { "unreachable" });
        let _ = write!(
            funktionen,
            "define internal {} {}({}) {{\nstart:\n{}{}}}\n\n",
            ir_typ(f.rueckgabe),
            funktions_name(&f.name),
            kopf.join(", "),
            bau.plaetze,
            bau.code
        );
    }

    let mut bau = FunktionsBau {
        modul: &mut modul,
        plaetze: String::new(),
        code: String::new(),
        naechste_nummer: 0,
        block: "start".to_string(),
    };
    bau.zeile("call void @oida_start()");
    bau.anweisungen(&programm.haupt);
    bau.zeile("call void @oida_ende()");
    bau.zeile("ret i32 0");
    let _ = write!(funktionen, "define i32 @main() {{\nstart:\n{}{}}}\n", bau.plaetze, bau.code);

    let mut aus = String::from(KOPF);
    for (nummer, inhalt) in modul.texte.iter().enumerate() {
        aus.push_str(&text_konstante(nummer, inhalt));
    }
    aus.push('\n');
    aus.push_str(&funktionen);
    aus
}
