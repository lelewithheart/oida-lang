//! Das Paket, das im Installer steckt: alle Dateien hintereinander, als Ganzes
//! gepackt. Dieselbe Datei verwenden `build.rs` (packt beim Bauen) und der
//! Installer selbst (packt beim Installieren aus).
//!
//! Aufbau, alle Zahlen mit dem kleinsten Byte zuerst:
//!
//! ```text
//! "OIDAPAK1"   8 Bytes Kennung
//! Verfahren    1 Byte: 0 = ungepackt, 5 = LZMS (das Packen von Windows)
//! Länge        8 Bytes: so viele Bytes sind es ausgepackt
//! Daten        bis zum Ende; ausgepackt je Datei:
//!                4 Bytes Länge des Namens, der Name (UTF-8, mit "/"),
//!                8 Bytes Länge des Inhalts, der Inhalt
//! ```

pub const KENNUNG: &[u8; 8] = b"OIDAPAK1";
const UNGEPACKT: u8 = 0;
#[cfg(windows)]
const LZMS: u8 = 5;

#[derive(Debug, PartialEq)]
pub struct Eintrag {
    /// Wohin die Datei kommt, vom Zielordner aus, mit "/" als Trenner.
    pub name: String,
    pub inhalt: Vec<u8>,
}

/// Alle Dateien hintereinander (noch nicht gepackt).
pub fn zusammen(eintraege: &[Eintrag]) -> Vec<u8> {
    let mut roh = Vec::new();
    for eintrag in eintraege {
        roh.extend_from_slice(&(eintrag.name.len() as u32).to_le_bytes());
        roh.extend_from_slice(eintrag.name.as_bytes());
        roh.extend_from_slice(&(eintrag.inhalt.len() as u64).to_le_bytes());
        roh.extend_from_slice(&eintrag.inhalt);
    }
    roh
}

/// Das Gegenstück zu [`zusammen`]. Ein Name, der aus dem Zielordner hinausführen
/// würde ("..", ein Laufwerk, ein "/" am Anfang), gilt als kaputtes Paket.
pub fn zerlegen(roh: &[u8]) -> Result<Vec<Eintrag>, String> {
    let kaputt = || "Des Paket im Installer is kaputt.".to_string();
    let mut eintraege = Vec::new();
    let mut stelle = 0usize;
    let mut nimm = |anzahl: usize| -> Result<&[u8], String> {
        let ende = stelle.checked_add(anzahl).filter(|e| *e <= roh.len()).ok_or_else(kaputt)?;
        let stueck = &roh[stelle..ende];
        stelle = ende;
        Ok(stueck)
    };
    loop {
        let Ok(laenge) = nimm(4) else { break };
        let laenge = u32::from_le_bytes(laenge.try_into().unwrap()) as usize;
        let name = String::from_utf8(nimm(laenge)?.to_vec()).map_err(|_| kaputt())?;
        let laenge = u64::from_le_bytes(nimm(8)?.try_into().unwrap()) as usize;
        let inhalt = nimm(laenge)?.to_vec();
        let harmlos = !name.is_empty()
            && !name.starts_with('/')
            && !name.contains(['\\', ':', '\0'])
            && name.split('/').all(|teil| !teil.is_empty() && teil != "." && teil != "..");
        if !harmlos {
            return Err(kaputt());
        }
        eintraege.push(Eintrag { name, inhalt });
    }
    // Bleibt am Ende etwas übrig, das kein ganzer Eintrag ist, stimmt etwas nicht.
    if stelle != roh.len() {
        return Err(kaputt());
    }
    Ok(eintraege)
}

/// Macht aus den rohen Daten das Paket: Kennung, Verfahren, Länge, Daten.
pub fn packen(roh: &[u8]) -> Vec<u8> {
    let (verfahren, daten) = gepackt(roh);
    let mut paket = Vec::with_capacity(daten.len() + 17);
    paket.extend_from_slice(KENNUNG);
    paket.push(verfahren);
    paket.extend_from_slice(&(roh.len() as u64).to_le_bytes());
    paket.extend_from_slice(&daten);
    paket
}

/// Das Gegenstück zu [`packen`]: die rohen Daten aus dem Paket.
pub fn auspacken(paket: &[u8]) -> Result<Vec<u8>, String> {
    let kaputt = || "Des Paket im Installer is kaputt.".to_string();
    if paket.len() < 17 || &paket[..8] != KENNUNG {
        return Err(kaputt());
    }
    let laenge = u64::from_le_bytes(paket[9..17].try_into().unwrap()) as usize;
    let daten = &paket[17..];
    let roh = match paket[8] {
        UNGEPACKT => daten.to_vec(),
        #[cfg(windows)]
        LZMS => windows::auspacken(daten, laenge).ok_or_else(kaputt)?,
        _ => return Err(kaputt()),
    };
    if roh.len() != laenge {
        return Err(kaputt());
    }
    Ok(roh)
}

