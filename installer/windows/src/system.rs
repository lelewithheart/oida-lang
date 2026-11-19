//! Was der Installer direkt von Windows braucht: die Registry (für den PATH
//! des Benutzers und den Eintrag unter "Apps"), die Nachricht an laufende
//! Programme, dass sich die Umgebung geändert hat, und die Frage, ob das
//! Fenster nur für den Installer aufgegangen ist.

use std::ffi::c_void;

/// `HKEY_CURRENT_USER`: alles hier betrifft nur den angemeldeten Benutzer und braucht keine Admin-Rechte.
const HKEY_CURRENT_USER: isize = 0x8000_0001u32 as i32 as isize;
const KEY_READ: u32 = 0x20019;
const KEY_ALL_ACCESS: u32 = 0xF003F;
pub const REG_SZ: u32 = 1;
pub const REG_EXPAND_SZ: u32 = 2;
const REG_DWORD: u32 = 4;
const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_MORE_DATA: i32 = 234;

#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegCreateKeyExW(
        wurzel: isize,
        pfad: *const u16,
        reserviert: u32,
        klasse: *const u16,
        optionen: u32,
        rechte: u32,
        sicherheit: *const c_void,
        griff: *mut isize,
        neu: *mut u32,
    ) -> i32;
    fn RegOpenKeyExW(wurzel: isize, pfad: *const u16, optionen: u32, rechte: u32, griff: *mut isize) -> i32;
    fn RegQueryValueExW(griff: isize, name: *const u16, reserviert: *mut u32, art: *mut u32, daten: *mut u8, laenge: *mut u32) -> i32;
    fn RegSetValueExW(griff: isize, name: *const u16, reserviert: u32, art: u32, daten: *const u8, laenge: u32) -> i32;
    fn RegDeleteTreeW(wurzel: isize, pfad: *const u16) -> i32;
    fn RegDeleteKeyW(wurzel: isize, pfad: *const u16) -> i32;
    fn RegCloseKey(griff: isize) -> i32;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn SendMessageTimeoutW(fenster: isize, nachricht: u32, w: usize, l: isize, art: u32, warten_ms: u32, ergebnis: *mut usize) -> isize;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetConsoleProcessList(liste: *mut u32, platz: u32) -> u32;
}

/// Text, wie Windows ihn will: UTF-16 mit einer Null am Ende.
fn breit(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Ein offener Schlüssel unter `HKEY_CURRENT_USER`.
pub struct Schluessel(isize);

impl Schluessel {
    /// Öffnet den Schlüssel zum Lesen und Schreiben und legt ihn an, wenn es ihn nicht gibt.
    pub fn anlegen(pfad: &str) -> Result<Schluessel, String> {
        let mut griff = 0isize;
        let code = unsafe {
            RegCreateKeyExW(HKEY_CURRENT_USER, breit(pfad).as_ptr(), 0, std::ptr::null(), 0, KEY_ALL_ACCESS, std::ptr::null(), &mut griff, std::ptr::null_mut())
        };
        if code == 0 { Ok(Schluessel(griff)) } else { Err(format!("An {pfad} in da Registry kumm i ned zuwe (Fehler {code}).")) }
    }

    /// Öffnet den Schlüssel nur zum Lesen; `None`, wenn es ihn nicht gibt.
    pub fn lesen(pfad: &str) -> Option<Schluessel> {
        let mut griff = 0isize;
        let code = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, breit(pfad).as_ptr(), 0, KEY_READ, &mut griff) };
        (code == 0).then_some(Schluessel(griff))
    }

    /// Ein Text-Wert samt seiner Art (`REG_SZ` oder `REG_EXPAND_SZ`); `Ok(None)`, wenn es ihn
    /// nicht gibt. Gibt es ihn, aber er lässt sich nicht als Text lesen, ist das ein Fehler:
    /// Wer dann weitermachte, als wäre nichts da, würde den Wert überschreiben (etwa den PATH).
    pub fn text(&self, name: &str) -> Result<Option<(String, u32)>, String> {
        let unlesbar = |grund: String| format!("Den Wert {name} in da Registry kaun i ned lesn ({grund}). I loss eam, wia er is.");
        let breit_name = breit(name);
        let (mut art, mut laenge) = (0u32, 0u32);
        unsafe {
            let code =
                RegQueryValueExW(self.0, breit_name.as_ptr(), std::ptr::null_mut(), &mut art, std::ptr::null_mut(), &mut laenge);
            if code == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if code != 0 && code != ERROR_MORE_DATA {
                return Err(unlesbar(format!("Fehler {code}")));
            }
            if art != REG_SZ && art != REG_EXPAND_SZ {
                return Err(unlesbar("des is ka Text".to_string()));
            }
            let mut daten = vec![0u16; (laenge as usize).div_ceil(2) + 1];
            let mut platz = (daten.len() * 2) as u32;
            let code =
                RegQueryValueExW(self.0, breit_name.as_ptr(), std::ptr::null_mut(), &mut art, daten.as_mut_ptr().cast(), &mut platz);
            if code != 0 {
                return Err(unlesbar(format!("Fehler {code}")));
            }
            daten.truncate((platz as usize) / 2);
            // Die Null am Ende gehört nicht zum Wert (und kann fehlen).
            while daten.last() == Some(&0) {
                daten.pop();
            }
            Ok(Some((String::from_utf16_lossy(&daten), art)))
        }
    }

    pub fn setz_text(&self, name: &str, wert: &str, art: u32) -> Result<(), String> {
        let daten = breit(wert);
        let code = unsafe { RegSetValueExW(self.0, breit(name).as_ptr(), 0, art, daten.as_ptr().cast(), (daten.len() * 2) as u32) };
        if code == 0 { Ok(()) } else { Err(format!("Den Wert {name} kaun i ned in de Registry schreibn (Fehler {code}).")) }
    }

    pub fn setz_zahl(&self, name: &str, wert: u32) -> Result<(), String> {
        let code = unsafe { RegSetValueExW(self.0, breit(name).as_ptr(), 0, REG_DWORD, (&raw const wert).cast(), 4) };
        if code == 0 { Ok(()) } else { Err(format!("Den Wert {name} kaun i ned in de Registry schreibn (Fehler {code}).")) }
    }
}

