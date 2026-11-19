#!/bin/sh
# Installiert OIDA unter Linux. Diese Datei packt sich selbst aus: Hinter der
# letzten Zeile des Skripts hängt alles, was installiert wird (ein tar.gz).
# Gebaut wird sie mit installer/linux/bauen.sh.
#
#   sh oida-<version>-linux-<prozessor>.sh              für dich allein, nach ~/.local
#   sudo sh oida-<version>-linux-<prozessor>.sh --system    für alle, nach /usr/local
#
# Der Compiler darin hängt von nichts ab und läuft auf jeder Distribution.
# Die Programme, die er baut, brauchen die gewöhnliche C-Bibliothek (glibc).
set -eu

VERSION="@VERSION@"
MARKE='# OIDA: ~/.local/bin in den PATH (vom Installer eingetragen)'
ZEILE='export PATH="$HOME/.local/bin:$PATH"'

hilfe() {
    cat <<EOF
oida-installer - installiert OIDA, de Programmiersproch

Aufruf:
  sh $(basename "$0") [Optionen]

Ohne Optionen kummt OIDA nach ~/.local (nur fia di, ohne sudo). Er frogt, bevor
er wos installiert oda in ane vo deine Dateien schreibt.

Optionen:
  --system          fia olle, nach /usr/local (braucht sudo)
  --ordner <pfad>   durthin stott nach ~/.local (es entstehn durt bin/ und share/oida/)
  --jo              nix frogn, überoi "jo" sogn
  --ohne-pfad       den PATH ned aungreifn
  --ohne-clang      clang ned suachn und ned installiern
  --ohne-vscode     de VS-Code-Erweiterung ned installiern
  --weg             OIDA wieda entferna
  --hilfe, --version
EOF
}

vorsilbe="$HOME/.local"
jo=0
weg=0
ohne_pfad=0
ohne_clang=0
ohne_vscode=0
while [ $# -gt 0 ]; do
    case "$1" in
        --system) vorsilbe=/usr/local ;;
        --ordner)
            if [ $# -lt 2 ]; then
                echo "Heast! Noch --ordner fehlt da Ordner." >&2
                exit 2
            fi
            vorsilbe=$2
            shift
            ;;
        --jo) jo=1 ;;
        --weg) weg=1 ;;
        --ohne-pfad) ohne_pfad=1 ;;
        --ohne-clang) ohne_clang=1 ;;
        --ohne-vscode) ohne_vscode=1 ;;
        --hilfe | -h)
            hilfe
            exit 0
            ;;
        --version)
            echo "oida-installer $VERSION"
            exit 0
            ;;
        *)
            echo "Heast! De Option $1 kenn i ned. Schau mit --hilfe noch." >&2
            exit 2
            ;;
    esac
    shift
done

