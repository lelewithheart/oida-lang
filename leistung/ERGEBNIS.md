# Leistungsvergleich

Erzeugt mit `cargo run --release -p oida-messen` am 2026-11-18. Nicht von Hand ändern.

Rechner: AMD Ryzen 5 8645HS w/ Radeon 760M Graphics, 14 GB Arbeitsspeicher, Microsoft Windows 11 Pro

| Sprache | Art | Version |
|---|---|---|
| OIDA | übersetzt (LLVM) | oida 0.1.0 |
| C | übersetzt (LLVM, clang -O2) | clang version 23.1.5 |
| Rust | übersetzt (LLVM, opt-level 3) | rustc 1.100.0 (0ef1bc386 2026-11-09) |
| Java | Bytecode + JIT | openjdk version "25.0.4.1" 2026-08-18 |
| JavaScript | JIT (Node) | v26.13.0 |
| Python | Interpreter | Python 3.13.14 |

## Überblick: Zeit

Kleiner ist besser. Die schnellste von mehreren Runden, samt Starten des Programms.

| Programm | OIDA | C | Rust | Java | JavaScript | Python |
|---|---:|---:|---:|---:|---:|---:|
| hallo | 20 ms | 21 ms | 22 ms | 127 ms | 88 ms | 192 ms |
| fib | 398 ms | 462 ms | 401 ms | 571 ms | 1,70 s | 21,35 s |
| primzahlen | 486 ms | 486 ms | 487 ms | 675 ms | 562 ms | 18,56 s |
| collatz | 198 ms | 201 ms | 200 ms | 434 ms | 2,92 s | 25,92 s |
| mandelbrot | 237 ms | 361 ms | 241 ms | 488 ms | 317 ms | 79,60 s |
| texte | 386 ms | 101 ms | 205 ms | 180 ms | 140 ms | 622 ms |
| ausgabe | 364 ms | 526 ms | 169 ms | 473 ms | 363 ms | 4,78 s |

## Überblick: Zeit im Verhältnis zu OIDA

1,00× heißt gleich schnell wie OIDA, 2× heißt doppelt so lang, 0,5× halb so lang. Die letzte Zeile ist das geometrische Mittel über alle Programme.

| Programm | OIDA | C | Rust | Java | JavaScript | Python |
|---|---:|---:|---:|---:|---:|---:|
| hallo | 1,00× | 1,04× | 1,08× | 6,30× | 4,35× | 9,52× |
| fib | 1,00× | 1,16× | 1,01× | 1,43× | 4,26× | 54× |
| primzahlen | 1,00× | 1,00× | 1,00× | 1,39× | 1,16× | 38× |
| collatz | 1,00× | 1,01× | 1,01× | 2,19× | 15× | 131× |
| mandelbrot | 1,00× | 1,52× | 1,02× | 2,06× | 1,34× | 336× |
| texte | 1,00× | 0,26× | 0,53× | 0,47× | 0,36× | 1,61× |
| ausgabe | 1,00× | 1,45× | 0,46× | 1,30× | 1,00× | 13× |
| **Mittel** | **1,00×** | **0,95×** | **0,83×** | **1,66×** | **2,05×** | **29×** |

## Überblick: Speicher

Der höchste Arbeitsspeicher, den das Programm gebraucht hat. Kleiner ist besser.

| Programm | OIDA | C | Rust | Java | JavaScript | Python |
|---|---:|---:|---:|---:|---:|---:|
| hallo | 4,9 MB | 4,8 MB | 4,9 MB | 56,7 MB | 34,5 MB | 23,5 MB |
| fib | 4,9 MB | 4,8 MB | 4,9 MB | 57,2 MB | 36,5 MB | 23,4 MB |
| primzahlen | 4,9 MB | 4,8 MB | 5,1 MB | 57,4 MB | 37,1 MB | 23,5 MB |
| collatz | 4,9 MB | 4,8 MB | 4,9 MB | 57,9 MB | 37,3 MB | 23,4 MB |
| mandelbrot | 4,9 MB | 4,8 MB | 4,9 MB | 57,9 MB | 38,9 MB | 23,4 MB |
| texte | 111,1 MB | 4,8 MB | 5,0 MB | 96,9 MB | 45,0 MB | 23,6 MB |
| ausgabe | 5,0 MB | 4,9 MB | 4,9 MB | 135,5 MB | 81,1 MB | 25,1 MB |

## Die Programme im Einzelnen

### hallo

