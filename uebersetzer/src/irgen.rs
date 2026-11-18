//! IR-Erzeugung: macht aus dem geprüften Syntaxbaum LLVM-IR als Text.
//!
//! Jede Variable bekommt einen Platz am Stack (`alloca`), gelesen und
//! geschrieben wird mit `load`/`store`. Das ist einfach zu erzeugen, und
//! LLVM macht daraus beim Optimieren von selbst Register.
//!
//! Die Funktionen mit `oida_` am Anfang kommen aus der Laufzeitbibliothek
//! (`laufzeit/src/lib.rs`) und müssen zu den Deklarationen in [`KOPF`] passen.
//!
//! # Texte und Referenzzählung
//!
//! Texte liegen am Heap und tragen einen Zähler: wie viele Stellen den Text
//! gerade brauchen. Fällt er auf 0, wird der Text freigegeben. Die Regeln:
//!
//!  - Ein Ausdruck liefert einen Text, der dem Empfänger **gehört** (Zähler
//!    schon mitgezählt): [`FunktionsBau::ausdruck`].
//!  - Wer einen Text nur liest (vergleichen, zusammenhängen, ausgeben), kann
//!    ihn **leihen**: [`FunktionsBau::geliehen`]. Eine Variable oder ein fester
//!    Text wird dann gar nicht gezählt, das spart die meiste Arbeit.
//!  - Eine Variable besitzt ihren Text. Beim Zuweisen wird der alte
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

declare void @oida_start()
declare void @oida_ende()
declare void @oida_schreib_zoi(i64)
declare void @oida_schreib_kommazoi(double)
declare void @oida_schreib_text(ptr)
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
declare void @oida_fehler_durch_null(i64) noreturn
declare void @oida_fehler_schritt_null(i64) noreturn
declare i64 @llvm.fptosi.sat.i64.f64(double)

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

; Referenzzählung für Texte. Der Zähler steht ganz vorne im Text.
; Negativ heißt: fester Text aus dem Programm, nicht zählen.
define internal void @oida.behalt(ptr %text) alwaysinline {
start:
  %z = load i64, ptr %text
  %fest = icmp slt i64 %z, 0
  br i1 %fest, label %ende, label %zaehlen
zaehlen:
  %mehr = add i64 %z, 1
  store i64 %mehr, ptr %text
  br label %ende
ende:
  ret void
}

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
    /// Die Text-Variablen (ihre Nummern) je offenem Block, der innerste hinten.
    /// Am Ende eines Blocks und bei GIB ZRUCK werden ihre Texte losgelassen.
    text_variablen: Vec<Vec<usize>>,
    /// Die Schleifen, in denen wir gerade stecken, die innerste hinten.
    schleifen: Vec<Schleife>,
}