impl Drop for Schluessel {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

/// Löscht einen Schlüssel mit allem darunter. Dass es ihn gar nicht gibt, ist kein Fehler.
pub fn schluessel_loeschen(pfad: &str) {
    let pfad = breit(pfad);
    unsafe {
        RegDeleteTreeW(HKEY_CURRENT_USER, pfad.as_ptr());
        RegDeleteKeyW(HKEY_CURRENT_USER, pfad.as_ptr());
    }
}

/// Sagt allen laufenden Programmen (vor allem dem Explorer), dass sich die
/// Umgebung geändert hat: Ein danach geöffnetes Terminal kennt den neuen PATH.
pub fn umgebung_melden() {
    const HWND_BROADCAST: isize = 0xFFFF;
    const WM_SETTINGCHANGE: u32 = 0x001A;
    const SMTO_ABORTIFHUNG: u32 = 0x0002;
    let was = breit("Environment");
    let mut ergebnis = 0usize;
    unsafe {
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, was.as_ptr() as isize, SMTO_ABORTIFHUNG, 3000, &mut ergebnis);
    }
}

/// Ist das Konsolenfenster nur für dieses Programm aufgegangen (Doppelklick im
/// Explorer)? Dann würde es am Ende sofort verschwinden, bevor man etwas lesen kann.
pub fn eigenes_fenster() -> bool {
    let mut liste = [0u32; 4];
    unsafe { GetConsoleProcessList(liste.as_mut_ptr(), liste.len() as u32) == 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn werte_schreiben_und_lesen() {
        // Ein eigener Schlüssel nur für diesen Test; am Ende ist er wieder weg.
        let pfad = format!("Software\\OIDA-Setup-Test-{}\\system", std::process::id());
        let oben = format!("Software\\OIDA-Setup-Test-{}", std::process::id());
        assert!(Schluessel::lesen(&pfad).is_none());
        {
            let schluessel = Schluessel::anlegen(&pfad).unwrap();
            assert_eq!(schluessel.text("Path"), Ok(None));
            schluessel.setz_text("Path", "C:\\a;%USERPROFILE%\\ö", REG_EXPAND_SZ).unwrap();
            assert_eq!(schluessel.text("Path"), Ok(Some(("C:\\a;%USERPROFILE%\\ö".to_string(), REG_EXPAND_SZ))));
            schluessel.setz_text("Laar", "", REG_SZ).unwrap();
            assert_eq!(schluessel.text("Laar"), Ok(Some((String::new(), REG_SZ))));
            schluessel.setz_zahl("Zoi", 7).unwrap();
            // Eine Zahl ist kein Text: Das ist ein Fehler, nicht "gibt es nicht".
            assert!(schluessel.text("Zoi").is_err());
        }
        assert_eq!(Schluessel::lesen(&pfad).unwrap().text("Path").unwrap().unwrap().0, "C:\\a;%USERPROFILE%\\ö");
        schluessel_loeschen(&oben);
        assert!(Schluessel::lesen(&pfad).is_none() && Schluessel::lesen(&oben).is_none());
        // Noch einmal löschen schadet nicht.
        schluessel_loeschen(&oben);
    }
}
