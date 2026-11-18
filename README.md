# OIDA

Eine Programmiersprache im österreichischen Dialekt, die zu echtem Maschinencode übersetzt wird.
Jede Anweisung hört mit `OIDA` auf.

```
# Fakultät, rekursiv gerechnet

FUNKTIAUN fakultaet(n ZOI) GIBT ZRUCK ZOI
    WENN n <= 1 DAUN
        GIB ZRUCK 1 OIDA
    PASST SCHO
    GIB ZRUCK n * fakultaet(n - 1) OIDA
PASST SCHO

SOG AU i = 1 OIDA
SOLONG i <= 10 TUA
    SCHREIB(KETTN(i) + "! =", fakultaet(i)) OIDA
    i = i + 1 OIDA
PASST SCHO
```

```
> oida beispiele\fakultaet.oida
Passt! beispiele\fakultaet.exe is fertig.
> beispiele\fakultaet.exe
1! = 1
2! = 2
...
10! = 3628800
```

Der Compiler heißt `oida`, ist in Rust geschrieben und erzeugt über LLVM eine normale Windows-`.exe`
(rund 300 KB, läuft ohne zusätzliche DLLs). OIDA ist die große Schwester von
[OIDA-SQL](https://github.com/lelewithheart/oida-sql).

**Stand: v0.1, das Grundgerüst.** Was es schon kann und was noch fehlt, steht unten im [Fahrplan](#fahrplan).
Derzeit nur für Windows (64 Bit).


## Was man braucht

Zwei Dinge, kein Visual Studio:

1. **Rust** mit der Toolchain `x86_64-pc-windows-gnullvm`
2. **llvm-mingw** (bringt `clang` mit, das aus dem LLVM-Code die `.exe` macht)

Einrichten:

```
winget install Rustlang.Rustup
rustup default stable-x86_64-pc-windows-gnullvm
```

Dann von <https://github.com/mstorsjo/llvm-mingw/releases> die Datei
`llvm-mingw-...-ucrt-x86_64.zip` herunterladen und nach
`%LOCALAPPDATA%\Programs\llvm-mingw` entpacken (so, dass dort der Ordner `bin` liegt).
Dort sucht `oida` von selbst nach `clang`.

Damit auch Rust ohne Visual Studio linken kann, kommt in die Datei `%USERPROFILE%\.cargo\config.toml`
(den Pfad an den eigenen Benutzernamen anpassen):

```toml
[target.x86_64-pc-windows-gnullvm]
linker = "C:/Users/DEINNAME/AppData/Local/Programs/llvm-mingw/bin/x86_64-w64-mingw32-clang.exe"
```

Liegt llvm-mingw woanders, entweder den `bin`-Ordner in den `PATH` geben oder die
Umgebungsvariable `OIDA_CLANG` auf die `clang.exe` setzen.


## Bauen

Im Projektordner:

```
cargo build --release
```

Danach liegt der Compiler in `target\release\oida.exe`. Die Laufzeitbibliothek wird dabei
mitgebaut und steckt in der `oida.exe` drin; man braucht also nur diese eine Datei
(plus llvm-mingw).

Die Tests:

```
cargo test
```


## Ein Programm übersetzen

```
target\release\oida beispiele\fizzbuzz.oida
beispiele\fizzbuzz.exe
```

Oder in einem Schritt übersetzen und gleich starten:

```
target\release\oida beispiele\fizzbuzz.oida --renn
```

| Option                  | Bedeutung                                                    |
|-------------------------|--------------------------------------------------------------|
| `-o <datei.exe>`        | so soll das Programm heißen (sonst wie die `.oida`-Datei)    |
| `--renn`                | das Programm nach dem Übersetzen gleich starten              |
| `--ir`                  | das LLVM-IR als `.ll`-Datei daneben speichern (zum Anschauen)|
| `--ohne-optimierung`    | schneller übersetzen, dafür ein langsameres Programm         |
| `--woerterbuch <datei>` | dieses Wörterbuch verwenden                                  |
| `--woerter`             | die Wörter der Sprache anzeigen                              |
| `--hilfe`, `--version`  |                                                              |

`.oida`-Dateien müssen als UTF-8 gespeichert sein.


## Die Sprache (v0.1)

### Typen

| Typ        | Bedeutung             | Beispiel             |
|------------|-----------------------|----------------------|
| `ZOI`      | ganze Zahl, 64 Bit    | `17`, `-3`           |
| `KOMMAZOI` | Kommazahl, 64 Bit     | `3.14` (mit Punkt)   |
| `JONA`     | Wahrheitswert         | `WOAHR`, `FOISCH`    |
| `KETTN`    | Text                  | `"Servas"`, `'Oida'` |

Die Typen sind fest, aber man muss sie fast nie hinschreiben: Der Compiler erkennt sie am Wert.

### Variablen

```
SOG AU alter = 17 OIDA            # alter ist eine ZOI
SOG AU name = "Franz" OIDA        # name ist eine KETTN
SOG AU schnitt KOMMAZOI = 2 OIDA  # Typ dazuschreiben geht auch: aus 2 wird 2.0
alter = alter + 1 OIDA
```

Eine Variable behält ihren Typ. `alter = "siebzehn" OIDA` ist ein Fehler.
Eine Variable gilt nur in dem Block (`WENN`, `SOLONG`, Funktion), in dem sie angesagt wurde.

### Rechnen und Vergleichen

- Rechnen: `+  -  *  /  %` (Rest). Trifft eine `ZOI` auf eine `KOMMAZOI`, wird mit Kommazahlen gerechnet.
  `7 / 2` ist `3` (ganze Zahlen), `7 / 2.0` ist `3.5`.
- Texte zusammenhängen: `"Servas " + name`
- Vergleichen: `=` oder `==`, `!=` oder `<>`, `<`, `<=`, `>`, `>=`
- Verknüpfen: `UND`, `ODA`, `NED`. Die rechte Seite wird nur ausgerechnet, wenn sie gebraucht wird.
- Umwandeln: `ZOI(...)`, `KOMMAZOI(...)`, `KETTN(...)`, zum Beispiel `"I bin " + KETTN(alter)`
  oder `ZOI("17") + 1`.

Rangfolge von schwach zu stark: `ODA`, `UND`, `NED`, Vergleiche, `+ -`, `* / %`, Minus als Vorzeichen.
Klammern gehen immer.

### Verzweigung

```
WENN alter >= 18 DAUN
    SCHREIB("erwachsen") OIDA
SUNST WENN alter >= 14 DAUN
    SCHREIB("mündig") OIDA
SUNST
    SCHREIB("Kind") OIDA
PASST SCHO
```

`SUNST` und `SUNST WENN` sind freiwillig. Nach `PASST SCHO` darf ein `OIDA` stehen, muss aber nicht.

### Schleife

```
SOG AU i = 1 OIDA
SOLONG i <= 10 TUA
    SCHREIB(i) OIDA
    i = i + 1 OIDA
PASST SCHO
```

### Funktionen

```
FUNKTIAUN schnitt(a ZOI, b ZOI) GIBT ZRUCK KOMMAZOI
    GIB ZRUCK KOMMAZOI(a + b) / 2 OIDA
PASST SCHO

FUNKTIAUN gruess(wen KETTN)          # ohne GIBT ZRUCK: gibt nichts zurück
    SCHREIB("Servas " + wen + "!") OIDA
PASST SCHO

gruess("Resi") OIDA
SCHREIB(schnitt(1, 2)) OIDA
```

Bei den Parametern steht erst der Name, dann der Typ. Funktionen stehen ganz außen (nicht in
anderen Blöcken), die Reihenfolge ist egal, und sie dürfen sich selbst aufrufen.
Alles, was außerhalb von Funktionen steht, ist das Hauptprogramm. Funktionen sehen die
Variablen des Hauptprogramms nicht; was sie brauchen, bekommen sie als Parameter.

### Ausgabe

```
SCHREIB("Summe:", 5050, 3.5, WOAHR) OIDA    # Summe: 5050 3.5 WOAHR
```

`SCHREIB` nimmt beliebig viele Werte, schreibt sie mit Leerzeichen dazwischen und macht eine neue Zeile.

### Kommentare

```
# bis zum Ende der Zeile
/* über
   mehrere Zeilen */
```

### Namen

Namen bestehen aus Buchstaben (auch ä, ö, ü, ß), Ziffern und `_`. Groß und klein wird bei Namen
unterschieden (`alter` ist nicht `Alter`), bei den Wörtern der Sprache nicht (`wenn` = `WENN`).
Ein Wort der Sprache kann kein Name sein.


## Die Wörter ändern

Alle Wörter der Sprache stehen in [`woerterbuch.txt`](woerterbuch.txt), im selben Format wie bei OIDA-SQL:

```
IF        = WENN, WAUN
DECLARE   = SOG AU, DEKLARIER
END       = PASST SCHO
```

Links steht das feste Wort, das der Compiler kennt, rechts stehen deine Wörter. Mehrere Varianten
trennt ein Beistrich, ein Eintrag darf aus mehreren Wörtern bestehen (dann gewinnt der längste
Treffer: `STIMMT NED` ist `FOISCH`, `STIMMT` allein ist `WOAHR`). Auch das `OIDA` am Ende kann man umtaufen.

So geht's:

1. `woerterbuch.txt` im Projektordner ändern und speichern (als UTF-8).
2. Fertig. Neu bauen ist nicht nötig: `oida` sucht die Datei neben der `oida.exe` und bis zu zwei
   Ordner darüber, findet also von `target\release\oida.exe` aus die Datei im Projektordner.

Mit `oida --woerter` sieht man, welches Wörterbuch gerade gilt und welche Wörter drinstehen.

Wer die `oida.exe` woanders hinkopiert, legt die `woerterbuch.txt` daneben. Sonst gilt das
Wörterbuch, das beim Bauen in die `oida.exe` eingebaut wurde. Für einzelne Aufrufe gibt es
`--woerterbuch <datei>` und die Umgebungsvariable `OIDA_WOERTERBUCH`.

Achtung: Die Beispiele und die Tests sind mit den Standard-Wörtern geschrieben. Wer `WENN`
umtauft, muss sie dort auch umtaufen.


## Fehlermeldungen

Der Compiler sagt, wo es hakt (Datei, Zeile, Spalte) und zeigt die Stelle:

```
beispiel.oida:3:22: Heast, wo is des OIDA?
 3 | SOG AU name = "Franz"
   |                      ^

Na, des wird nix: 1 Fehler.
```

Ein paar andere:

```
'alter' is a ZOI, do kaunst ka KETTN einetuan.
'sume' kenn i ned. Host des mit SOG AU augsogt?
A KETTN + a ZOI? Des geht si ned aus. Nimm KETTN(...) zum Umwandln.
Des haaßt WENN, ned IF!
An Strichpunkt brauchst do ned. Bei uns haaßt des OIDA.
```

Fehler bei den Typen werden alle auf einmal gemeldet, sonst hört der Compiler beim ersten auf.
Beim Laufen bricht das Programm mit einer Meldung ab, wenn durch 0 geteilt wird oder
`ZOI("...")` keine Zahl bekommt:

```
Heast! Zeile 3: Durch nix kaun ma ned teilen.
```


## Aufbau

```
woerterbuch.txt        die Wörter der Sprache
beispiele/             Beispielprogramme, jeweils mit der erwarteten Ausgabe (.erwartet)
uebersetzer/           der Compiler (Rust), ergibt oida.exe
  src/woerterbuch.rs     Wörterbuch lesen, längster Treffer gewinnt
  src/lexer.rs           Quelltext -> Tokens
  src/parser.rs          Tokens -> Syntaxbaum (ast.rs)
  src/typen.rs           Typprüfung mit Typerkennung
  src/irgen.rs           Syntaxbaum -> LLVM-IR als Text
  src/treiber.rs         ruft clang auf, linkt die Laufzeit dazu
  src/fehler.rs          Fehlermeldungen mit Zeile und Spalte
  src/main.rs            der Befehl oida
  build.rs               baut die Laufzeit und packt sie in die oida.exe
  tests/                 Tests von Anfang bis Ende (.oida -> .exe -> Ausgabe)
laufzeit/              Laufzeitbibliothek (Rust), wird in jedes Programm gelinkt:
                       Ausgabe, Texte, Umwandlungen, Fehler beim Laufen
```

Der Compiler schreibt LLVM-IR als Text und lässt `clang` den Rest machen (Optimieren,
Maschinencode, Linken). Mit `--ir` kann man sich das IR anschauen.


## Was v0.1 noch nicht kann

- Texte werden nie freigegeben. Jedes `+` auf Texten und jedes `KETTN(...)` braucht Speicher,
  der erst am Programmende zurückgegeben wird. Für kurze Programme egal, für eine Schleife mit
  Millionen Durchläufen nicht. Das löst die Referenzzählung (siehe Fahrplan).
- Keine Eingabe von der Tastatur, keine Dateien.
- Schleifen kann man nicht vorzeitig verlassen (außer mit `GIB ZRUCK` in einer Funktion).
- Ganze Zahlen laufen bei mehr als 64 Bit still über.
- Die Meldungen beim Laufen (`Durch nix kaun ma ned teilen`) richten sich nicht nach dem Wörterbuch.
- Nur Windows, 64 Bit.


## Fahrplan

Noch nicht drin, in ungefähr dieser Reihenfolge geplant:

1. **Strukturen**: eigene Datentypen mit Feldern (Strukturen und Funktionen statt Klassen)
2. **Listen**
3. **Referenzzählung** für alles, was am Heap liegt (Texte, Listen, Strukturen)
4. **Der `oidasql(...)`-Block**: OIDA-SQL direkt in der Sprache, nicht als Text in Anführungszeichen,
   samt Anbindung an MySQL, etwa
   `oidasql( GIB MA name VON schuela WO schnitt < 2 )`
5. **Editor-Unterstützung**: Einfärben und Vorschläge

Dazu Kleineres: Schleifen verlassen, `FIA JEDE`, Eingabe, mehrere Dateien.
