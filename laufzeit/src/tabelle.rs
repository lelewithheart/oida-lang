//! Nummerierte Plätze für das, was ein Programm offen hat: Dateien und Verbindungen.
//!
//! Die Nummer, die ein OIDA-Programm in der Hand hat, ist die Stelle in so einer
//! Tabelle. Es gibt jede Tabelle einmal für das ganze Programm: Eine Verbindung,
//! die das Hauptprogramm angenommen hat, kann es so einem Thread mitgeben
//! (`NEBNBEI bedien(gast)`), und dort gilt dieselbe Nummer.
//!
//! Verwenden kann einen Platz immer nur ein Thread auf einmal: Wer etwas damit
//! tut, leiht es sich aus der Tabelle und stellt es danach zurück. So wartet beim
//! Lesen von einer Verbindung nur der eine Thread und nicht jeder, der gerade
//! irgendeine Datei oder Verbindung braucht.

use std::sync::{Mutex, MutexGuard};

enum Platz<T> {
    Leer,
    Da(T),
    /// Ein Thread arbeitet gerade damit.
    Verliehen,
    /// ... und inzwischen hat ein anderer zugemacht: Beim Zurückstellen wird es weggeworfen.
    VerliehenUndZu,
}

pub(crate) struct Tabelle<T> {
    plaetze: Mutex<Vec<Platz<T>>>,
    /// Darf ein frei gewordener Platz wieder vergeben werden? Bei Verbindungen nicht: Wer nach
    /// dem Zumachen noch schreibt, erwischt so nie die Verbindung von jemand anderem.
    wieder: bool,
}

/// Was beim Ausleihen herauskommt.
pub(crate) enum Geliehen<R> {
    Fertig(R),
    /// Den Platz gibt es nicht (mehr).
    Zu,
    /// Ein anderer Thread verwendet ihn gerade.
    Besetzt,
}

impl<T> Tabelle<T> {
    pub(crate) const fn neu(wieder: bool) -> Tabelle<T> {
        Tabelle { plaetze: Mutex::new(Vec::new()), wieder }
    }