#[cfg(windows)]
fn gepackt(roh: &[u8]) -> (u8, Vec<u8>) {
    // Geht das Packen nicht (sehr altes Windows), kommt das Paket eben ungepackt hinein.
    match windows::packen(roh) {
        Some(daten) if daten.len() < roh.len() => (LZMS, daten),
        _ => (UNGEPACKT, roh.to_vec()),
    }
}

#[cfg(not(windows))]
fn gepackt(roh: &[u8]) -> (u8, Vec<u8>) {
    (UNGEPACKT, roh.to_vec())
}

/// Das Packen, das Windows seit Windows 8 mitbringt (cabinet.dll, compressapi.h).
#[cfg(windows)]
mod windows {
    use std::ffi::c_void;

    const COMPRESS_ALGORITHM_LZMS: u32 = 5;

    #[link(name = "cabinet")]
    unsafe extern "system" {
        fn CreateCompressor(verfahren: u32, speicher: *const c_void, griff: *mut isize) -> i32;
        fn Compress(griff: isize, roh: *const c_void, roh_laenge: usize, ziel: *mut c_void, platz: usize, laenge: *mut usize) -> i32;
        fn CloseCompressor(griff: isize) -> i32;
        fn CreateDecompressor(verfahren: u32, speicher: *const c_void, griff: *mut isize) -> i32;
        fn Decompress(griff: isize, gepackt: *const c_void, gepackt_laenge: usize, ziel: *mut c_void, platz: usize, laenge: *mut usize) -> i32;
        fn CloseDecompressor(griff: isize) -> i32;
    }

    pub fn packen(roh: &[u8]) -> Option<Vec<u8>> {
        unsafe {
            let mut griff = 0isize;
            if CreateCompressor(COMPRESS_ALGORITHM_LZMS, std::ptr::null(), &mut griff) == 0 {
                return None;
            }
            // Gepackt wird es nie viel größer als roh; ein wenig Luft für den Fall, dass sich nichts packen lässt.
            let mut ziel = vec![0u8; roh.len() + roh.len() / 8 + 4096];
            let mut laenge = 0usize;
            let geklappt = Compress(griff, roh.as_ptr().cast(), roh.len(), ziel.as_mut_ptr().cast(), ziel.len(), &mut laenge);
            CloseCompressor(griff);
            if geklappt == 0 {
                return None;
            }
            ziel.truncate(laenge);
            Some(ziel)
        }
    }

    pub fn auspacken(gepackt: &[u8], roh_laenge: usize) -> Option<Vec<u8>> {
        unsafe {
            let mut griff = 0isize;
            if CreateDecompressor(COMPRESS_ALGORITHM_LZMS, std::ptr::null(), &mut griff) == 0 {
                return None;
            }
            let mut ziel = vec![0u8; roh_laenge];
            let mut laenge = 0usize;
            let geklappt =
                Decompress(griff, gepackt.as_ptr().cast(), gepackt.len(), ziel.as_mut_ptr().cast(), ziel.len(), &mut laenge);
            CloseDecompressor(griff);
            if geklappt == 0 {
                return None;
            }
            ziel.truncate(laenge);
            Some(ziel)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eintrag(name: &str, inhalt: &[u8]) -> Eintrag {
        Eintrag { name: name.to_string(), inhalt: inhalt.to_vec() }
    }

    #[test]
    fn hin_und_zurueck() {
        let viel: Vec<u8> = (0..200_000u32).flat_map(|i| (i % 251).to_le_bytes()).collect();
        let eintraege = vec![eintrag("oida.exe", &viel), eintrag("beispiele/hallo.oida", "SCHREIB(\"Grüß di\") OIDA\n".as_bytes()), eintrag("laar.txt", b"")];
        let roh = zusammen(&eintraege);
        let paket = packen(&roh);
        // Unter Windows wird wirklich gepackt: Das Paket ist deutlich kleiner.
        #[cfg(windows)]
        assert!(paket[8] == 5 && paket.len() < roh.len() / 4, "{} von {}", paket.len(), roh.len());
        assert_eq!(zerlegen(&auspacken(&paket).unwrap()).unwrap(), eintraege);
        assert_eq!(zerlegen(&[]).unwrap(), Vec::new());
    }

    #[test]
    fn kaputtes_wird_erkannt() {
        let paket = packen(&zusammen(&[eintrag("a.txt", b"servas servas servas servas servas")]));
        assert!(auspacken(&paket[..paket.len() - 3]).is_err());
        assert!(auspacken(b"wos aundas").is_err());
        let mut falsch = paket.clone();
        falsch[0] = b'X';
        assert!(auspacken(&falsch).is_err());
        // Ein abgeschnittener Eintrag.
        let roh = zusammen(&[eintrag("a.txt", b"servas")]);
        assert!(zerlegen(&roh[..roh.len() - 1]).is_err());
    }

    #[test]
    fn namen_die_hinausfuehren_gelten_nicht() {
        for name in ["../weg.txt", "a/../../weg.txt", "/ganz/vorn", "C:/x", "a\\b", "a//b", ""] {
            assert!(zerlegen(&zusammen(&[eintrag(name, b"x")])).is_err(), "{name}");
        }
    }
}
