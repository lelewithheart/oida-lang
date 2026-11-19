//! Das Programm und seine Umgebung (für `BRAUCH programm OIDA`): womit es
//! gestartet wurde, Umgebungsvariablen, der Ordner, in dem es arbeitet,
//! vorzeitig aufhören, und andere Programme starten.

use std::cell::RefCell;
use std::io::Write;
use std::process::{Command, Stdio};

use super::dateien::{merke_fehler, merke_meldung, schraeg};
use super::{ListenKopf, TextKopf, bytes, liste_aus_texten, neuer_text};

/// Was vom letzten gestarteten Programm zurückkam: sein Code, was es geschrieben hat, und seine Fehlerausgabe.
struct Lauf {
    code: i64,
    ausgabe: Vec<u8>,
    fehler: Vec<u8>,
}

thread_local! {
    static LETZTER_LAUF: RefCell<Lauf> = const { RefCell::new(Lauf { code: 0, ausgabe: Vec::new(), fehler: Vec::new() }) };
}

/// # Safety
/// `text` muss ein gültiger Text sein.
unsafe fn text_von(text: *const TextKopf) -> String {
    String::from_utf8_lossy(unsafe { bytes(text) }).into_owned()
}

/// `argumente()`: was beim Starten hinter dem Namen des Programms stand.
#[unsafe(no_mangle)]
pub extern "C" fn oida_programm_argumente() -> *mut ListenKopf {
    let alle: Vec<String> = std::env::args_os().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
    liste_aus_texten(&alle)
}

/// `umgebung(nam)`: der Wert einer Umgebungsvariable, oder null, wenn es sie nicht gibt.
///
/// # Safety
/// `name` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_programm_umgebung(name: *const TextKopf) -> *mut TextKopf {
    let name = unsafe { text_von(name) };
    // Namen mit "=" oder einem Nullzeichen gibt es nicht; die Standardbibliothek würde dabei abbrechen.
    if name.is_empty() || name.contains(['=', '\0']) {
        return std::ptr::null_mut();
    }
    match std::env::var_os(&name) {
        Some(wert) => neuer_text(wert.to_string_lossy().as_bytes()),
        None => std::ptr::null_mut(),
    }
}

/// `setz_umgebung(nam, wert)`: gilt für dieses Programm und alle, die es startet.
///
/// # Safety
/// `name` und `wert` müssen gültige Texte sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_programm_umgebung_setzen(name: *const TextKopf, wert: *const TextKopf) -> bool {
    let (name, wert) = unsafe { (text_von(name), text_von(wert)) };
    if name.is_empty() || name.contains(['=', '\0']) || wert.contains('\0') {
        merke_meldung("So kaun a Umgebungsvariable ned haaßn.");
        return false;
    }
    // Das Programm hat nur diesen einen Faden, der die Umgebung liest oder ändert.
    unsafe { std::env::set_var(&name, &wert) };
    true
}

/// Ein Ordner mit "/" am Ende, damit man einen Namen einfach anhängen kann.
fn ordner_text(pfad: &std::path::Path) -> *mut TextKopf {
    neuer_text(format!("{}/", schraeg(pfad).trim_end_matches('/')).as_bytes())
}

/// `arbeits_ordna()`: der Ordner, in dem das Programm gestartet wurde (dort landen Pfade ohne Ordner).
#[unsafe(no_mangle)]
pub extern "C" fn oida_programm_arbeitsordner() -> *mut TextKopf {
    match std::env::current_dir() {
        Ok(ordner) => ordner_text(&ordner),
        Err(_) => neuer_text(b"./"),
    }
}

/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_programm_arbeitsordner_wechseln(pfad: *const TextKopf) -> bool {
    match std::env::set_current_dir(unsafe { text_von(pfad) }) {
        Ok(()) => true,
        Err(fehler) => {
            merke_fehler(&fehler);
            false
        }
    }
}

/// `temp_ordna()`: der Ordner des Systems für Dateien, die man nur kurz braucht.
#[unsafe(no_mangle)]
pub extern "C" fn oida_programm_temp_ordner() -> *mut TextKopf {
    ordner_text(&std::env::temp_dir())
}