/// Was HAU OB und WEIDA über ihre Schleife wissen müssen.
#[derive(Clone)]
struct Schleife {
    /// Hierhin springt WEIDA.
    weiter: String,
    /// Hierhin springt HAU OB.
    ende: String,
    /// Wie viele Blöcke offen waren, als der Rumpf begonnen hat. Die Texte
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

impl<'m> FunktionsBau<'m> {
    fn neu(modul: &'m mut Modul) -> FunktionsBau<'m> {
        FunktionsBau {
            modul,
            plaetze: String::new(),
            code: String::new(),
            naechste_nummer: 0,
            block: "start".to_string(),
            text_variablen: vec![Vec::new()],
            schleifen: Vec::new(),
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

    /// Legt den Platz für eine Variable an. Text-Variablen werden gemerkt,
    /// damit ihr Text am Ende des Blocks losgelassen wird.
    fn platz(&mut self, id: usize, typ: Typ) {
        let _ = writeln!(self.plaetze, "  %v{id} = alloca {}", ir_typ(typ));
        if typ == Typ::Kettn {
            self.text_variablen.last_mut().expect("es gibt immer einen Block").push(id);
        }
    }

    // ----------------------------------------------------- Texte loslassen

    /// Der Text wird hier nicht mehr gebraucht.
    fn lass(&mut self, text: &str) {
        self.zeile(&format!("call void @oida.lass(ptr {text})"));
    }

    /// Lässt den Text los, wenn er uns gehört (siehe [`Self::geliehen`]).
    fn lass_wenn_eigen(&mut self, text: &str, eigen: bool) {
        if eigen {
            self.lass(text);
        }
    }

    /// Lässt die Texte der Variablen los, die in den innersten `bloecke`
    /// Blöcken angesagt wurden.
    fn lass_variablen(&mut self, bloecke: usize) {
        let anfang = self.text_variablen.len() - bloecke;
        let nummern: Vec<usize> = self.text_variablen[anfang..].iter().flatten().copied().collect();
        // In umgekehrter Reihenfolge, wie beim Aufräumen üblich.
        for id in nummern.into_iter().rev() {
            let t = self.temp();
            self.zeile(&format!("{t} = load ptr, ptr %v{id}"));
            self.lass(&t);
        }
    }

    // ------------------------------------------------------- Anweisungen

    /// Die Anweisungen eines eigenen Blocks (WENN, SOLONG): Variablen, die
    /// darin angesagt werden, gelten nur bis zu seinem Ende.
    fn block(&mut self, anweisungen: &[Anweisung]) {
        self.text_variablen.push(Vec::new());
        for a in anweisungen {
            self.anweisung(a);
        }
        self.lass_variablen(1);
        self.text_variablen.pop();
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
                if wert.typ == Typ::Kettn {
                    // Der alte Text wird nicht mehr gebraucht. Erst nach dem
                    // Speichern loslassen, falls es derselbe ist (s = s).
                    let alt = self.temp();
                    self.zeile(&format!("{alt} = load ptr, ptr %v{id}"));
                    self.zeile(&format!("store ptr {w}, ptr %v{id}"));
                    self.lass(&alt);
                } else {
                    self.zeile(&format!("store {} {w}, ptr %v{id}", ir_typ(wert.typ)));
                }
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
                    bloecke: self.text_variablen.len(),
                });
                self.block(rumpf);
                self.schleifen.pop();
                self.zeile(&format!("br label %{m_kopf}"));
                self.beginne_block(&m_ende);
            }
            AnweisungArt::Fia { id, von, bis, schritt, rumpf, .. } => {
                let zeile = anweisung.pos.zeile;
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
                    bloecke: self.text_variablen.len(),
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
            AnweisungArt::HauOb | AnweisungArt::Weida => {
                let schleife = self.schleifen.last().expect("die Typprüfung lässt das nur in Schleifen zu").clone();
                // Die Texte aus allen Blöcken, die dabei verlassen werden.
                self.lass_variablen(self.text_variablen.len() - schleife.bloecke);
                let ziel = if matches!(anweisung.art, AnweisungArt::HauOb) { schleife.ende } else { schleife.weiter };
                self.zeile(&format!("br label %{ziel}"));
                // Was danach noch kommt, wird nie erreicht, braucht aber einen eigenen Block.
                let m = self.marke("nach_sprung");
                self.beginne_block(&m);
            }
            AnweisungArt::GibZruck { wert } => {
                // Erst den Wert ausrechnen (er gehört dann dem Aufrufer),
                // dann alle Text-Variablen der Funktion loslassen.
                let w = wert.as_ref().map(|wert| (self.ausdruck(wert), wert.typ));
                self.lass_variablen(self.text_variablen.len());
                match w {
                    Some((w, typ)) => self.zeile(&format!("ret {} {w}", ir_typ(typ))),
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
                    match argument.typ {
                        Typ::Zoi => self.zeile(&format!("call void @oida_schreib_zoi(i64 {w})")),
                        Typ::Kommazoi => self.zeile(&format!("call void @oida_schreib_kommazoi(double {w})")),
                        Typ::Kettn => {
                            self.zeile(&format!("call void @oida_schreib_text(ptr {w})"));
                            self.lass_wenn_eigen(&w, eigen);
                        }
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
                let w = self.ausdruck(ausdruck);
                if ausdruck.typ == Typ::Kettn {
                    // Das Ergebnis will niemand haben.
                    self.lass(&w);
                }
            }
        }
    }

    // --------------------------------------------------------- Ausdrücke

    /// WOAHR oder FOISCH als Text. Das sind feste Texte, die niemand zählt.
    fn jona_als_text(&mut self, wert: &str) -> String {
        let woahr = self.modul.woahr.clone();
        let foisch = self.modul.foisch.clone();
        let (woahr, foisch) = (self.modul.text(&woahr), self.modul.text(&foisch));
        let t = self.temp();
        self.zeile(&format!("{t} = select i1 {wert}, ptr {woahr}, ptr {foisch}"));
        t
    }

    /// Wie [`Self::ausdruck`], aber für Stellen, die einen Text nur lesen.
    /// Gibt dazu zurück, ob der Text uns gehört und nach dem Lesen
    /// losgelassen werden muss. Eine Variable und ein fester Text werden nur
    /// geliehen: Solange gelesen wird, kann sie niemand ändern.
    fn geliehen(&mut self, ausdruck: &Ausdruck) -> (String, bool) {
        if ausdruck.typ != Typ::Kettn {
            return (self.ausdruck(ausdruck), false);
        }
        match &ausdruck.art {
            AusdruckArt::Text(inhalt) => (self.modul.text(inhalt), false),
            AusdruckArt::Variable { id, .. } => {
                let t = self.temp();
                self.zeile(&format!("{t} = load ptr, ptr %v{id}"));
                (t, false)
            }
            _ => (self.ausdruck(ausdruck), true),
        }
    }

    /// Erzeugt den Code für einen Ausdruck und gibt zurück, wo der Wert steht
    /// (ein Register wie `%t3` oder gleich eine Konstante).
    /// Ein Text gehört danach dem Empfänger.
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
                if ausdruck.typ == Typ::Kettn {
                    // Jetzt braucht ihn außer der Variable noch jemand.
                    self.zeile(&format!("call void @oida.behalt(ptr {t})"));
                }
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
                self.lass_wenn_eigen(&l, l_eigen);
                self.lass_wenn_eigen(&r, r_eigen);
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
                if innen.typ == *ziel {
                    return self.ausdruck(innen);
                }
                // Aus einem Text wird beim Umwandeln nur gelesen.
                let (w, eigen) = self.geliehen(innen);
                let t = self.umwandlung(innen.typ, *ziel, &w, ausdruck.pos.zeile);
                self.lass_wenn_eigen(&w, eigen);
                t
            }
        }
    }

