//! Fehlermeldungen mit Datei, Zeile und Spalte.

use std::fmt::Write;

/// Eine Stelle im Quelltext. Zeile und Spalte fangen bei 1 an.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub zeile: u32,
    pub spalte: u32,
    /// In welcher Datei: die Nummer in der Liste der Quellen
    /// (`treiber::Quelle`). 0 ist die Datei, die übersetzt wird.
    pub datei: u16,
}

impl Position {
    pub fn neu(zeile: u32, spalte: u32) -> Position {
        Position { zeile, spalte, datei: 0 }
    }
}

/// Ein Fehler im OIDA-Programm.
#[derive(Debug, Clone, PartialEq)]
pub struct Fehler {
    pub pos: Position,
    pub meldung: String,
}

impl Fehler {
    pub fn neu(pos: Position, meldung: impl Into<String>) -> Fehler {
        Fehler { pos, meldung: meldung.into() }
    }

    /// Die Meldung samt der betroffenen Zeile und einem Pfeil auf die Stelle:
    ///
    /// ```text
    /// beispiel.oida:3:13: Heast, wo is des OIDA?
    ///  3 | SOG AU x = 5
    ///    |             ^
    /// ```
    pub fn anzeigen(&self, datei: &str, quelltext: &str) -> String {
        let mut aus = format!("{}:{}:{}: {}\n", datei, self.pos.zeile, self.pos.spalte, self.meldung);
        let zeile = quelltext.lines().nth(self.pos.zeile.saturating_sub(1) as usize);
        if let Some(zeile) = zeile {
            let nummer = self.pos.zeile.to_string();
            let zeile = zeile.trim_start_matches('\u{feff}').replace('\t', " ");
            let _ = writeln!(aus, " {nummer} | {zeile}");
            let _ = writeln!(
                aus,
                " {} | {}^",
                " ".repeat(nummer.len()),
                " ".repeat(self.pos.spalte.saturating_sub(1) as usize)
            );
        }
        aus
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anzeige_mit_pfeil() {
        let f = Fehler::neu(Position::neu(2, 13), "Heast, wo is des OIDA?");
        let text = f.anzeigen("a.oida", "# Kommentar\nSOG AU x = 5\n");
        assert_eq!(text, "a.oida:2:13: Heast, wo is des OIDA?\n 2 | SOG AU x = 5\n   |             ^\n");
    }
}
