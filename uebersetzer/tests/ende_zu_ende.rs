//! Tests von Anfang bis Ende: `.oida`-Datei -> `oida` -> Programm -> Ausgabe.
//!
//! Diese Tests brauchen `clang` (unter Windows das aus llvm-mingw, siehe README).
//! Sie laufen unter Windows und unter Linux; was Fenster oder Ton braucht, gibt
//! es derzeit nur unter Windows und wird anderswo ausgelassen.
//!
//! - `beispiele/*.oida` und `tests/programme/*.oida`: müssen sich übersetzen
//!   lassen und genau das ausgeben, was in der `.erwartet`-Datei daneben steht.
//! - `tests/fehler/*.oida`: dürfen sich NICHT übersetzen lassen; jede Zeile
//!   der `.erwartet`-Datei muss in der Fehlermeldung vorkommen.
//! - `tests/laufzeitfehler/*.oida`: lassen sich übersetzen, brechen aber beim
//!   Laufen ab; `.erwartet` ist die Ausgabe davor, `.fehler` die Meldung.
//! - `tests/zoinpruefung/*.oida`: brechen nur ab, wenn mit `--mit-zoinprüfung`
//!   übersetzt wird (eine ganze Zahl wird zu groß); sonst laufen sie durch.
//!
//! Liegt neben einem Programm eine `.eingabe`-Datei, bekommt es deren Inhalt
//! als Eingabe (für `LIES`). Der Zufall startet in allen Tests mit demselben
//! Wert (`OIDA_SOMEN`), damit immer dasselbe herauskommt.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

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
        .filter(|pfad| geht_hier(pfad))
        .collect();
    dateien.sort();
    assert!(!dateien.is_empty(), "keine .oida-Dateien in {}", ordner.display());
    dateien
}