Nur eine Zeile ausgeben. Misst, wie lange ein Programm zum Starten braucht.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 20 ms | 1,00× | 7 | 4,9 MB | 387 ms | 290 KB |
| C | 21 ms | 1,04× | 7 | 4,8 MB | 376 ms | 60 KB |
| Rust | 22 ms | 1,08× | 7 | 4,9 MB | 523 ms | 412 KB |
| Java | 127 ms | 6,30× | 7 | 56,7 MB | 852 ms | 0 KB |
| JavaScript | 88 ms | 4,35× | 7 | 34,5 MB | – | – |
| Python | 192 ms | 9,52× | 7 | 23,5 MB | – | – |

### fib

Fibonacci-Zahl 40, rekursiv. Misst Funktionsaufrufe (rund 330 Millionen).

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 398 ms | 1,00× | 7 | 4,9 MB | 393 ms | 292 KB |
| C | 462 ms | 1,16× | 7 | 4,8 MB | 393 ms | 61 KB |
| Rust | 401 ms | 1,01× | 7 | 4,9 MB | 580 ms | 414 KB |
| Java | 571 ms | 1,43× | 7 | 57,2 MB | 964 ms | 1 KB |
| JavaScript | 1,70 s | 4,26× | 5 | 36,5 MB | – | – |
| Python | 21,35 s | 54× | 1 | 23,4 MB | – | – |

### primzahlen

Primzahlen unter 2 Millionen zählen, durch Probeteilen. Misst Schleifen und Teilen mit Rest.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 486 ms | 1,00× | 7 | 4,9 MB | 458 ms | 292 KB |
| C | 486 ms | 1,00× | 7 | 4,8 MB | 334 ms | 61 KB |
| Rust | 487 ms | 1,00× | 7 | 5,1 MB | 533 ms | 414 KB |
| Java | 675 ms | 1,39× | 7 | 57,4 MB | 964 ms | 1 KB |
| JavaScript | 562 ms | 1,16× | 7 | 37,1 MB | – | – |
| Python | 18,56 s | 38× | 3 | 23,5 MB | – | – |

### collatz

Längste Collatz-Folge für Startwerte unter 1 Million. Misst Schleifen, Verzweigungen und Rechnen mit ganzen Zahlen.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 198 ms | 1,00× | 7 | 4,9 MB | 385 ms | 292 KB |
| C | 201 ms | 1,01× | 7 | 4,8 MB | 406 ms | 61 KB |
| Rust | 200 ms | 1,01× | 7 | 4,9 MB | 569 ms | 414 KB |
| Java | 434 ms | 2,19× | 7 | 57,9 MB | 885 ms | 1 KB |
| JavaScript | 2,92 s | 15× | 5 | 37,3 MB | – | – |
| Python | 25,92 s | 131× | 1 | 23,4 MB | – | – |

### mandelbrot

Mandelbrot-Menge auf 1000 x 1000 Punkten, höchstens 300 Schritte je Punkt. Misst Rechnen mit Kommazahlen.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 237 ms | 1,00× | 7 | 4,9 MB | 376 ms | 292 KB |
| C | 361 ms | 1,52× | 7 | 4,8 MB | 374 ms | 62 KB |
| Rust | 241 ms | 1,02× | 7 | 4,9 MB | 475 ms | 414 KB |
| Java | 488 ms | 2,06× | 7 | 57,9 MB | 784 ms | 1 KB |
| JavaScript | 317 ms | 1,34× | 7 | 38,9 MB | – | – |
| Python | 79,60 s | 336× | 1 | 23,4 MB | – | – |

### texte

Eine Million kurze Texte zusammenbauen und vergleichen. Misst Texte und den Speicher dafür.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 386 ms | 1,00× | 7 | 111,1 MB | 403 ms | 293 KB |
| C | 101 ms | 0,26× | 7 | 4,8 MB | 340 ms | 63 KB |
| Rust | 205 ms | 0,53× | 7 | 5,0 MB | 491 ms | 416 KB |
| Java | 180 ms | 0,47× | 7 | 96,9 MB | 854 ms | 1 KB |
| JavaScript | 140 ms | 0,36× | 7 | 45,0 MB | – | – |
| Python | 622 ms | 1,61× | 7 | 23,6 MB | – | – |

### ausgabe

Zwei Millionen Zeilen ausgeben. Misst die Ausgabe.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 364 ms | 1,00× | 7 | 5,0 MB | 343 ms | 292 KB |
| C | 526 ms | 1,45× | 7 | 4,9 MB | 317 ms | 61 KB |
| Rust | 169 ms | 0,46× | 7 | 4,9 MB | 617 ms | 420 KB |
| Java | 473 ms | 1,30× | 7 | 135,5 MB | 819 ms | 1 KB |
| JavaScript | 363 ms | 1,00× | 7 | 81,1 MB | – | – |
| Python | 4,78 s | 13× | 5 | 25,1 MB | – | – |