# Ein Ordner von ganz oben, auch wenn er von hier aus angegeben wurde.
case "$vorsilbe" in
    /*) ;;
    *) vorsilbe="$(pwd)/$vorsilbe" ;;
esac
vorsilbe=${vorsilbe%/}
bin="$vorsilbe/bin"
daten="$vorsilbe/share/oida"
liste="$daten/installiert.txt"

# Stellt eine Frage, auf die man mit Enter (ja) oder n (nein) antwortet. Mit
# --jo wird nicht gefragt. Ist niemand da, der antworten könnte, heißt das nein.
frag() {
    if [ "$jo" = 1 ]; then
        return 0
    fi
    printf '%s [J/n] ' "$1"
    if [ ! -t 0 ]; then
        echo "(kana do zum Frogn: na)"
        return 1
    fi
    antwort=""
    read -r antwort || antwort=n
    case "$antwort" in
        n* | N*) return 1 ;;
        *) return 0 ;;
    esac
}

# Die Zeile, hinter der das Paket anfängt (leer, wenn nichts dranhängt).
paket_anfang() {
    awk '/^__OIDA_PAKET__$/ { print NR + 1; exit }' "$0"
}

# Sucht clang so, wie es auch der Compiler tut: OIDA_CLANG, clang, das neueste clang-<nummer>.
finde_clang() {
    if [ -n "${OIDA_CLANG:-}" ] && [ -f "$OIDA_CLANG" ]; then
        echo "$OIDA_CLANG"
        return 0
    fi
    if command -v clang >/dev/null 2>&1; then
        command -v clang
        return 0
    fi
    nummer=40
    while [ "$nummer" -ge 9 ]; do
        if command -v "clang-$nummer" >/dev/null 2>&1; then
            command -v "clang-$nummer"
            return 0
        fi
        nummer=$((nummer - 1))
    done
    return 1
}

# Der Befehl, mit dem diese Distribution clang installiert (leer, wenn ich sie nicht kenne).
clang_befehl() {
    if command -v apt-get >/dev/null 2>&1; then
        echo "apt-get install -y clang"
    elif command -v dnf >/dev/null 2>&1; then
        echo "dnf install -y clang"
    elif command -v pacman >/dev/null 2>&1; then
        echo "pacman -S --noconfirm clang"
    elif command -v zypper >/dev/null 2>&1; then
        echo "zypper install -y clang"
    fi
}

installieren() {
    echo "OIDA $VERSION - Installiern"
    echo
    echo "[1/4] Dateien nach $vorsilbe"
    anfang=$(paket_anfang)
    if [ -z "$anfang" ] || [ -z "$(tail -n +"$anfang" "$0" | head -c 1 | od -An -c)" ]; then
        echo "Heast! In dera Datei is nix zum Installiern drin (de is nur zum Entferna: --weg)." >&2
        exit 1
    fi
    if ! mkdir -p "$bin" "$daten" 2>/dev/null || [ ! -w "$bin" ] || [ ! -w "$daten" ]; then
        echo "Heast! In $vorsilbe derf i ned schreibn. Fia /usr/local brauchst sudo; sunst nimm --ordner." >&2
        exit 1
    fi
    alte=""
    if [ -f "$liste" ]; then
        alte=$(cat "$liste")
    fi
    neue=$(tail -n +"$anfang" "$0" | tar -tzf - | grep -v '/$')
    tail -n +"$anfang" "$0" | tar -xzf - -C "$vorsilbe"
    # Diese Datei ohne das Paket: Damit lässt sich später alles entfernen.
    head -n "$((anfang - 1))" "$0" >"$daten/deinstalliern.sh"
    chmod 755 "$daten/deinstalliern.sh"
    printf '%s\n' "$neue" "share/oida/deinstalliern.sh" "share/oida/installiert.txt" >"$liste"
    # Was eine ältere Version hingelegt hat und jetzt nicht mehr dabei ist, kommt weg.
    if [ -n "$alte" ]; then
        printf '%s\n' "$alte" | while IFS= read -r datei; do
            case "$datei" in
                "" | /* | *..*) continue ;;
            esac
            if ! grep -qxF -- "$datei" "$liste"; then
                rm -f "$vorsilbe/$datei"
            fi
        done
    fi
    echo "      $(wc -l <"$liste" | tr -d ' ') Dateien"

    echo "[2/4] PATH (damit ma überoi afoch \"oida\" tippn kaun)"
    case ":$PATH:" in
        *":$bin:"*) echo "      $bin is scho drin" ;;
        *)
            profil="$HOME/.profile"
            case "${SHELL:-}" in
                */zsh) profil="$HOME/.zprofile" ;;
            esac
            if [ "$ohne_pfad" = 1 ]; then
                echo "      ausglossn (--ohne-pfad)"
            elif [ "$vorsilbe" != "$HOME/.local" ]; then
                echo "      $bin is ned in deim PATH. Trog's söwa ei:  export PATH=\"$bin:\$PATH\""
            elif grep -qF "$MARKE" "$profil" 2>/dev/null; then
                echo "      steht scho in $profil (güt ab da nextn Aumödung, oda glei mit:  . $profil)"
            elif frag "      $bin is ned in deim PATH. Soll i's in $profil eintrogn?"; then
                printf '\n%s\n%s\n' "$MARKE" "$ZEILE" >>"$profil"
                echo "      eintrogn. Güt ab da nextn Aumödung, oda glei mit:  . $profil"
            else
                echo "      Guat, dann trog's söwa ei:  $ZEILE"
            fi
            ;;
    esac

    echo "[3/4] clang"
    clang_da=1
    schief=0
    if [ "$ohne_clang" = 1 ]; then
        echo "      ausglossn (--ohne-clang)"
    elif clang=$(finde_clang); then
        echo "      is scho do: $clang"
    else
        clang_da=0
        echo "      OIDA braucht clang, um aus deim Programm a fertigs z'mochn. Des fehlt no."
        befehl=$(clang_befehl)
        if [ -z "$befehl" ]; then
            echo "      De Paketverwoitung do kenn i ned. Installier clang söwa, dann geht's."
        else
            if [ "$(id -u)" != 0 ]; then
                if command -v sudo >/dev/null 2>&1; then
                    befehl="sudo $befehl"
                else
                    befehl=""
                    echo "      Installier's ois root:  $(clang_befehl)"
                fi
            fi
            if [ -n "$befehl" ]; then
                if frag "      Soll i's installiern?  $befehl"; then
                    if $befehl && finde_clang >/dev/null; then
                        clang_da=1
                        echo "      clang is jetzt do"
                    else
                        schief=1
                        echo "      Heast! Des hot ned klappt. Probier's söwa:  $befehl"
                    fi
                else
                    echo "      Guat, dann ned. Späta geht's mit:  $befehl"
                fi
            fi
        fi
    fi

    echo "[4/4] VS Code"
    vsix="$daten/oida-vscode.vsix"
    if [ "$ohne_vscode" = 1 ]; then
        echo "      ausglossn (--ohne-vscode)"
    elif [ ! -f "$vsix" ]; then
        echo "      (in dem Installer is ka Erweiterung dabei)"
    elif ! command -v code >/dev/null 2>&1; then
        echo "      VS Code find i ned. Wennst as späta host:  code --install-extension \"$vsix\""
    elif frag "      VS Code is do. Soll i de OIDA-Erweiterung einbaun (Foabn, Fehler beim Tippn, F5)?"; then
        if code --install-extension "$vsix" --force >/dev/null 2>&1; then
            echo "      eingebaut"
        else
            echo "      Heast! VS Code hot de Erweiterung ned gnumma. Probier:  code --install-extension \"$vsix\""
        fi
    else
        echo "      Guat, dann ned. Späta geht's mit:  code --install-extension \"$vsix\""
    fi

    echo
    if [ "$clang_da" = 1 ]; then
        echo "Passt! OIDA is installiert."
    else
        echo "OIDA is installiert, oba clang fehlt no (schau obn): Ohne des kaun's nix übersetzn."
    fi
    echo "Probier:"
    echo "    oida --version"
    echo "    oida \"$daten/beispiele/hallo.oida\" -o hallo --renn"
    echo "Wia's weidageht, steht in $daten/LIESMICH.txt"
    # 3: installiert, aber etwas, das OIDA braucht, ließ sich nicht holen.
    if [ "$schief" = 1 ]; then
        exit 3
    fi
}