/// Fenster und Ton gibt es derzeit nur unter Windows: Programme, die sie
/// dazuholen, lassen die Tests anderswo aus.
fn geht_hier(quelle: &Path) -> bool {
    cfg!(windows) || !["BRAUCH fensta", "BRAUCH ton"].iter().any(|brauch| lies(quelle).contains(brauch))
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

/// Der Befehl, um ein übersetztes Programm zu starten. `quelle` ist die
/// `.oida`-Datei dazu; liegt daneben eine `.eingabe`-Datei, ist das die Eingabe.
fn befehl(exe: &Path, quelle: Option<&Path>) -> Command {
    let mut befehl = Command::new(exe);
    befehl.env("OIDA_SOMEN", "1");
    // Programme mit Tönen bleiben im Test stumm.
    befehl.env("OIDA_TON_AUS", "1");
    // Programme, die Dateien schreiben, tun das im Arbeitsordner des Tests, nicht im Projekt.
    // Auch der Temp-Ordner ist für sie der Arbeitsordner des Tests: Zwei Tests lassen dasselbe
    // Programm zur selben Zeit laufen, und im echten Temp-Ordner würden sie sich die Dateien
    // gegenseitig überschreiben und löschen.
    if let Some(ordner) = exe.parent() {
        befehl.current_dir(ordner);
        for name in ["TMPDIR", "TMP", "TEMP"] {
            befehl.env(name, ordner);
        }
    }
    match quelle.map(|q| q.with_extension("eingabe")).filter(|e| e.is_file()) {
        Some(eingabe) => befehl.stdin(fs::File::open(eingabe).unwrap()),
        None => befehl.stdin(Stdio::null()),
    };
    befehl
}

fn starte(exe: &Path) -> Output {
    befehl(exe, None).output().unwrap_or_else(|e| panic!("{}: {e}", exe.display()))
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
            let lauf = befehl(&exe, Some(&quelle)).output().unwrap();
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
            let lauf = befehl(&exe, Some(&quelle)).env("OIDA_TEXTE_PRUEFEN", "1").output().unwrap();
            assert!(lauf.status.success(), "{} ({art}):\n{}", quelle.display(), text(&lauf.stderr));
            assert_eq!(text(&lauf.stdout), erwartet, "{} ({art})", quelle.display());
            assert_eq!(text(&lauf.stderr), "OIDA: 0 Texte, Listen und Objekte übrig\n", "{} ({art})", quelle.display());
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
fn zu_grosse_zahlen_laufen_still_ueber_und_melden_sich_nur_mit_schalter() {
    let arbeit = arbeitsordner("zoinpruefung");
    for quelle in oida_dateien(&tests().join("zoinpruefung")) {
        // Mit dem Schalter: eine Meldung samt Zeile, und was davor geschrieben wurde, ist da.
        for schalter in ["--mit-zoinprüfung", "--mit-zoinpruefung"] {
            for (art, optionen) in [("optimiert", vec![schalter]), ("ohne-optimierung", vec![schalter, "--ohne-optimierung"])] {
                let exe = arbeit.join(format!("{}-{art}.exe", name(&quelle)));
                uebersetze(&quelle, &exe, &optionen);
                let lauf = befehl(&exe, Some(&quelle)).output().unwrap();
                assert_eq!(lauf.status.code(), Some(1), "{} ({art})", quelle.display());
                assert_eq!(text(&lauf.stdout), lies(&quelle.with_extension("erwartet")), "{} ({art})", quelle.display());
                assert_eq!(text(&lauf.stderr), lies(&quelle.with_extension("fehler")), "{} ({art})", quelle.display());
            }
        }
        // Ohne Schalter (die Vorgabe): Das Programm läuft durch und rechnet mit der übergelaufenen Zahl weiter.
        let exe = arbeit.join(format!("{}-ohne.exe", name(&quelle)));
        uebersetze(&quelle, &exe, &[]);
        let lauf = starte(&exe);
        assert_eq!(lauf.status.code(), Some(0), "{}: {}", quelle.display(), text(&lauf.stderr));
        assert!(text(&lauf.stdout).ends_with("nachher\n"), "{}: {}", quelle.display(), text(&lauf.stdout));
    }
    // Was dabei herauskommt: die größte Zahl plus 1 ist die kleinste.
    let exe = arbeit.join("plus.exe");
    uebersetze(&tests().join("zoinpruefung").join("ueberlauf_plus.oida"), &exe, &[]);
    assert_eq!(text(&starte(&exe).stdout), "vorher\n-9223372036854775808\nnachher\n");
    // Teilen durch 0 wird immer gemeldet, mit und ohne Schalter.
    for optionen in [&[][..], &["--mit-zoinprüfung"][..]] {
        let exe = arbeit.join("durch_null.exe");
        uebersetze(&tests().join("laufzeitfehler").join("durch_null.oida"), &exe, optionen);
        let lauf = starte(&exe);
        assert_eq!(lauf.status.code(), Some(1));
        assert_eq!(text(&lauf.stderr), "Heast! Zeile 3: Durch nix kaun ma ned teilen.\n");
    }
}

/// Ein `.tar.gz`, Byte für Byte selbst gebaut, so wie GitHub es ausliefert: Die Namen stehen als
/// rohes UTF-8 im Kopf jeder Datei (ohne Angabe der Zeichentabelle). Genau daran ist das
/// Auspacken unter Windows zuerst gescheitert: Aus `zöhn.oida` wurde `z├Âhn.oida`.
/// Gepackt wird nicht wirklich ("stored"): Das versteht jedes gzip.
fn tar_gz(dateien: &[(String, &str)]) -> Vec<u8> {
    let mut tar: Vec<u8> = Vec::new();
    for (name, inhalt) in dateien {
        let mut kopf = [0u8; 512];
        assert!(name.len() <= 100, "{name} ist für diesen einfachen Kopf zu lang");
        kopf[..name.len()].copy_from_slice(name.as_bytes());
        kopf[100..108].copy_from_slice(b"0000644\0");
        kopf[108..116].copy_from_slice(b"0000000\0");
        kopf[116..124].copy_from_slice(b"0000000\0");
        kopf[124..136].copy_from_slice(format!("{:011o}\0", inhalt.len()).as_bytes());
        kopf[136..148].copy_from_slice(b"00000000000\0");
        kopf[148..156].copy_from_slice(b"        "); // die Prüfsumme zählt hier als Leerzeichen
        kopf[156] = b'0';
        kopf[257..265].copy_from_slice(b"ustar\x0000");
        let summe: u32 = kopf.iter().map(|b| u32::from(*b)).sum();
        kopf[148..156].copy_from_slice(format!("{summe:06o}\0 ").as_bytes());
        tar.extend_from_slice(&kopf);
        tar.extend_from_slice(inhalt.as_bytes());
        tar.resize(tar.len().div_ceil(512) * 512, 0);
    }
    tar.resize(tar.len() + 1024, 0);

    let mut gz = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3];
    let stuecke: Vec<&[u8]> = tar.chunks(65535).collect();
    for (i, stueck) in stuecke.iter().enumerate() {
        gz.push(u8::from(i == stuecke.len() - 1));
        gz.extend_from_slice(&(stueck.len() as u16).to_le_bytes());
        gz.extend_from_slice(&(!(stueck.len() as u16)).to_le_bytes());
        gz.extend_from_slice(stueck);
    }
    let mut crc = 0xffff_ffffu32;
    for byte in &tar {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    gz.extend_from_slice(&(!crc).to_le_bytes());
    gz.extend_from_slice(&(tar.len() as u32).to_le_bytes());
    gz
}

/// Legt ein Paket so ab, wie GitHub es ausliefert: `<basis>/<wer>/<repo>/archive/<stand>.tar.gz`,
/// darin ein Ordner `<repo>-<stand>` mit den Dateien.
fn paket_ablegen(basis: &Path, wer: &str, repo: &str, stand: &str, dateien: &[(&str, &str)]) {
    let mit_ordner: Vec<(String, &str)> = dateien.iter().map(|(name, inhalt)| (format!("{repo}-{stand}/{name}"), *inhalt)).collect();
    let ziel = basis.join(wer).join(repo).join("archive");
    fs::create_dir_all(&ziel).unwrap();
    fs::write(ziel.join(format!("{stand}.tar.gz")), tar_gz(&mit_ordner)).unwrap();
}

#[test]
fn pakete_holen_und_brauchen() {
    let arbeit = arbeitsordner("pakete");
    let (basis, projekt) = (arbeit.join("netz"), arbeit.join("projekt"));
    fs::create_dir_all(&projekt).unwrap();
    // "Das Netz": zwei Pakete. wuerfel braucht selbst kortn (seine eigene pakete.txt).
    paket_ablegen(&basis, "franz", "kortn", "HEAD", &[("paket.oida", "FUNKTIAUN ziag() GIBT ZRUCK KETTN\n    GIB ZRUCK \"Herz Ass\" OIDA\nPASST SCHO\n")]);
    // Erst einmal ohne die Liste holen, um die Prüfsumme von kortn zu erfahren.
    let hol = |argumente: &[&str]| {
        let quelle = format!("file:///{}", basis.display().to_string().replace('\\', "/").trim_start_matches('/'));
        oida().arg("hol").args(argumente).current_dir(&projekt).env("OIDA_PAKET_QUELLE", quelle).output().unwrap()
    };
    let lauf = hol(&["franz/kortn"]);
    assert!(lauf.status.success(), "{}", text(&lauf.stderr));
    let liste = lies(&projekt.join("pakete.txt"));
    let zeile_kortn = liste.lines().find(|z| z.starts_with("kortn = ")).expect("kortn steht in der Liste").to_string();
    assert!(zeile_kortn.starts_with("kortn = franz/kortn sha256:") && zeile_kortn.len() == "kortn = franz/kortn sha256:".len() + 64, "{liste}");
    fs::remove_dir_all(projekt.join("pakete")).unwrap();
    fs::remove_file(projekt.join("pakete.txt")).unwrap();

    paket_ablegen(
        &basis,
        "franz",
        "wuerfel",
        "HEAD",
        &[
            ("wuerfel.oida", "BRAUCH \"teile/hüfe.oida\" OIDA\nBRAUCH \"kortn\" OIS k OIDA\nFUNKTIAUN wuaf() GIBT ZRUCK KETTN\n    GIB ZRUCK rahmen(\"6\") + \" \" + k.ziag() OIDA\nPASST SCHO\n"),
            ("teile/hüfe.oida", "FUNKTIAUN rahmen(t KETTN) GIBT ZRUCK KETTN\n    GIB ZRUCK \"[\" + t + \"]\" OIDA\nPASST SCHO\n"),
            ("pakete.txt", &format!("# was dieses Paket braucht\n{zeile_kortn}\n")),
        ],
    );

    // Holen: das Paket und was es selbst braucht.
    let lauf = hol(&["franz/wuerfel"]);
    assert!(lauf.status.success(), "{}", text(&lauf.stderr));
    assert!(text(&lauf.stdout).contains("Passt! 'wuerfel' liegt in pakete/wuerfel. Im Programm: BRAUCH \"wuerfel\" OIS wuerfel OIDA"), "{}", text(&lauf.stdout));
    // (Eine Datei mit Umlaut im Namen: Die muss auch unter Windows richtig ankommen.)
    for datei in ["pakete/wuerfel/wuerfel.oida", "pakete/wuerfel/teile/hüfe.oida", "pakete/kortn/paket.oida"] {
        assert!(projekt.join(datei).is_file(), "{datei} fehlt");
    }
    let liste = lies(&projekt.join("pakete.txt"));
    assert!(liste.contains(&zeile_kortn) && liste.contains("wuerfel = franz/wuerfel sha256:"), "{liste}");

    // Brauchen: nur der Name, mit OIS. Das Paket holt seine eigenen Dateien und sein eigenes Paket.
    fs::write(projekt.join("spui.oida"), "BRAUCH \"wuerfel\" OIS w OIDA\nBRAUCH \"kortn\" OIS kortn OIDA\nSCHREIB(w.wuaf(), kortn.ziag()) OIDA\n").unwrap();
    let exe = projekt.join("spui.exe");
    uebersetze(&projekt.join("spui.oida"), &exe, &[]);
    assert_eq!(text(&starte(&exe).stdout), "[6] Herz Ass Herz Ass\n");

    // Ein Paket, das es (noch) nicht gibt: Die Meldung sagt, wie man es holt.
    fs::write(projekt.join("fehlt.oida"), "BRAUCH \"gibtsned\" OIS g OIDA\n").unwrap();
    let ergebnis = oida().arg(projekt.join("fehlt.oida")).output().unwrap();
    assert!(text(&ergebnis.stderr).contains("und a ka Paket 'gibtsned' (in pakete/gibtsned). A Paket hoit ma mit: oida hol wer/gibtsned"), "{}", text(&ergebnis.stderr));

    // Alles wieder holen (wie auf einem anderen Rechner): Es kommt dasselbe.
    fs::remove_dir_all(projekt.join("pakete")).unwrap();
    let lauf = hol(&[]);
    assert!(lauf.status.success(), "{}", text(&lauf.stderr));
    assert!(projekt.join("pakete/wuerfel/wuerfel.oida").is_file() && projekt.join("pakete/kortn/paket.oida").is_file());
    assert_eq!(lies(&projekt.join("pakete.txt")), liste);

    // Jemand hat das Paket geändert: Ohne --neich wird es nicht genommen, und das alte bleibt liegen.
    paket_ablegen(&basis, "franz", "kortn", "HEAD", &[("paket.oida", "FUNKTIAUN ziag() GIBT ZRUCK KETTN\n    GIB ZRUCK \"Pik Bua\" OIDA\nPASST SCHO\n")]);
    let lauf = hol(&[]);
    assert_eq!(lauf.status.code(), Some(2));
    assert!(text(&lauf.stderr).contains("Des Paket 'kortn' is nimma desöwe wia beim letztn Moi"), "{}", text(&lauf.stderr));
    assert!(lies(&projekt.join("pakete/kortn/paket.oida")).contains("Herz Ass"));
    assert_eq!(lies(&projekt.join("pakete.txt")), liste);
    let lauf = hol(&["--neich"]);
    assert!(lauf.status.success(), "{}", text(&lauf.stderr));
    assert!(lies(&projekt.join("pakete/kortn/paket.oida")).contains("Pik Bua"));
    assert_ne!(lies(&projekt.join("pakete.txt")), liste);

    // Ein bestimmter Stand, unter einem anderen Namen.
    paket_ablegen(&basis, "franz", "kortn", "v1", &[("kortn.oida", "FUNKTIAUN ziag() GIBT ZRUCK KETTN\n    GIB ZRUCK \"oide Kortn\" OIDA\nPASST SCHO\n")]);
    let lauf = hol(&["franz/kortn@v1", "--ois", "oide"]);
    assert_eq!(lauf.status.code(), Some(2), "kortn.oida passt nicht zum Namen 'oide'");
    assert!(text(&lauf.stderr).contains("Do drin gibt's ka 'oide.oida' und ka 'paket.oida'"), "{}", text(&lauf.stderr));
    paket_ablegen(&basis, "franz", "kortn", "v1", &[("paket.oida", "FUNKTIAUN ziag() GIBT ZRUCK KETTN\n    GIB ZRUCK \"oide Kortn\" OIDA\nPASST SCHO\n")]);
    let lauf = hol(&["franz/kortn@v1", "--ois", "oide"]);
    assert!(lauf.status.success(), "{}", text(&lauf.stderr));
    assert!(lies(&projekt.join("pakete.txt")).contains("oide = franz/kortn@v1 sha256:"));

    // Ein Ordner am eigenen Rechner als Paket.
    let lokal = arbeit.join("mei-paket");
    fs::create_dir_all(lokal.join(".git")).unwrap();
    fs::write(lokal.join("paket.oida"), "FUNKTIAUN servas() GIBT ZRUCK KETTN\n    GIB ZRUCK \"vo daham\" OIDA\nPASST SCHO\n").unwrap();
    fs::write(lokal.join(".git").join("HEAD"), "gehört nicht zum Paket").unwrap();
    let lauf = hol(&["../mei-paket"]);
    assert!(lauf.status.success(), "{}", text(&lauf.stderr));
    assert!(text(&lauf.stdout).contains("Im Programm: BRAUCH \"mei-paket\" OIS mei_paket OIDA"), "{}", text(&lauf.stdout));
    let lauf = hol(&["../mei-paket", "--ois", "daham"]);
    assert!(lauf.status.success(), "{}", text(&lauf.stderr));
    assert!(projekt.join("pakete/daham/paket.oida").is_file() && !projekt.join("pakete/daham/.git").exists());
    fs::write(projekt.join("zwa.oida"), "BRAUCH \"daham\" OIS d OIDA\nBRAUCH \"oide\" OIS o OIDA\nSCHREIB(d.servas(), o.ziag()) OIDA\n").unwrap();
    let exe = projekt.join("zwa.exe");
    uebersetze(&projekt.join("zwa.oida"), &exe, &[]);
    assert_eq!(text(&starte(&exe).stdout), "vo daham oide Kortn\n");

    // Was es nicht gibt, und was keine Adresse ist.
    let lauf = hol(&["franz/gibtsned"]);
    assert_eq!(lauf.status.code(), Some(2));
    assert!(text(&lauf.stderr).contains("franz/gibtsned/archive/HEAD.tar.gz"), "{}", text(&lauf.stderr));
    let lauf = hol(&["irgendwos"]);
    assert!(text(&lauf.stderr).contains("'irgendwos' vasteh i ned. So geht's: oida hol wer/paket"), "{}", text(&lauf.stderr));

    // Zwei Pakete auf einmal und eine Option, die es nicht gibt.
    let lauf = hol(&["franz/kortn", "franz/wuerfel"]);
    assert_eq!(lauf.status.code(), Some(2));
    assert!(text(&lauf.stderr).contains("I hoi nur a Paket auf amoi ('franz/wuerfel' is z'vü)."), "{}", text(&lauf.stderr));
    let lauf = hol(&["franz/kortn", "--schnö"]);
    assert_eq!(lauf.status.code(), Some(2));
    assert!(text(&lauf.stderr).contains("De Option --schnö kenn i bei 'hol' ned."), "{}", text(&lauf.stderr));

    // Ein neuer Stand ersetzt den alten ganz: Was es im neuen nicht mehr gibt, bleibt nicht liegen.
    paket_ablegen(&basis, "franz", "kortn", "v2", &[("oide.oida", "FUNKTIAUN ziag() GIBT ZRUCK KETTN\n    GIB ZRUCK \"neiche Kortn\" OIDA\nPASST SCHO\n")]);
    assert!(projekt.join("pakete/oide/paket.oida").is_file());
    let lauf = hol(&["franz/kortn@v2", "--ois", "oide"]);
    assert!(lauf.status.success(), "{}", text(&lauf.stderr));
    assert!(projekt.join("pakete/oide/oide.oida").is_file());
    assert!(!projekt.join("pakete/oide/paket.oida").exists(), "die Datei aus dem alten Stand liegt noch da");
    assert!(lies(&projekt.join("pakete.txt")).contains("oide = franz/kortn@v2 sha256:"));

    // Ein Paket, das selbst etwas aus einem Ordner braucht: Den gibt es nur bei dem, der es geschrieben hat.
    // (Hier gibt es den Ordner zufällig; geholt wird er trotzdem nicht.)
    paket_ablegen(
        &basis,
        "franz",
        "schlecht",
        "HEAD",
        &[("paket.oida", "FUNKTIAUN nix_do()\nPASST SCHO\n"), ("pakete.txt", "fremd = ../mei-paket sha256:00\n")],
    );
    let lauf = hol(&["franz/schlecht"]);
    assert_eq!(lauf.status.code(), Some(2));
    assert!(
        text(&lauf.stderr).contains("Des Paket 'schlecht' braucht 'fremd' aus an Ordner (../mei-paket), den's nur beim Schreiba gibt."),
        "{}",
        text(&lauf.stderr)
    );
    assert!(!projekt.join("pakete/fremd").exists());
}

#[test]
fn fehler_beim_laufen_werden_gemeldet() {
    let arbeit = arbeitsordner("laufzeitfehler");
    for quelle in oida_dateien(&tests().join("laufzeitfehler")) {
        for (art, optionen) in [("optimiert", &[][..]), ("ohne-optimierung", &["--ohne-optimierung"][..])] {
            let exe = arbeit.join(format!("{}-{art}.exe", name(&quelle)));
            uebersetze(&quelle, &exe, optionen);
            let lauf = befehl(&exe, Some(&quelle)).output().unwrap();
            assert_eq!(lauf.status.code(), Some(1), "{} ({art})", quelle.display());
            // Was vor dem Fehler geschrieben wurde, darf nicht verloren gehen.
            assert_eq!(text(&lauf.stdout), lies(&quelle.with_extension("erwartet")), "{} ({art})", quelle.display());
            assert_eq!(text(&lauf.stderr), lies(&quelle.with_extension("fehler")), "{} ({art})", quelle.display());
        }
    }
}

#[test]
fn programm_kennt_seine_argumente_und_hoert_mit_code_auf() {
    // Ein Werkzeug für die Kommandozeile: Argumente (auch mit Leerzeichen und
    // Umlauten), eine Meldung auf der Fehlerausgabe, ein eigener Code am Ende.
    let arbeit = arbeitsordner("werkzeug");
    let exe = arbeit.join("aufhean.exe");
    uebersetze(&tests().join("werkzeug").join("aufhean.oida"), &exe, &[]);
    let datei = arbeit.join("aufhean.txt");

    let lauf = befehl(&exe, None).args(["--aus", "zwa Wörter", "", "a\"b"]).output().unwrap();
    assert_eq!(lauf.status.code(), Some(3));
    assert_eq!(text(&lauf.stdout), "4 [\"--aus\", \"zwa Wörter\", \"\", \"a\"b\"]\n[--aus]\n[zwa Wörter]\n[]\n[a\"b]\nglei is aus\n");
    assert_eq!(text(&lauf.stderr), "I hea auf, wäu du des so wüst.\n");
    // Was vor dem Aufhören in die offene Datei ging, steht drin.
    assert_eq!(lies(&datei), "no gschwind gschriebn\n");

    // Ohne Argumente läuft es bis zum Ende und endet mit 0; auch dann wird die Datei fertig.
    fs::remove_file(&datei).unwrap();
    let lauf = befehl(&exe, None).env("OIDA_TEXTE_PRUEFEN", "1").output().unwrap();
    assert_eq!(lauf.status.code(), Some(0));
    assert_eq!(text(&lauf.stdout), "0 []\nbis zum End\n");
    assert_eq!(text(&lauf.stderr), "OIDA: 0 Texte, Listen und Objekte übrig\n");
    assert_eq!(lies(&datei), "no gschwind gschriebn\n");
}

/// Ein kleiner HTTP-Server für den Test darunter. Gibt den Port zurück, auf dem er horcht.
fn probe_server() -> u16 {
    use std::io::{BufRead, BufReader, Read, Write};
    let horcher = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = horcher.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut strom in horcher.incoming().flatten() {
            let mut leser = BufReader::new(strom.try_clone().unwrap());
            let mut kopf = Vec::new();
            loop {
                let mut zeile = String::new();
                if leser.read_line(&mut zeile).unwrap_or(0) == 0 || zeile == "\r\n" {
                    break;
                }
                kopf.push(zeile.trim_end().to_string());
            }
            let wert = |name: &str| {
                kopf.iter().find_map(|z| z.to_ascii_lowercase().strip_prefix(&format!("{name}:")).map(|w| w.trim().to_string())).unwrap_or_default()
            };
            let mut inhalt = vec![0; wert("content-length").parse().unwrap_or(0)];
            let _ = leser.read_exact(&mut inhalt);
            let inhalt = String::from_utf8_lossy(&inhalt).into_owned();
            let erste: Vec<&str> = kopf.first().map(|z| z.split_whitespace().collect()).unwrap_or_default();
            let (methode, pfad) = (erste.first().copied().unwrap_or(""), erste.get(1).copied().unwrap_or(""));
            let fertig = |code: &str, koepfe: &str, inhalt: &[u8]| {
                let mut antwort = format!("HTTP/1.1 {code}\r\nContent-Length: {}\r\n{koepfe}\r\n", inhalt.len()).into_bytes();
                antwort.extend_from_slice(inhalt);
                antwort
            };
            let antwort = match pfad.split('?').next().unwrap_or("") {
                "/servas" => fertig(
                    "200 OK",
                    "Content-Type: text/plain; charset=utf-8\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\n",
                    format!("Servas vom Server! ({pfad})").as_bytes(),
                ),
                "/echo" => fertig("200 OK", "", format!("{methode} {pfad}|{}|{}|{inhalt}", wert("x-probe"), wert("content-type")).as_bytes()),
                // {"nam":"Susi","punkte":70} -> 50 Punkte mehr, und ein Eintrag, den das Programm nicht kennt.
                "/punkte" => {
                    let punkte: i64 = inhalt.rsplit(':').next().unwrap_or("").trim_end_matches('}').trim().parse().unwrap_or(-1);
                    let name = inhalt.split('"').nth(3).unwrap_or("?");
                    fertig("200 OK", "Content-Type: application/json\r\n", format!("{{\"dazua\": 50, \"nam\": \"{name}\", \"punkte\": {}}}", punkte + 50).as_bytes())
                }
                "/oid" => fertig("301 Moved Permanently", "Location: /servas\r\n", b""),
                "/stickln" => b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nServa\r\n3\r\ns, \r\n4\r\nOIDA\r\n0\r\n\r\n".to_vec(),
                "/bytes" => fertig("200 OK", "Content-Type: application/octet-stream\r\n", &[0, 255, 79, 73, 68, 65, 10]),
                _ => fertig("404 Not Found", "", b"nix do"),
            };
            let _ = strom.write_all(&antwort);
            let _ = strom.shutdown(std::net::Shutdown::Both);
        }
    });
    port
}

