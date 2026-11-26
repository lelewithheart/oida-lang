# OIDA für VS Code

Damit schreibt und startet man [OIDA](https://github.com/lelewithheart)-Programme in VS Code.

## Was die Erweiterung kann

- **Einfärben** von `.oida`-Dateien: Wörter der Sprache, Typen (auch eigene Strukturen und Klassen), Funktionen und Methoden, Variablen, Felder, Zahlen, Texte, Kommentare.
- **Fehler beim Tippen**: rot unterstrichen, mit der Meldung vom Compiler (`Heast, wo is des OIDA?`).
- **Starten mit F5** (oder dem Pfeil rechts oben): übersetzt das Programm und lässt es unten im Terminal laufen.
- **Übersetzen mit Strg+Umschalt+B**: legt die `.exe` neben die `.oida`-Datei.
- **Vorschläge** für die Wörter der Sprache und **Schnipsel** (`wenn`, `wennsunst`, `solong`, `fia`, `fiajede`, `struktur`, `funktiaun`, `sogau`, `schreib`).
- **Vorschläge für das eigene Programm:** Funktionen (mit ihren Parametern zum Überschreiben und dem Kommentar, der über ihnen steht), Klassen, Strukturen, Aufzählungen, Variablen und feste Werte, auch aus Dateien und Teilen der Bibliothek, die mit `BRAUCH` dazugeholt sind. Hinter einem Punkt kommen genau die Felder und Methoden von dem, was davor steht, samt den geerbten (`hund.ort.` zeigt die Felder des Punkts, `MEI.` die der eigenen Klasse), hinter dem Namen einer Aufzählung ihre Werte (`Ampl.` zeigt `ROT`, `GRÜN`). Variablen kommen nur dort, wo sie gelten: die einer Funktion nur in ihr, die des Hauptprogramms nicht in Funktionen. Das geht auch, während die Zeile noch unfertig ist.
- **Groß oder klein:** Der Compiler nimmt `WENN` genauso wie `wenn`. Vorschläge und Schnipsel richten sich danach, wie man tippt: Wer klein tippt, bekommt `wenn ... daun`. Mit der Einstellung `oida.schreibweise` (`wie getippt`, `GROSS`, `klein`) lässt es sich festlegen.
- **Einrücken** nach `DAUN`, `TUA`, `SUNST` und `FUNKTIAUN`, zurück bei `PASST SCHO`.
- Eingebaute Funktionen wie `LÄNGN(...)` werden als Funktionen eingefärbt, aber nur mit Klammer.
- Kommentare mit Strg+# ein- und ausschalten.

Alles richtet sich nach dem Wörterbuch: Die Erweiterung fragt bei jedem Schritt den Compiler.
Wer in `woerterbuch.txt` ein Wort ändert und speichert, sieht es sofort im Editor.

## Befehle (Strg+Umschalt+P)

| Befehl                                   | Taste              |
|------------------------------------------|--------------------|
| OIDA: Programm starten                   | F5                 |
| OIDA: Programm übersetzen (.exe erzeugen)| Strg+Umschalt+B    |
| OIDA: Wörterbuch öffnen                  |                    |
| OIDA: Compiler (oida.exe) aussuchen      |                    |

## Einstellungen

| Einstellung        | Bedeutung                                                               |
|--------------------|-------------------------------------------------------------------------|
| `oida.compiler`    | Pfad zur `oida.exe`. Leer: die Erweiterung sucht selbst (siehe unten).   |
| `oida.woerterbuch` | Pfad zu einem eigenen Wörterbuch. Leer: das vom Compiler.               |

Die `oida.exe` wird in dieser Reihenfolge gesucht: Einstellung `oida.compiler`, im geöffneten Ordner
unter `target\release` und `target\debug`, dort, wo sie beim Bauen der Erweiterung lag, im `PATH`.

## Bauen und installieren

Im Ordner `vscode` des OIDA-Projekts `installieren.bat` doppelklicken. Das baut den Compiler,
packt die Erweiterung als `oida-vscode.vsix` und installiert sie in VS Code.

Von Hand:

```
powershell -ExecutionPolicy Bypass -File vscode\bauen.ps1 -Installieren
```

Die Dateien `language-configuration.json`, `syntaxes\oida.tmLanguage.json` und `snippets\oida.json`
werden dabei aus `woerterbuch.txt` erzeugt (`oida --editor-dateien vscode`); nicht von Hand ändern.

## Testen

```
powershell -ExecutionPolicy Bypass -File vscode\testen.ps1
```

Es geht kurz ein eigenes VS-Code-Fenster auf, in dem die Prüfungen aus `test\lauf.js` laufen
(Fehler, Farben, Vorschläge, Starten). Mit `-Vsix` wird die fertig gepackte Erweiterung getestet.