entfernen() {
    if [ ! -f "$liste" ]; then
        echo "Heast! In $vorsilbe is ka OIDA installiert ($liste fehlt)." >&2
        echo "Wor's des .deb-Paket? Dann:  sudo apt remove oida" >&2
        exit 1
    fi
    if [ ! -w "$daten" ]; then
        echo "Heast! In $vorsilbe derf i ned schreibn. Nimm sudo." >&2
        exit 1
    fi
    echo "OIDA entferna aus $vorsilbe"
    if ! frag "Soll OIDA wirklich weg?"; then
        echo "Guat, es bleibt ollas, wia's is."
        exit 0
    fi
    # Nur, was in der Liste steht. Ein Name, der aus dem Ordner hinausführt, wird nicht angefasst.
    dateien=$(cat "$liste")
    printf '%s\n' "$dateien" | while IFS= read -r datei; do
        case "$datei" in
            "" | /* | *..*) continue ;;
        esac
        rm -f "$vorsilbe/$datei"
    done
    # Leere Ordner von innen nach außen; was noch etwas enthält (eigene Dateien), bleibt.
    find "$daten" -depth -type d -exec rmdir {} \; 2>/dev/null || true
    if [ -d "$daten" ]; then
        echo "In $daten liegt no wos, des ned vo mia is; des hob i lieng lossn."
    fi
    # Die zwei Zeilen im Profil, genau so, wie der Installer sie geschrieben hat.
    for profil in "$HOME/.profile" "$HOME/.zprofile"; do
        if grep -qF "$MARKE" "$profil" 2>/dev/null; then
            awk -v marke="$MARKE" -v zeile="$ZEILE" '
                $0 == marke {
                    # Die Leerzeile, die der Installer davor gesetzt hat, geht mit.
                    if (anzahl > 0 && zeilen[anzahl] == "") anzahl--
                    # Die Zeile danach nur, wenn sie wirklich die vom Installer ist.
                    if ((getline naechste) > 0 && naechste != zeile) zeilen[++anzahl] = naechste
                    next
                }
                { zeilen[++anzahl] = $0 }
                END { for (i = 1; i <= anzahl; i++) print zeilen[i] }' "$profil" >"$profil.oida-neu"
            cat "$profil.oida-neu" >"$profil"
            rm -f "$profil.oida-neu"
            echo "  aus $profil gnumma"
        fi
    done
    echo "OIDA is weg."
    echo "clang hob i ned augriffn. De VS-Code-Erweiterung geht weg mit:  code --uninstall-extension lelewithheart.oida"
}

if [ "$weg" = 1 ]; then
    entfernen
else
    installieren
fi
exit 0
__OIDA_PAKET__
