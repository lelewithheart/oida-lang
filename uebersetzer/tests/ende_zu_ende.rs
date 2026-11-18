//! Tests von Anfang bis Ende: `.oida`-Datei -> `oida` -> `.exe` -> Ausgabe.
//!
//! Diese Tests brauchen `clang` aus llvm-mingw (siehe README).
//!
//! - `beispiele/*.oida` und `tests/programme/*.oida`: müssen sich übersetzen
//!   lassen und genau das ausgeben, was in der `.erwartet`-Datei daneben steht.
//! - `tests/fehler/*.oida`: dürfen sich NICHT übersetzen lassen; jede Zeile
//!   der `.erwartet`-Datei muss in der Fehlermeldung vorkommen.
//! - `tests/laufzeitfehler/*.oida`: lassen sich übersetzen, brechen aber beim
//!   Laufen ab; `.erwartet` ist die Ausgabe davor, `.fehler` die Meldung.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn oida() -> Command {
    Command::new(env!("CARGO_BIN_EXE_oida"))
}

/// Der Projektordner (dort liegen `beispiele/` und `woerterbuch.txt`).
fn wurzel() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn tests() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

/// Ein leerer Ordner für die Dateien eines Tests.
fn arbeitsordner(name: &str) -> PathBuf {
    let ordner = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&ordner);
    fs::create_dir_all(&ordner).unwrap();
    ordner
}

fn oida_dateien(ordner: &Path) -> Vec<PathBuf> {
    let mut dateien: Vec<PathBuf> = fs::read_dir(ordner)
        .unwrap_or_else(|e| panic!("{}: {e}", ordner.display()))
        .map(|eintrag| eintrag.unwrap().path())
        .filter(|pfad| pfad.extension().is_some_and(|e| e == "oida"))
        .collect();
    dateien.sort();
    assert!(!dateien.is_empty(), "keine .oida-Dateien in {}", ordner.display());
    dateien
}

fn name(datei: &Path) -> String {
    datei.file_stem().unwrap().to_string_lossy().into_owned()
}

/// Ausgabe als Text, Zeilenenden vereinheitlicht.
fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("Ausgabe ist kein UTF-8").replace("\r\n", "\n")
}

fn lies(datei: &Path) -> String {
    fs::read_to_string(datei).unwrap_or_else(|e| panic!("{}: {e}", datei.display())).replace("\r\n", "\n")
}

/// Übersetzt eine Datei und prüft, dass das geklappt hat.
fn uebersetze(quelle: &Path, exe: &Path, optionen: &[&str]) {
    let ergebnis = oida().arg(quelle).arg("-o").arg(exe).args(optionen).output().unwrap();
    assert!(
        ergebnis.status.success(),
        "{} lässt sich nicht übersetzen:\n{}",
        quelle.display(),
        text(&ergebnis.stderr)
    );
    assert!(exe.is_file(), "{} wurde nicht erzeugt", exe.display());
}

fn starte(exe: &Path) -> Output {
    Command::new(exe).output().unwrap_or_else(|e| panic!("{}: {e}", exe.display()))
}

/// Übersetzt alle Programme in einem Ordner (mit und ohne Optimierung)
/// und vergleicht die Ausgabe mit der `.erwartet`-Datei.
fn pruefe_programme(ordner: &Path, arbeitsname: &str) {
    let arbeit = arbeitsordner(arbeitsname);
    for quelle in oida_dateien(ordner) {
        let erwartet_datei = quelle.with_extension("erwartet");
        assert!(erwartet_datei.is_file(), "zu {} fehlt die .erwartet-Datei", quelle.display());
        let erwartet = lies(&erwartet_datei);
        for (art, optionen) in [("optimiert", &[][..]), ("ohne-optimierung", &["--ohne-optimierung"][..])] {
            let exe = arbeit.join(format!("{}-{art}.exe", name(&quelle)));
            uebersetze(&quelle, &exe, optionen);
            let lauf = starte(&exe);
            assert!(lauf.status.success(), "{} ({art}) ist abgestürzt:\n{}", quelle.display(), text(&lauf.stderr));
            assert_eq!(text(&lauf.stdout), erwartet, "falsche Ausgabe von {} ({art})", quelle.display());
            assert_eq!(text(&lauf.stderr), "", "{} ({art}) schreibt Fehler", quelle.display());
        }
    }
}