    /// Aufruf einer eingebauten Funktion. Texte werden dabei nur gelesen;
    /// kommt ein Text heraus, ist er neu und gehört dem Empfänger.
    fn eingebaut(&mut self, funktion: Eingebaut, argumente: &[Ausdruck], ergebnis: Typ, zeile: u32) -> String {
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
            if argument.typ == Typ::Kettn {
                self.lass_wenn_eigen(wert, *eigen);
            }
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
            let liste = self.temp();
            let _ = writeln!(self.plaetze, "  {liste} = alloca [{n} x ptr]");
            for (i, (wert, _)) in werte.iter().enumerate() {
                let platz = self.temp();
                self.zeile(&format!("{platz} = getelementptr inbounds [{n} x ptr], ptr {liste}, i64 0, i64 {i}"));
                self.zeile(&format!("store ptr {wert}, ptr {platz}"));
            }
            self.zeile(&format!("{t} = call ptr @oida_text_verbinden_viele(ptr {liste}, i64 {n})"));
        }
        for (wert, eigen) in &werte {
            self.lass_wenn_eigen(wert, *eigen);
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
        let mut bau = FunktionsBau::neu(&mut modul);
        let mut kopf = Vec::new();
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
        } else {
            // Die Typprüfung stellt sicher, dass jeder Weg mit GIB ZRUCK aufhört.
            bau.zeile("unreachable");
        }
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

    let mut bau = FunktionsBau::neu(&mut modul);
    bau.zeile("call void @oida_start()");
    for a in &programm.haupt {
        bau.anweisung(a);
    }
    bau.lass_variablen(1);
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