/// `hea_auf(code)`: Das Programm endet sofort. Was noch nicht ausgegeben oder
/// in eine Datei geschrieben war, kommt vorher noch hinaus.
#[unsafe(no_mangle)]
pub extern "C" fn oida_programm_aufhoeren(code: i64) -> ! {
    super::leeren();
    super::dateien::alles_zu();
    std::process::exit(code.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
}

/// `schreib_fehla(text)`: eine Zeile auf die Fehlerausgabe statt auf die gewöhnliche.
///
/// # Safety
/// `text` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_programm_fehler_schreiben(text: *const TextKopf) {
    // Was bisher gewöhnlich ausgegeben wurde, kommt zuerst: So bleibt die Reihenfolge.
    super::leeren();
    let mut aus = std::io::stderr().lock();
    let _ = aus.write_all(unsafe { bytes(text) });
    let _ = aus.write_all(b"\n");
}

/// `betriebssystem()`: worauf das Programm läuft, "windows" oder "linux".
#[unsafe(no_mangle)]
pub extern "C" fn oida_programm_system() -> *mut TextKopf {
    neuer_text(std::env::consts::OS.as_bytes())
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetOEMCP() -> u32;
    fn MultiByteToWideChar(seite: u32, art: u32, text: *const u8, laenge: i32, ziel: *mut u16, platz: i32) -> i32;
}

/// Was ein Programm ausgegeben hat, als UTF-8. Viele Programme unter Windows
/// schreiben noch in der alten Zeichentabelle der Konsole; das wird umgewandelt.
#[cfg(windows)]
fn als_utf8(roh: Vec<u8>) -> Vec<u8> {
    if std::str::from_utf8(&roh).is_ok() || roh.len() > i32::MAX as usize {
        return roh;
    }
    unsafe {
        let seite = GetOEMCP();
        let laenge = MultiByteToWideChar(seite, 0, roh.as_ptr(), roh.len() as i32, std::ptr::null_mut(), 0);
        if laenge <= 0 {
            return String::from_utf8_lossy(&roh).into_owned().into_bytes();
        }
        let mut breit = vec![0u16; laenge as usize];
        MultiByteToWideChar(seite, 0, roh.as_ptr(), roh.len() as i32, breit.as_mut_ptr(), laenge);
        String::from_utf16_lossy(&breit).into_bytes()
    }
}

/// Außerhalb von Windows ist die Ausgabe von Programmen UTF-8; was es nicht ist, wird ersetzt.
#[cfg(not(windows))]
fn als_utf8(roh: Vec<u8>) -> Vec<u8> {
    match String::from_utf8(roh) {
        Ok(text) => text.into_bytes(),
        Err(fehler) => String::from_utf8_lossy(fehler.as_bytes()).into_owned().into_bytes(),
    }
}