#[test]
fn beispiele_geben_das_richtige_aus() {
    pruefe_programme(&wurzel().join("beispiele"), "beispiele");
}

#[test]
fn testprogramme_geben_das_richtige_aus() {
    pruefe_programme(&tests().join("programme"), "programme");
}

/// Im Prüfmodus der Laufzeit (`OIDA_TEXTE_PRUEFEN`) wird am Ende gemeldet,
/// wie viele Texte nicht freigegeben wurden, und das Programm bricht ab,
/// wenn ein Text nach dem Freigeben noch verwendet wird. So prüft sich die
/// Referenzzählung des Compilers an allen Programmen selbst.
#[test]
fn texte_werden_alle_freigegeben() {
    let arbeit = arbeitsordner("texte-pruefen");
    let mut quellen = oida_dateien(&wurzel().join("beispiele"));
    quellen.extend(oida_dateien(&tests().join("programme")));
    for quelle in quellen {
        let erwartet = lies(&quelle.with_extension("erwartet"));
        for (art, optionen) in [("optimiert", &[][..]), ("ohne-optimierung", &["--ohne-optimierung"][..])] {
            let exe = arbeit.join(format!("{}-{art}.exe", name(&quelle)));
            uebersetze(&quelle, &exe, optionen);
            let lauf = Command::new(&exe).env("OIDA_TEXTE_PRUEFEN", "1").output().unwrap();
            assert!(lauf.status.success(), "{} ({art}):\n{}", quelle.display(), text(&lauf.stderr));
            assert_eq!(text(&lauf.stdout), erwartet, "{} ({art})", quelle.display());
            assert_eq!(text(&lauf.stderr), "OIDA: 0 Texte übrig\n", "{} ({art})", quelle.display());
        }
    }
}

#[test]
fn fehler_im_programm_werden_gemeldet() {
    let arbeit = arbeitsordner("fehler");
    for quelle in oida_dateien(&tests().join("fehler")) {
        let exe = arbeit.join(format!("{}.exe", name(&quelle)));
        let ergebnis = oida().arg(&quelle).arg("-o").arg(&exe).output().unwrap();
        let meldung = text(&ergebnis.stderr);
        assert_eq!(ergebnis.status.code(), Some(1), "{}:\n{meldung}", quelle.display());
        assert!(!exe.exists(), "{}: trotz Fehler wurde eine .exe erzeugt", quelle.display());
        let datei = quelle.display().to_string();
        for zeile in lies(&quelle.with_extension("erwartet")).lines().filter(|z| !z.is_empty()) {
            // Zeilen mit ":zeile:spalte:" vorne müssen direkt hinter dem Dateinamen stehen.
            let gesucht = if zeile.starts_with(':') { format!("{datei}{zeile}") } else { zeile.to_string() };
            assert!(meldung.contains(&gesucht), "{}: erwartet war\n  {gesucht}\nbekommen:\n{meldung}", quelle.display());
        }
    }
}

#[test]
fn fehlermeldung_zeigt_die_stelle() {
    let quelle = tests().join("fehler").join("fehlendes_oida.oida");
    let ergebnis = oida().arg(&quelle).arg("-o").arg(arbeitsordner("stelle").join("x.exe")).output().unwrap();
    let meldung = text(&ergebnis.stderr);
    let erwartet = " 3 | SOG AU name = \"Franz\"\n   |                      ^\n";
    assert!(meldung.contains(erwartet), "{meldung}");
}

#[test]
fn fehler_beim_laufen_werden_gemeldet() {
    let arbeit = arbeitsordner("laufzeitfehler");
    for quelle in oida_dateien(&tests().join("laufzeitfehler")) {
        for (art, optionen) in [("optimiert", &[][..]), ("ohne-optimierung", &["--ohne-optimierung"][..])] {
            let exe = arbeit.join(format!("{}-{art}.exe", name(&quelle)));
            uebersetze(&quelle, &exe, optionen);
            let lauf = starte(&exe);
            assert_eq!(lauf.status.code(), Some(1), "{} ({art})", quelle.display());
            // Was vor dem Fehler geschrieben wurde, darf nicht verloren gehen.
            assert_eq!(text(&lauf.stdout), lies(&quelle.with_extension("erwartet")), "{} ({art})", quelle.display());
            assert_eq!(text(&lauf.stderr), lies(&quelle.with_extension("fehler")), "{} ({art})", quelle.display());
        }
    }
}