    fn sperre(&self) -> MutexGuard<'_, Vec<Platz<T>>> {
        self.plaetze.lock().unwrap_or_else(|vergiftet| vergiftet.into_inner())
    }

    /// Legt etwas ab und gibt die Nummer des Platzes zurück.
    pub(crate) fn ablegen(&self, was: T) -> i64 {
        let mut plaetze = self.sperre();
        let frei = if self.wieder { plaetze.iter().position(|p| matches!(p, Platz::Leer)) } else { None };
        let stelle = frei.unwrap_or_else(|| {
            plaetze.push(Platz::Leer);
            plaetze.len() - 1
        });
        plaetze[stelle] = Platz::Da(was);
        stelle as i64
    }

    /// Tut etwas mit dem, was auf dem Platz liegt. Solange das dauert, ist der Platz
    /// verliehen, die Tabelle selbst aber frei für alle anderen.
    pub(crate) fn mit<R>(&self, nummer: i64, tu: impl FnOnce(&mut T) -> R) -> Geliehen<R> {
        let Ok(stelle) = usize::try_from(nummer) else { return Geliehen::Zu };
        let mut ding = {
            let mut plaetze = self.sperre();
            match plaetze.get_mut(stelle) {
                Some(platz @ Platz::Da(_)) => match std::mem::replace(platz, Platz::Verliehen) {
                    Platz::Da(ding) => ding,
                    _ => unreachable!(),
                },
                Some(Platz::Verliehen | Platz::VerliehenUndZu) => return Geliehen::Besetzt,
                _ => return Geliehen::Zu,
            }
        };
        let ergebnis = tu(&mut ding);
        let mut plaetze = self.sperre();
        match plaetze[stelle] {
            Platz::Verliehen => plaetze[stelle] = Platz::Da(ding),
            // Inzwischen zugemacht: `ding` wird hier weggeworfen und damit geschlossen.
            _ => plaetze[stelle] = Platz::Leer,
        }
        Geliehen::Fertig(ergebnis)
    }

    /// Macht den Platz frei und gibt zurück, was dort lag. Arbeitet gerade ein anderer
    /// Thread damit, kommt nichts zurück; es wird weggeworfen, sobald der fertig ist.
    pub(crate) fn nimm(&self, nummer: i64) -> Option<T> {
        let stelle = usize::try_from(nummer).ok()?;
        let mut plaetze = self.sperre();
        let platz = plaetze.get_mut(stelle)?;
        match std::mem::replace(platz, Platz::Leer) {
            Platz::Da(ding) => Some(ding),
            Platz::Verliehen | Platz::VerliehenUndZu => {
                *platz = Platz::VerliehenUndZu;
                None
            }
            Platz::Leer => None,
        }
    }

    /// Alles, was noch da liegt (am Ende des Programms).
    pub(crate) fn alles(&self) -> Vec<T> {
        let mut plaetze = self.sperre();
        let mut aus = Vec::new();
        for platz in plaetze.iter_mut() {
            if let Platz::Da(ding) = std::mem::replace(platz, Platz::Leer) {
                aus.push(ding);
            }
        }
        aus
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ablegen_ausleihen_und_zumachen() {
        let tabelle: Tabelle<String> = Tabelle::neu(true);
        let a = tabelle.ablegen("ans".to_string());
        let b = tabelle.ablegen("zwa".to_string());
        assert_eq!((a, b), (0, 1));
        assert!(matches!(tabelle.mit(a, |t| t.push('!')), Geliehen::Fertig(())));
        assert!(matches!(tabelle.mit(a, |t| t.clone()), Geliehen::Fertig(t) if t == "ans!"));
        assert!(matches!(tabelle.mit(7, |_| ()), Geliehen::Zu));
        assert!(matches!(tabelle.mit(-1, |_| ()), Geliehen::Zu));
        assert_eq!(tabelle.nimm(a).as_deref(), Some("ans!"));
        assert_eq!(tabelle.nimm(a), None);
        assert!(matches!(tabelle.mit(a, |_| ()), Geliehen::Zu));
        // Der freie Platz wird wieder vergeben.
        assert_eq!(tabelle.ablegen("drei".to_string()), 0);
        assert_eq!(tabelle.alles(), ["drei", "zwa"]);
        assert!(tabelle.alles().is_empty());
    }

    #[test]
    fn ohne_wiederverwenden_bleibt_ein_platz_leer() {
        let tabelle: Tabelle<i32> = Tabelle::neu(false);
        assert_eq!(tabelle.ablegen(1), 0);
        assert_eq!(tabelle.nimm(0), Some(1));
        assert_eq!(tabelle.ablegen(2), 1);
        assert!(matches!(tabelle.mit(0, |_| ()), Geliehen::Zu));
    }

    #[test]
    fn was_verliehen_ist_bekommt_kein_zweiter() {
        let tabelle: Tabelle<i32> = Tabelle::neu(false);
        let nummer = tabelle.ablegen(5);
        let aussen = tabelle.mit(nummer, |wert| {
            // Während der eine damit arbeitet, kommt ein anderer nicht dran ...
            assert!(matches!(tabelle.mit(nummer, |_| ()), Geliehen::Besetzt));
            // ... andere Plätze gehen aber.
            let anderer = tabelle.ablegen(6);
            assert!(matches!(tabelle.mit(anderer, |w| *w), Geliehen::Fertig(6)));
            *wert += 1;
        });
        assert!(matches!(aussen, Geliehen::Fertig(())));
        assert!(matches!(tabelle.mit(nummer, |w| *w), Geliehen::Fertig(6)));
        // Zumachen, während es verliehen ist: Danach ist der Platz leer.
        let aussen = tabelle.mit(nummer, |_| assert_eq!(tabelle.nimm(nummer), None));
        assert!(matches!(aussen, Geliehen::Fertig(())));
        assert!(matches!(tabelle.mit(nummer, |_| ()), Geliehen::Zu));
    }
}