#[test]
fn netz_holt_und_schickt() {
    // HTTP von Anfang bis Ende: Das OIDA-Programm redet mit einem Server in diesem Test.
    let arbeit = arbeitsordner("netz");
    let quelle = tests().join("werkzeug").join("netz_http.oida");
    let exe = arbeit.join("netz_http.exe");
    uebersetze(&quelle, &exe, &[]);
    let port = probe_server();
    let lauf = befehl(&exe, None).env("OIDA_TEST_PORT", port.to_string()).env("OIDA_TEXTE_PRUEFEN", "1").output().unwrap();
    assert_eq!(text(&lauf.stdout), lies(&quelle.with_extension("erwartet")));
    assert_eq!(text(&lauf.stderr), "OIDA: 0 Texte, Listen und Objekte übrig\n");
    assert_eq!(lauf.status.code(), Some(0));
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
    // Unter Windows heißt das Programm servas.exe, sonst einfach servas.
    let programm = arbeit.join(format!("servas{}", std::env::consts::EXE_SUFFIX));
    assert_eq!(text(&starte(&programm).stdout), "Servas Wöd!\n");
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
fn ordner_fuer_das_programm_fehlt() {
    // Das soll der Compiler selbst sagen, nicht erst der Linker (das sähe aus wie ein Fehler im Compiler).
    let arbeit = arbeitsordner("ordner-fehlt");
    let ergebnis = oida()
        .arg(wurzel().join("beispiele").join("hallo.oida"))
        .arg("-o")
        .arg(arbeit.join("den").join("gibts").join("ned"))
        .output()
        .unwrap();
    assert_eq!(ergebnis.status.code(), Some(2));
    let meldung = text(&ergebnis.stderr);
    assert!(meldung.starts_with("Heast! Den Ordner ") && meldung.contains("gibt's ned"), "{meldung}");
    assert!(!meldung.contains("clang"), "{meldung}");
}

#[test]
fn datei_die_es_nicht_gibt() {
    let ergebnis = oida().arg("gibts_ned.oida").output().unwrap();
    assert_eq!(ergebnis.status.code(), Some(2));
    assert!(text(&ergebnis.stderr).starts_with("Heast! I kaun gibts_ned.oida ned lesn"), "{}", text(&ergebnis.stderr));
}

/// Außerhalb von Windows sagt schon der Compiler, dass es Fenster und Ton dort noch nicht gibt
/// (statt eines Programms, das dann nicht läuft).
#[test]
#[cfg(not(windows))]
fn fenster_und_ton_gibt_es_nur_unter_windows() {
    let arbeit = arbeitsordner("nur-windows");
    for (quelle, teil) in [("programme/toene.oida", "ton"), ("laufzeitfehler/zeichnen_ohne_fenster.oida", "fensta")] {
        let exe = arbeit.join("x");
        let ergebnis = oida().arg(tests().join(quelle)).arg("-o").arg(&exe).output().unwrap();
        let meldung = text(&ergebnis.stderr);
        assert_eq!(ergebnis.status.code(), Some(1), "{quelle}:\n{meldung}");
        assert!(meldung.contains(&format!("De Bibliothek '{teil}' gibt's derweil nur unter Windows")), "{quelle}:\n{meldung}");
        assert!(!exe.exists());
    }
}

/// Ein Bild, wie es die Laufzeit beim Zeichnen ohne Fenster schreibt
/// (BMP mit 32 Bit, die unterste Zeile steht zuerst in der Datei).
struct Bild {
    breite: usize,
    hoehe: usize,
    punkte: Vec<u8>,
}

impl Bild {
    fn lies(datei: &Path) -> Bild {
        let bytes = fs::read(datei).unwrap_or_else(|e| panic!("{}: {e}", datei.display()));
        assert_eq!(&bytes[..2], b"BM", "{} ist kein BMP", datei.display());
        let zahl = |ab: usize| u32::from_le_bytes(bytes[ab..ab + 4].try_into().unwrap()) as usize;
        let (anfang, breite, hoehe) = (zahl(10), zahl(18), zahl(22));
        assert_eq!(u16::from_le_bytes([bytes[28], bytes[29]]), 32);
        Bild { breite, hoehe, punkte: bytes[anfang..].to_vec() }
    }

    /// Rot, Grün und Blau am Punkt (x, y); (0, 0) ist links oben.
    fn farbe(&self, x: usize, y: usize) -> (u8, u8, u8) {
        let stelle = ((self.hoehe - 1 - y) * self.breite + x) * 4;
        (self.punkte[stelle + 2], self.punkte[stelle + 1], self.punkte[stelle])
    }

    /// Stimmt die Farbe am Punkt, bis auf Rundung?
    #[track_caller]
    fn pruefe(&self, x: usize, y: usize, soll: (u8, u8, u8), was: &str) {
        let ist = self.farbe(x, y);
        let nah = |a: u8, b: u8| a.abs_diff(b) <= 3;
        assert!(nah(ist.0, soll.0) && nah(ist.1, soll.1) && nah(ist.2, soll.2), "{was} bei ({x}, {y}): {ist:?} statt {soll:?}");
    }

    /// Wie viele Punkte im Rechteck eine andere Farbe haben als `grund`.
    fn andere(&self, x: std::ops::Range<usize>, y: std::ops::Range<usize>, grund: (u8, u8, u8)) -> usize {
        y.flat_map(|y| x.clone().map(move |x| (x, y))).filter(|(x, y)| self.farbe(*x, *y) != grund).count()
    }
}

#[test]
#[cfg_attr(not(windows), ignore = "Fenster und Ton gibt es derzeit nur unter Windows")]
fn fenster_zeichnet_die_richtigen_formen() {
    // Das Programm zeichnet in Bilder statt in ein Fenster; hier werden einzelne Punkte nachgeschaut.
    let arbeit = arbeitsordner("fenster");
    let exe = arbeit.join("formen.exe");
    uebersetze(&tests().join("fenster").join("formen.oida"), &exe, &[]);
    let lauf = befehl(&exe, None)
        .env("OIDA_FENSTER_BILDER", &arbeit)
        .env("OIDA_FENSTER_ENDE", "2")
        .env("OIDA_TEXTE_PRUEFEN", "1")
        .output()
        .unwrap();
    assert!(lauf.status.success(), "{}", text(&lauf.stderr));
    assert_eq!(text(&lauf.stdout), "400 300\n2 WOAHR WOAHR\n");
    assert_eq!(text(&lauf.stderr), "OIDA: 0 Texte, Listen und Objekte übrig\n");

    let bild = Bild::lies(&arbeit.join("bild_002.bmp"));
    assert_eq!((bild.breite, bild.hoehe), (400, 300));
    let grund = (20, 30, 60);
    let weiss = (255, 255, 255);
    bild.pruefe(5, 5, grund, "Hintergrund");
    // Gefüllte Formen
    bild.pruefe(50, 40, (255, 0, 0), "Rechteck");
    bild.pruefe(85, 40, grund, "rechts vom Rechteck");
    bild.pruefe(150, 40, (0, 255, 0), "Kreis");
    bild.pruefe(150, 12, grund, "über dem Kreis");
    bild.pruefe(230, 45, (0, 0, 255), "Dreieck");
    bild.pruefe(205, 20, grund, "neben der Spitze vom Dreieck");
    bild.pruefe(320, 40, (255, 255, 0), "rundes Rechteck");
    bild.pruefe(281, 21, grund, "runde Ecke");
    bild.pruefe(60, 110, (255, 0, 255), "Ellipse");
    bild.pruefe(150, 110, (0, 255, 255), "Vieleck");
    // Ränder: die Linie selbst, und innen bleibt der Hintergrund
    bild.pruefe(220, 110, weiss, "Rand vom Rechteck");
    bild.pruefe(250, 110, grund, "im Rechteck mit Rand");
    bild.pruefe(330, 85, weiss, "Kreislinie");
    bild.pruefe(330, 110, grund, "im Kreis mit Rand");
    bild.pruefe(200, 160, weiss, "Linie");
    bild.pruefe(200, 165, grund, "unter der Linie");
    bild.pruefe(120, 220, weiss, "Bogen unten");
    bild.pruefe(120, 180, grund, "wo der Bogen oben offen ist");
    bild.pruefe(20, 200, weiss, "Linienzug, das Stück zurück zum Anfang");
    // Halb durchsichtiges Blau über Rot und über dem Hintergrund
    bild.pruefe(190, 200, (255, 0, 0), "Rot allein");
    bild.pruefe(210, 200, (128, 0, 128), "Blau halb über Rot");
    bild.pruefe(230, 200, (10, 15, 158), "Blau halb über dem Hintergrund");
    // Verschoben nach (320, 200), um 90 Grad gedreht und verdoppelt: aus 20 x 5 wird ein Balken nach unten
    bild.pruefe(315, 220, (255, 128, 0), "gedrehtes Rechteck");
    bild.pruefe(315, 238, (255, 128, 0), "gedrehtes Rechteck, unten");
    bild.pruefe(325, 220, grund, "rechts vom gedrehten Rechteck");
    bild.pruefe(305, 220, grund, "links vom gedrehten Rechteck");
    // Nach lage_zruck() ist alles wieder gerade
    bild.pruefe(390, 290, (128, 128, 128), "Rechteck in der Ecke");
    // Text: Wo er stehen soll, ist etwas; daneben nicht
    assert!(bild.andere(20..90, 240..264, grund) > 100, "kein Text links");
    assert_eq!(bild.andere(100..300, 240..252, grund), 0, "Text, wo keiner hingehört");
    assert!(bild.andere(150..250, 256..284, grund) > 200, "kein Text in der Mitte");
    assert!(bild.andere(330..380, 240..260, grund) > 60, "kein Text rechts");
    assert_eq!(bild.andere(381..400, 240..260, grund), 0, "der rechte Text ragt über seine Kante");

    // Das erste Bild unterscheidet sich nur dort, wo die Nummer steht.
    let erstes = Bild::lies(&arbeit.join("bild_001.bmp"));
    let mut anders = Vec::new();
    for y in 0..300 {
        for x in 0..400 {
            if erstes.farbe(x, y) != bild.farbe(x, y) {
                anders.push((x, y));
            }
        }
    }
    assert!(!anders.is_empty(), "beide Bilder sind gleich, obwohl die Nummer anders ist");
    assert!(anders.iter().all(|(x, y)| (200..260).contains(x) && (255..290).contains(y)), "{anders:?}");
}

#[test]
#[cfg_attr(not(windows), ignore = "Fenster und Ton gibt es derzeit nur unter Windows")]
fn echtes_fenster_geht_auf_und_zu() {
    // Hier geht für einen Augenblick wirklich ein Fenster auf. Geprüft wird,
    // dass Windows mitspielt und das Programm sauber aufhört; was im Fenster
    // zu sehen ist, prüft der Test davor am Bild.
    if std::env::var_os("OIDA_TEST_OHNE_FENSTER").is_some() {
        return;
    }
    let arbeit = arbeitsordner("fenster_echt");
    let exe = arbeit.join("formen.exe");
    uebersetze(&tests().join("fenster").join("formen.oida"), &exe, &[]);
    let lauf = befehl(&exe, None).env("OIDA_FENSTER_ENDE", "5").env("OIDA_TEXTE_PRUEFEN", "1").output().unwrap();
    assert!(lauf.status.success(), "{}", text(&lauf.stderr));
    assert_eq!(text(&lauf.stdout), "400 300\n5 WOAHR WOAHR\n");
    assert_eq!(text(&lauf.stderr), "OIDA: 0 Texte, Listen und Objekte übrig\n");
}

#[test]
#[cfg_attr(not(windows), ignore = "Fenster und Ton gibt es derzeit nur unter Windows")]
fn spiel_laeuft_nach_drehbuch() {
    // Das Beispielspiel, ohne Fenster und mit Tasten nach Drehbuch: nach
    // rechts und dann hinauf zum Stern (der Zufall ist festgelegt).
    let arbeit = arbeitsordner("spiel");
    let exe = arbeit.join("sternfanga.exe");
    uebersetze(&wurzel().join("beispiele").join("spui").join("sternfanga.oida"), &exe, &[]);
    let spiel = |bilder: &str, tasten: &str| {
        let lauf = befehl(&exe, None)
            .env("OIDA_SOMEN", "3")
            .env("OIDA_FENSTER_BILDER", &arbeit)
            .env("OIDA_FENSTER_BILD_JEDES", "100")
            .env("OIDA_FENSTER_ENDE", bilder)
            .env("OIDA_FENSTER_TASTEN", tasten)
            .env("OIDA_TEXTE_PRUEFEN", "1")
            .output()
            .unwrap();
        assert!(lauf.status.success(), "{}", text(&lauf.stderr));
        assert_eq!(text(&lauf.stderr), "OIDA: 0 Texte, Listen und Objekte übrig\n");
        text(&lauf.stdout)
    };
    // Ein Stern gefangen: 10 Punkte, und der Bestwert wird gespeichert.
    assert_eq!(spiel("100", "RECHTS@1-62,AUFFE@63-82"), "Punkte: 10 - Bestes: 10\n");
    assert_eq!(lies(&arbeit.join("sternfanga_bestes.txt")), "10");
    let bild = Bild::lies(&arbeit.join("bild_100.bmp"));
    assert_eq!((bild.breite, bild.hoehe), (800, 600));
    bild.pruefe(700, 580, (13, 27, 42), "Hintergrund");
    bild.pruefe(790, 5, (27, 38, 59), "Leiste oben");
    // Beim nächsten Mal ist der Bestwert noch da, auch ohne etwas zu fangen.
    assert_eq!(spiel("3", ""), "Punkte: 0 - Bestes: 10\n");
    // Wer stehen bleibt, wird erwischt: Nach drei Treffern ist das Spiel aus, ESC beendet es.
    assert_eq!(spiel("2000", "ESC@1990"), "Punkte: 0 - Bestes: 10\n");
}

/// Wie viele Punkte im Rechteck `passt` erfüllen.
fn zaehle(bild: &Bild, x: std::ops::Range<usize>, y: std::ops::Range<usize>, passt: impl Fn((u8, u8, u8)) -> bool) -> usize {
    y.flat_map(|y| x.clone().map(move |x| (x, y))).filter(|(x, y)| passt(bild.farbe(*x, *y))).count()
}

#[test]
#[cfg_attr(not(windows), ignore = "Fenster und Ton gibt es derzeit nur unter Windows")]
fn roguewave_geht_durch_alle_zustaende() {
    // Das große Beispiel: ein ganzes Spiel in OIDA. Ein Prüfprogramm holt sich
    // seine Teile dazu und führt es durch Anfangsbild, Waffenwahl, Spiel mit
    // allen Waffen und Gegnern, Weiterentwicklungen, Ereignisse, Boss, Pause,
    // Endboss, Sieg, Endlos, Ende und Bestenliste.
    let arbeit = arbeitsordner("roguewave_probe");
    let exe = arbeit.join("probe.exe");
    let quelle = tests().join("fenster").join("roguewave_probe.oida");
    uebersetze(&quelle, &exe, &[]);
    let lauf = befehl(&exe, None)
        .env("OIDA_FENSTER_BILDER", &arbeit)
        .env("OIDA_FENSTER_BILD_JEDES", "250")
        // Im Spiel ab Bild 500 nach rechts laufen und viermal ausweichen.
        .env("OIDA_FENSTER_TASTEN", "D@501-2000,LAA@700,LAA@1000,LAA@1300,LAA@1600")
        .env("OIDA_TEXTE_PRUEFEN", "1")
        .output()
        .unwrap();
    assert!(lauf.status.success(), "{}", text(&lauf.stderr));
    assert_eq!(text(&lauf.stdout), lies(&quelle.with_extension("erwartet")));
    assert_eq!(text(&lauf.stderr), "OIDA: 0 Texte, Listen und Objekte übrig\n");

    let bild = |nummer: usize| {
        let bild = Bild::lies(&arbeit.join(format!("bild_{nummer}.bmp")));
        assert_eq!((bild.breite, bild.hoehe), (960, 640));
        bild
    };
    let gold = |f: (u8, u8, u8)| f.0 > 150 && f.1 > 120 && f.2 < 60 && f.0 > f.1;
    let hell = |f: (u8, u8, u8)| f.0.max(f.1).max(f.2) > 150;
    let blau = |f: (u8, u8, u8)| f.2 > 200 && f.0 < 140 && f.1 > 150;

    // Das Anfangsbild: Himmel mit Verlauf, Titel, die gewählte Spielart, PLAY, unten die Einstellungen
    let anfang = bild(250);
    anfang.pruefe(5, 5, (7, 10, 31), "Himmel im Anfangsbild");
    assert!(zaehle(&anfang, 300..660, 70..122, hell) > 5000, "der Titel fehlt");
    anfang.pruefe(300, 334, (54, 181, 226), "Knopf PLAY");
    anfang.pruefe(410, 240, (33, 157, 205), "gewählte Spielart NORMAL");
    assert!(zaehle(&anfang, 426..534, 532..596, hell) > 200, "das Zahnrad mit SETTINGS fehlt");

    // Die Wahl der Waffe: Überschrift, vier Karten mit hellblauem Rand, links die Übersicht
    let waffen = bild(500);
    assert!(zaehle(&waffen, 400..800, 100..135, blau) > 3000, "CHOOSE YOUR WEAPON fehlt");
    for karte in 0..4 {
        let (x, y) = (269 + (karte % 3) * 230, 210 + (karte / 3) * 210);
        let rand = waffen.farbe(x + 60, y);
        let innen = waffen.farbe(x + 20, y + 100);
        assert!(rand.2 > innen.2 + 100 && rand.1 > innen.1 + 100, "Karte {karte}: Rand {rand:?}, innen {innen:?}");
    }
    assert!(zaehle(&waffen, 14..250, 80..560, blau) > 700, "die Übersicht links fehlt");

    // Mitten im Spiel: die Balken oben, die vier Waffen links unten
    let spiel = bild(2000);
    spiel.pruefe(30, 20, (255, 51, 68), "Lebensbalken");
    let xp = spiel.farbe(20, 44);
    assert!(xp == (68, 170, 255) || xp == (17, 34, 68), "Erfahrungsbalken: {xp:?}");
    for waffe in 0..4 {
        let punkte = zaehle(&spiel, 16 + waffe * 50..46 + waffe * 50, 596..632, hell);
        assert!(punkte > 80, "Waffe {waffe} fehlt in der Anzeige ({punkte} Punkte)");
    }

    // Die Auswahl nach dem großen Boss: goldene Überschrift, die vierte Karte ist die violette Weiterentwicklung
    let lohn = bild(2500);
    assert!(zaehle(&lohn, 300..910, 100..135, gold) > 4000, "die goldene Überschrift fehlt");
    let (rand, innen) = (lohn.farbe(269 + 60, 420), lohn.farbe(269 + 20, 520));
    assert!(rand.0 > 120 && rand.2 > 170 && rand.2 > rand.1 + 50, "Rand der epischen Karte: {rand:?}");
    assert!(innen.0 < 30 && innen.2 < 60, "Inneres der Karte: {innen:?}");

    // Der Boss: sein Lebensbalken unten in der Mitte (rosa, oder golden, solange er offen dasteht)
    let boss = bild(3000);
    let balken = boss.farbe(335, 585);
    assert!(balken == (221, 34, 119) || balken == (255, 215, 0), "Lebensbalken vom Boss: {balken:?}");

    // Die Pause deckt das Spiel zu: Knöpfe in der Mitte, Übersicht links, Stand vom Lauf rechts.
    let pause = bild(3250);
    pause.pruefe(5, 5, (10, 10, 30), "Hintergrund der Pause");
    assert!(zaehle(&pause, 340..620, 120..540, blau) > 10000, "die Knöpfe der Pause fehlen");
    assert!(zaehle(&pause, 28..264, 60..580, blau) > 500, "die Übersicht in der Pause fehlt");
    assert!(zaehle(&pause, 708..932, 230..410, blau) > 350, "der Stand vom Lauf fehlt");

    // Der Endboss: sein Leben steht oben, noch voll und in der Farbe der ersten Phase.
    bild(3750).pruefe(340, 48, (255, 0, 255), "Lebensbalken vom Endboss");

    // Gewonnen: goldene Überschrift. Aus: rote.
    let gewonnen = bild(4000);
    assert!(zaehle(&gewonnen, 250..710, 40..100, gold) > 3000, "YOU WIN fehlt");
    gewonnen.pruefe(5, 5, (17, 17, 34), "Hintergrund nach dem Sieg");
    let _ = bild(4250);
    let aus = bild(4500);
    let rot = |f: (u8, u8, u8)| f.0 > 230 && (50..95).contains(&f.1) && (80..125).contains(&f.2);
    assert!(zaehle(&aus, 330..630, 45..95, rot) > 3000, "GAME OVER fehlt");
    let _ = bild(4750);

    // Die Bestenlisten liegen als Dateien da: eine Zeile pro Lauf, der gewonnene vor dem verlorenen.
    let normal = lies(&arbeit.join("roguewave_bestenlistn_normal.txt"));
    let zeilen: Vec<&str> = normal.lines().collect();
    assert_eq!(zeilen.len(), 2);
    assert!(zeilen[0].starts_with("lauf") && zeilen[0].contains(";25;") && zeilen[0].ends_with(";jo"), "{normal}");
    assert!(zeilen[1].contains(";26;") && zeilen[1].ends_with(";na"), "{normal}");
    let leicht = lies(&arbeit.join("roguewave_bestenlistn_easy.txt"));
    assert_eq!(leicht.lines().count(), 1);
    assert!(leicht.contains(";1;0;") && leicht.ends_with(";na\n"), "{leicht}");
}

#[test]
#[cfg_attr(not(windows), ignore = "Fenster und Ton gibt es derzeit nur unter Windows")]
fn roguewave_spielt_sich() {
    // Das Spiel selbst, wie es jemand startet: ENTER, die erste Waffe nehmen,
    // herumlaufen, ausweichen, bei jedem Aufstieg die erste Verbesserung nehmen.
    // Eine halbe Minute nach Drehbuch.
    let arbeit = arbeitsordner("roguewave");
    let exe = arbeit.join("roguewave.exe");
    uebersetze(&wurzel().join("beispiele").join("roguewave").join("roguewave.oida"), &exe, &[]);
    let mut tasten = String::from("ENTER@5");
    // Große Runden: Wer stehen bleibt, wird überrannt.
    for runde in 0..3 {
        let a = 10 + runde * 800;
        tasten += &format!(",RECHTS@{}-{},S@{}-{},A@{}-{},AUFFE@{}-{}", a, a + 199, a + 200, a + 399, a + 400, a + 599, a + 600, a + 799);
        tasten += &format!(",LAA@{},LAA@{}", a + 300, a + 700);
    }
    for mal in 1..=30 {
        tasten += &format!(",1@{}", mal * 60);
    }
    let lauf = befehl(&exe, None)
        .env("OIDA_SOMEN", "7")
        .env("OIDA_FENSTER_BILDER", &arbeit)
        .env("OIDA_FENSTER_BILD_JEDES", "900")
        .env("OIDA_FENSTER_ENDE", "1800")
        .env("OIDA_FENSTER_TASTEN", &tasten)
        .env("OIDA_TEXTE_PRUEFEN", "1")
        .output()
        .unwrap();
    assert!(lauf.status.success(), "{}", text(&lauf.stderr));
    assert_eq!(text(&lauf.stderr), "OIDA: 0 Texte, Listen und Objekte übrig\n");
    assert_eq!(text(&lauf.stdout), "Wave 2 - Kills 23 - Level 1 - Zeit 00:28\n");
    let bild = Bild::lies(&arbeit.join("bild_1800.bmp"));
    assert_eq!((bild.breite, bild.hoehe), (960, 640));
    let leben = bild.farbe(18, 20);
    assert!(leben.0 > 200 && leben.2 < 110, "Lebensbalken: {leben:?}");
    // Die Spielfigur ist in der Nähe der Mitte zu sehen (die Kamera zieht etwas nach).
    let figur = zaehle(&bild, 380..580, 220..420, |f| f == (51, 204, 255));
    assert!(figur > 300, "von der Spielfigur sind nur {figur} Punkte da");
}

#[test]
#[cfg_attr(not(windows), ignore = "Fenster und Ton gibt es derzeit nur unter Windows")]
fn fenster_kann_verlaeufe_leuchten_formen_und_bilder() {
    // Farbverläufe, additives Mischen, Leuchten, freie Formen, Ausschnitte, Bilder
    // aus einer Datei und selbst gezeichnete, Textbreite: einzelne Punkte nachgeschaut.
    let arbeit = arbeitsordner("fenster_mehr");
    let exe = arbeit.join("mehr.exe");
    uebersetze(&tests().join("fenster").join("mehr.oida"), &exe, &[]);
    fs::copy(tests().join("fenster").join("probe.png"), arbeit.join("probe.png")).unwrap();
    let lauf = befehl(&exe, None)
        .env("OIDA_FENSTER_BILDER", &arbeit)
        .env("OIDA_FENSTER_ENDE", "12")
        // Die Maus nach Drehbuch: hinstellen, zweimal klicken, am Rad drehen (drei Rasten hinunter, eine hinauf).
        .env("OIDA_FENSTER_TASTEN", "MAUS(100;50)@3,KLICK@4,MAUS(320.5;240)@6-9,KLICK@7-8,RADL_OBE@5-7,RADL_AUFFE@9")
        .env("OIDA_TEXTE_PRUEFEN", "1")
        .output()
        .unwrap();
    assert!(lauf.status.success(), "{}", text(&lauf.stderr));
    assert_eq!(
        text(&lauf.stdout),
        "64 64 127.0 0.0\nMaus: 2 320.5 240.0 Radl: 2.0\ngladn: 48 48\ngibts_ned.png: De gibt's ned.\nka_büd.png: Des is ka Büd, des i lesn kaun.\n"
    );
    assert_eq!(text(&lauf.stderr), "OIDA: 0 Texte, Listen und Objekte übrig\n");

    let bild = Bild::lies(&arbeit.join("bild_002.bmp"));
    assert_eq!((bild.breite, bild.hoehe), (640, 480));
    let grund = (17, 17, 34);
    bild.pruefe(5, 5, grund, "Hintergrund");

    // Gerader Verlauf: links rot, rechts blau, in der Mitte beides halb
    let (l, m, r) = (bild.farbe(25, 50), bild.farbe(120, 50), bild.farbe(215, 50));
    assert!(l.0 > 230 && l.2 < 30, "links im Verlauf: {l:?}");
    assert!(r.2 > 230 && r.0 < 30, "rechts im Verlauf: {r:?}");
    assert!((100..160).contains(&m.0) && (100..160).contains(&m.2) && m.1 < 10, "Mitte vom Verlauf: {m:?}");
    // Runder Verlauf: innen weiß, auf halbem Weg halb durchsichtig, außen nichts mehr
    let (innen, halb) = (bild.farbe(320, 50), bild.farbe(340, 50));
    assert!(innen.0 > 235 && innen.1 > 235, "Mitte vom runden Verlauf: {innen:?}");
    assert!((90..185).contains(&halb.0), "halber Weg im runden Verlauf: {halb:?}");
    bild.pruefe(365, 50, grund, "außerhalb vom runden Verlauf");

    // Additiv gemischt: Rot und Grün zählen sich in der Mitte zu Gelb zusammen (und zum Hintergrund dazu).
    bild.pruefe(430, 50, (255, 17, 34), "nur Rot");
    bild.pruefe(500, 50, (17, 255, 34), "nur Grün");
    bild.pruefe(465, 50, (255, 255, 34), "Rot und Grün zusammen");

    // Leuchten: Um den weißen Kreis liegt ein bläulicher Schein, der nach außen schwächer wird.
    bild.pruefe(70, 170, (255, 255, 255), "leuchtender Kreis");
    let (nah, weit) = (bild.farbe(70, 143), bild.farbe(70, 132));
    assert!(nah.2 > 60 && nah.2 > nah.0 + 20, "Schein knapp über dem Kreis: {nah:?}");
    assert!(weit.2 < nah.2 && weit.2 > grund.2, "Schein weiter weg: {weit:?} nach {nah:?}");
    bild.pruefe(70, 105, grund, "kein Schein mehr");

    // Freie Form (ein Herz aus zwei Kurven), dreifach vergrößert
    bild.pruefe(360, 172, (255, 68, 102), "im Herz");
    bild.pruefe(360, 150, grund, "in der Kerbe über dem Herz");
    bild.pruefe(325, 190, grund, "neben der Spitze vom Herz");

    // Ausschnitte: Das gelbe Quadrat ist nur im Kreis da, das grüne nur im runden Rechteck.
    bild.pruefe(480, 170, (255, 238, 85), "im Kreis-Ausschnitt");
    bild.pruefe(445, 135, grund, "Ecke vom Quadrat, außerhalb vom Kreis");
    bild.pruefe(580, 170, (85, 255, 136), "im runden Ausschnitt");
    bild.pruefe(541, 141, grund, "runde Ecke vom Ausschnitt");
    bild.pruefe(533, 170, grund, "links vom Ausschnitt");

    // Das selbst gezeichnete Bild: in eigener Größe, doppelt so groß, und nur ein Stück davon
    let (rot, blau) = ((255, 68, 102), (51, 204, 255));
    bild.pruefe(30, 270, rot, "Bild: Quadrat");
    bild.pruefe(60, 300, blau, "Bild: Kreis");
    bild.pruefe(75, 265, grund, "Bild: durchsichtige Ecke");
    bild.pruefe(110, 270, rot, "großes Bild: Quadrat");
    bild.pruefe(180, 340, blau, "großes Bild: Kreis");
    bild.pruefe(254, 264, rot, "Stück vom Bild: Quadrat");
    bild.pruefe(290, 300, blau, "Stück vom Bild: Kreis");
    // Nur beim richtigen Stück (ab 20, 40 breit, doppelt vergrößert) reicht der Kreis bis hierher.
    bild.pruefe(326, 300, blau, "Stück vom Bild: rechter Rand vom Kreis");

    // Das Bild aus der Datei, mit seiner halb durchsichtigen Scheibe; daneben vergrößert mit harten Kanten
    bild.pruefe(365, 265, (255, 0, 0), "PNG: rot");
    bild.pruefe(390, 265, (0, 255, 0), "PNG: grün");
    bild.pruefe(365, 290, (0, 0, 255), "PNG: blau");
    let scheibe = bild.farbe(396, 296);
    assert!((120..155).contains(&scheibe.0) && (125..160).contains(&scheibe.2), "PNG: halb durchsichtig: {scheibe:?}");
    bild.pruefe(450, 270, (255, 0, 0), "PNG groß: rot");
    bild.pruefe(540, 270, (0, 255, 0), "PNG groß: grün");
    bild.pruefe(450, 360, (0, 0, 255), "PNG groß: blau");
    // Harte Kanten: Direkt an der Grenze zwischen Rot und Grün gibt es keine Mischfarbe.
    for x in 500..508 {
        let f = bild.farbe(x, 270);
        assert!(f == (255, 0, 0) || f == (0, 255, 0), "PNG groß bei x = {x}: {f:?}");
    }

    // Der Rahmen ist genau so breit wie der Text (127 Punkte): rechts davon ist nichts mehr.
    bild.pruefe(20 + 127 + 6, 434, grund, "rechts vom Textrahmen");

    // Linien: gerade abgeschnitten, mit runden Enden, und danach wieder gerade.
    let gelb = (255, 238, 85);
    bild.pruefe(345, 440, gelb, "gerade Linie");
    bild.pruefe(336, 440, grund, "vor der geraden Linie");
    bild.pruefe(455, 440, gelb, "rundes Ende, links über die Linie hinaus");
    bild.pruefe(525, 440, gelb, "rundes Ende, rechts über die Linie hinaus");
    bild.pruefe(454, 433, grund, "das Ende ist rund, nicht eckig");
    bild.pruefe(556, 440, grund, "nach linien_rund(FOISCH) wieder gerade");
    bild.pruefe(580, 440, gelb, "die dritte Linie");
}

#[test]
#[cfg_attr(not(windows), ignore = "Fenster und Ton gibt es derzeit nur unter Windows")]
fn musik_aus_einer_mp3_datei() {
    // Laden (nebenher), Länge, abspielen, wiederholen, ausblenden, und was schiefgehen kann.
    let arbeit = arbeitsordner("musik");
    let exe = arbeit.join("musik.exe");
    let quelle = tests().join("fenster").join("musik.oida");
    uebersetze(&quelle, &exe, &[]);
    fs::copy(tests().join("fenster").join("ton.mp3"), arbeit.join("ton.mp3")).unwrap();
    let lauf = befehl(&exe, None).env("OIDA_TEXTE_PRUEFEN", "1").output().unwrap();
    assert!(lauf.status.success(), "{}", text(&lauf.stderr));
    assert_eq!(text(&lauf.stdout), lies(&quelle.with_extension("erwartet")));
    assert_eq!(text(&lauf.stderr), "OIDA: 0 Texte, Listen und Objekte übrig\n");
}