#[test]
fn eigenes_woerterbuch() {
    let arbeit = arbeitsordner("woerterbuch");
    let woerterbuch = arbeit.join("meins.txt");
    fs::write(&woerterbuch, "# eigene Wörter\nIF = FOIS\nOIDA = HAWARA\nPRINT = ZAG\n").unwrap();

    let quelle = arbeit.join("eigen.oida");
    fs::write(&quelle, "DECLARE x = 3 HAWARA\nFOIS x > 2 THEN ZAG(\"jo\", TRUE) HAWARA END\n").unwrap();
    let exe = arbeit.join("eigen.exe");
    uebersetze(&quelle, &exe, &["--woerterbuch", woerterbuch.to_str().unwrap()]);
    assert_eq!(text(&starte(&exe).stdout), "jo TRUE\n");

    // Die Meldungen verwenden die eigenen Wörter.
    fs::write(&quelle, "ZAG(1)\n").unwrap();
    let ergebnis =
        oida().arg(&quelle).arg("--woerterbuch").arg(&woerterbuch).arg("-o").arg(&exe).output().unwrap();
    assert!(text(&ergebnis.stderr).contains(":1:7: Heast, wo is des HAWARA?"), "{}", text(&ergebnis.stderr));

    // Mit den eigenen Wörtern gelten die alten nicht mehr.
    fs::write(&quelle, "ZAG(1) OIDA\n").unwrap();
    let ergebnis =
        oida().arg(&quelle).arg("--woerterbuch").arg(&woerterbuch).arg("-o").arg(&exe).output().unwrap();
    assert_eq!(ergebnis.status.code(), Some(1));
}

#[test]
fn kaputtes_woerterbuch_wird_gemeldet() {
    let arbeit = arbeitsordner("woerterbuch-kaputt");
    let woerterbuch = arbeit.join("kaputt.txt");
    fs::write(&woerterbuch, "IF = WENN\nWHILE = WENN\n").unwrap();
    let quelle = arbeit.join("a.oida");
    fs::write(&quelle, "SCHREIB(1) OIDA\n").unwrap();
    let ergebnis = oida().arg(&quelle).arg("--woerterbuch").arg(&woerterbuch).output().unwrap();
    assert_eq!(ergebnis.status.code(), Some(2));
    let meldung = text(&ergebnis.stderr);
    assert!(meldung.contains("Zeile 2: 'WENN' steht schon bei IF"), "{meldung}");
}

#[test]
fn ohne_o_heisst_die_exe_wie_die_quelle_und_ir_wird_gespeichert() {
    let arbeit = arbeitsordner("optionen");
    let quelle = arbeit.join("servas.oida");
    fs::copy(wurzel().join("beispiele").join("hallo.oida"), &quelle).unwrap();

    let ergebnis = oida().arg(&quelle).arg("--ir").output().unwrap();
    assert!(ergebnis.status.success(), "{}", text(&ergebnis.stderr));
    assert!(text(&ergebnis.stdout).starts_with("Passt! "), "{}", text(&ergebnis.stdout));
    assert_eq!(text(&starte(&arbeit.join("servas.exe")).stdout), "Servas Wöd!\n");
    let ir = lies(&arbeit.join("servas.ll"));
    assert!(ir.contains("define i32 @main()"), "{ir}");
}

#[test]
fn renn_startet_das_programm_gleich() {
    let arbeit = arbeitsordner("renn");
    let exe = arbeit.join("fizzbuzz.exe");
    let ergebnis = oida()
        .arg(wurzel().join("beispiele").join("fizzbuzz.oida"))
        .arg("-o")
        .arg(&exe)
        .arg("--renn")
        .output()
        .unwrap();
    assert!(ergebnis.status.success(), "{}", text(&ergebnis.stderr));
    assert_eq!(text(&ergebnis.stdout), lies(&wurzel().join("beispiele").join("fizzbuzz.erwartet")));
}

#[test]
fn datei_die_es_nicht_gibt() {
    let ergebnis = oida().arg("gibts_ned.oida").output().unwrap();
    assert_eq!(ergebnis.status.code(), Some(2));
    assert!(text(&ergebnis.stderr).starts_with("Heast! I kaun gibts_ned.oida ned lesn"), "{}", text(&ergebnis.stderr));
}