/// Startet ein Programm, gibt ihm `eingabe` zu lesen und wartet, bis es fertig
/// ist. Danach liegen Code und Ausgaben bereit (siehe die Funktionen darunter).
/// Gibt `false` zurück, wenn es sich gar nicht starten ließ.
///
/// # Safety
/// `programm` und `eingabe` müssen gültige Texte sein, `argumente` eine gültige Liste von Texten.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_programm_starten(programm: *const TextKopf, argumente: *const ListenKopf, eingabe: *const TextKopf) -> bool {
    let (programm, eingabe) = unsafe { (text_von(programm), bytes(eingabe).to_vec()) };
    let argumente: Vec<String> = unsafe {
        let l = &*argumente;
        std::slice::from_raw_parts(l.daten as *const *const TextKopf, l.laenge as usize).iter().map(|t| text_von(*t)).collect()
    };
    if programm.is_empty() || programm.contains('\0') || argumente.iter().any(|a| a.contains('\0')) {
        merke_meldung("So kaun a Programm ned haaßn.");
        return false;
    }
    super::leeren();
    let gestartet = Command::new(&programm)
        .args(&argumente)
        .stdin(if eingabe.is_empty() { Stdio::null() } else { Stdio::piped() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut kind = match gestartet {
        Ok(kind) => kind,
        Err(fehler) => {
            merke_fehler(&fehler);
            return false;
        }
    };
    // Die Eingabe schreibt ein eigener Faden: Sonst könnten beide Seiten aufeinander
    // warten (das Programm darauf, dass jemand seine Ausgabe liest, wir darauf, dass es weiterliest).
    let hinein = kind.stdin.take();
    let fertig = std::thread::scope(|faeden| {
        if let Some(mut hinein) = hinein {
            faeden.spawn(move || {
                let _ = hinein.write_all(&eingabe);
            });
        }
        kind.wait_with_output()
    });
    match fertig {
        Ok(ergebnis) => {
            LETZTER_LAUF.with(|l| {
                *l.borrow_mut() = Lauf {
                    code: ergebnis.status.code().map_or(-1, i64::from),
                    ausgabe: als_utf8(ergebnis.stdout),
                    fehler: als_utf8(ergebnis.stderr),
                }
            });
            true
        }
        Err(fehler) => {
            merke_fehler(&fehler);
            false
        }
    }
}

/// Mit welchem Code das zuletzt gestartete Programm geendet hat (0 heißt meistens: alles gut).
#[unsafe(no_mangle)]
pub extern "C" fn oida_programm_lauf_code() -> i64 {
    LETZTER_LAUF.with(|l| l.borrow().code)
}

/// Was das zuletzt gestartete Programm ausgegeben hat.
#[unsafe(no_mangle)]
pub extern "C" fn oida_programm_lauf_ausgabe() -> *mut TextKopf {
    LETZTER_LAUF.with(|l| neuer_text(&l.borrow().ausgabe))
}

/// Was das zuletzt gestartete Programm auf die Fehlerausgabe geschrieben hat.
#[unsafe(no_mangle)]
pub extern "C" fn oida_programm_lauf_fehler() -> *mut TextKopf {
    LETZTER_LAUF.with(|l| neuer_text(&l.borrow().fehler))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inhalt(text: *mut TextKopf) -> String {
        String::from_utf8(unsafe { bytes(text) }.to_vec()).unwrap()
    }

    #[test]
    fn umgebung_setzen_und_lesen() {
        let name = neuer_text(b"OIDA_PROBE_UMGEBUNG");
        unsafe {
            assert!(oida_programm_umgebung(name).is_null());
            assert!(oida_programm_umgebung_setzen(name, neuer_text("Grüß di".as_bytes())));
            assert_eq!(inhalt(oida_programm_umgebung(name)), "Grüß di");
            // Namen, die es nicht geben kann, bringen das Programm nicht um.
            assert!(oida_programm_umgebung(neuer_text(b"A=B")).is_null());
            assert!(oida_programm_umgebung(neuer_text(b"")).is_null());
            assert!(!oida_programm_umgebung_setzen(neuer_text(b"A=B"), neuer_text(b"x")));
        }
    }

    #[test]
    fn ordner_haben_einen_schraegstrich_am_ende() {
        for ordner in [oida_programm_arbeitsordner(), oida_programm_temp_ordner()] {
            let ordner = inhalt(ordner);
            assert!(ordner.ends_with('/') && !ordner.ends_with("//") && !ordner.contains('\\'), "{ordner}");
        }
    }

    #[test]
    fn ein_programm_starten_und_seine_ausgabe_lesen() {
        // Die Befehlszeile des Systems: unter Windows cmd, sonst sh.
        let (schale, schalter) = if cfg!(windows) { ("cmd", "/c") } else { ("sh", "-c") };
        let argumente = liste_aus_texten(&[schalter.to_string(), "echo Servas&& echo Na 1>&2&& exit 3".to_string()]);
        unsafe {
            assert!(oida_programm_starten(neuer_text(schale.as_bytes()), argumente, neuer_text(b"")));
        }
        assert_eq!(oida_programm_lauf_code(), 3);
        assert_eq!(inhalt(oida_programm_lauf_ausgabe()).trim(), "Servas");
        assert_eq!(inhalt(oida_programm_lauf_fehler()).trim(), "Na");
        // Mit Eingabe: "sort" gibt die Zeilen geordnet zurück.
        unsafe {
            assert!(oida_programm_starten(neuer_text(b"sort"), liste_aus_texten(&[]), neuer_text(b"zwa\r\nans\r\n")));
        }
        assert_eq!(inhalt(oida_programm_lauf_ausgabe()).replace('\r', ""), "ans\nzwa\n");
        // Ein Programm, das es nicht gibt, lässt sich nicht starten.
        unsafe {
            assert!(!oida_programm_starten(neuer_text(b"des_gibts_sicha_ned_4711"), liste_aus_texten(&[]), neuer_text(b"")));
        }
    }

    #[test]
    fn das_system_hat_einen_namen() {
        assert_eq!(inhalt(oida_programm_system()), if cfg!(windows) { "windows" } else { "linux" });
    }

    #[test]
    #[cfg(windows)]
    fn alte_zeichentabelle_wird_umgewandelt() {
        // 0x84 ist in den Tabellen 437 und 850 das "ä"; als UTF-8 wäre das Byte allein ungültig.
        let aus = String::from_utf8(als_utf8(vec![b'K', 0x84, b's'])).unwrap();
        assert_eq!(aus.chars().count(), 3);
        assert!(aus.starts_with('K') && aus.ends_with('s'), "{aus}");
        assert_eq!(als_utf8("Käs".as_bytes().to_vec()), "Käs".as_bytes());
    }
}
