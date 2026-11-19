#!/bin/bash
# Probiert die Linux-Installer aus (vorher bauen.sh oder unter Windows bauen.ps1 -Linux).
#
#   bash testen.sh              die Datei, die sich selbst auspackt
#   sudo bash testen.sh --deb   das .deb-Paket: wird wirklich installiert und wieder entfernt
#
# Der erste Test verwendet einen nachgemachten Home-Ordner unter /tmp: Dein
# echtes ~/.profile und ~/.local bleiben, wie sie sind.
set -u

hier=$(cd "$(dirname "$0")" && pwd)
aus="$hier/../aus"
geprueft=0

pruef() { # pruef "<was>" <befehl...>
    was=$1
    shift
    if "$@"; then
        geprueft=$((geprueft + 1))
        echo "  ok  $was"
    else
        echo "FALSCH: $was" >&2
        exit 1
    fi
}
enthaelt() { case "$1" in *"$2"*) return 0 ;; *) return 1 ;; esac; }

if [ "${1:-}" = "--deb" ]; then
    deb=$(ls -t "$aus"/oida_*.deb 2>/dev/null | head -n 1)
    [ -n "$deb" ] || { echo "Kein .deb in installer/aus. Zuerst bauen." >&2; exit 1; }
    [ "$(id -u)" = 0 ] || { echo "Das .deb installiert sich nur als root: sudo bash testen.sh --deb" >&2; exit 1; }
    export DEBIAN_FRONTEND=noninteractive
    probe=$(mktemp -d)
    cp "$deb" "$probe/oida.deb"
    chmod 755 "$probe" && chmod 644 "$probe/oida.deb"
    echo "1. Was im Paket steht"
    angaben=$(dpkg-deb -I "$probe/oida.deb")
    pruef "es heißt oida und braucht clang" enthaelt "$angaben" "Depends: clang"
    inhalt=$(dpkg-deb -c "$probe/oida.deb")
    pruef "der Compiler ist ausführbar und gehört root" enthaelt "$inhalt" "-rwxr-xr-x root/root"
    pruef "Beispiele und LIESMICH sind dabei" enthaelt "$inhalt" "./usr/share/oida/beispiele/hallo.oida"
    echo "2. Installieren"
    apt-get install -y -q "$probe/oida.deb" >/dev/null 2>&1
    pruef "oida ist da" test -x /usr/bin/oida
    pruef "oida --version" enthaelt "$(/usr/bin/oida --version)" "oida "
    # Verwenden als gewöhnlicher Benutzer, nicht als root.
    wer=${SUDO_USER:-$(getent passwd 1000 | cut -d: -f1)}
    lauf=$(su "$wer" -c "cd \$(mktemp -d) && oida /usr/share/oida/beispiele/fizzbuzz.oida -o fizzbuzz && ./fizzbuzz | head -n 3 | tr '\n' ' '")
    pruef "ein Benutzer übersetzt ein Beispiel aus dem Paket" enthaelt "$lauf" "1 2 Fizz"
    echo "3. Entfernen"
    apt-get remove -y -q oida >/dev/null 2>&1
    pruef "oida ist wieder weg" test ! -e /usr/bin/oida -a ! -e /usr/share/oida
    rm -rf "$probe"
    echo
    echo "$geprueft von $geprueft bestanden"
    exit 0
fi

installer=$(ls -t "$aus"/oida-*-linux-*.sh 2>/dev/null | head -n 1)
[ -n "$installer" ] || { echo "Kein Installer in installer/aus. Zuerst bauen." >&2; exit 1; }
probe=$(mktemp -d)
trap 'rm -rf "$probe"' EXIT
cp "$installer" "$probe/inst.sh"
export HOME="$probe/heim"
mkdir -p "$HOME"
printf '# mei Profil\nexport EDITOR=nano\n' >"$HOME/.profile"
cp "$HOME/.profile" "$probe/profil.vorher"
cd "$probe" || exit 1
daten="$HOME/.local/share/oida"

echo "1. Niemand da zum Antworten und kein --jo: Es wird nichts in .profile geschrieben"
aus_text=$(sh inst.sh --ohne-vscode </dev/null 2>&1)
pruef "Code 0" test $? -eq 0
pruef "die Frage wird als nein gewertet" enthaelt "$aus_text" "kana do zum Frogn"
pruef ".profile ist unverändert" cmp -s "$HOME/.profile" "$probe/profil.vorher"
pruef "der Compiler liegt trotzdem da" test -x "$HOME/.local/bin/oida"

echo "2. Installieren mit --jo"
aus_text=$(sh inst.sh --jo --ohne-vscode </dev/null 2>&1)
pruef "Code 0" test $? -eq 0
pruef "die Erfolgsmeldung" enthaelt "$aus_text" "Passt! OIDA is installiert."
for datei in LIESMICH.txt woerterbuch-vorlog.txt deinstalliern.sh installiert.txt beispiele/hallo.oida beispiele/turnier/spiela.oida; do
    pruef "$datei liegt da" test -f "$daten/$datei"
done
pruef ".profile hat die Zeile für den PATH" grep -qF 'export PATH="$HOME/.local/bin:$PATH"' "$HOME/.profile"

echo "3. Noch einmal installieren: nichts doppelt"
sh inst.sh --jo --ohne-vscode </dev/null >/dev/null 2>&1
pruef "die Zeile steht genau einmal in .profile" test "$(grep -c 'OIDA' "$HOME/.profile")" -eq 1

echo "4. Der installierte Compiler"
lauf=$(. "$HOME/.profile" && command -v oida && oida --version)
pruef "über den PATH aus .profile zu finden" enthaelt "$lauf" "$HOME/.local/bin/oida"
pruef "hängt von keiner Bibliothek des Systems ab" enthaelt "$(ldd "$HOME/.local/bin/oida" 2>&1)" "statically linked"
if command -v clang >/dev/null 2>&1; then
    lauf=$(. "$HOME/.profile" && oida "$daten/beispiele/hallo.oida" -o hallo --renn 2>&1)
    pruef "ein Beispiel aus dem Installer lässt sich übersetzen und läuft" enthaelt "$lauf" "Servas"
else
    echo "  (ausgelassen: kein clang auf diesem Rechner)"
fi

echo "5. Ohne clang: sagt, wie man es holt, und tut nichts von selbst"
mkdir -p "$probe/nur"
for werkzeug in sh awk tail head od tar gzip grep cat chmod mkdir printf wc tr rm id basename find rmdir sed pwd; do
    [ -x "$(command -v $werkzeug)" ] && ln -sf "$(command -v $werkzeug)" "$probe/nur/$werkzeug"
done
aus_text=$(PATH="$probe/nur" sh inst.sh --ohne-vscode --ohne-pfad </dev/null 2>&1)
pruef "Code 0" test $? -eq 0
pruef "sagt, dass clang fehlt" enthaelt "$aus_text" "clang fehlt no"

echo "6. In einen anderen Ordner"
sh inst.sh --jo --ohne-vscode --ohne-clang --ordner "$probe/woanders" </dev/null >/dev/null 2>&1
pruef "der Compiler liegt dort" test -x "$probe/woanders/bin/oida"

echo "7. Entfernen lässt liegen, was nicht von OIDA ist"
echo "des ghört mia" >"$daten/beispiele/meins.oida"
aus_text=$(sh "$daten/deinstalliern.sh" --weg --jo </dev/null 2>&1)
pruef "Code 0" test $? -eq 0
pruef "die Meldung" enthaelt "$aus_text" "OIDA is weg."
pruef "nur die eigene Datei ist noch da" test "$(find "$HOME/.local" -type f | tr '\n' ' ')" = "$daten/beispiele/meins.oida "
pruef ".profile ist wieder genau wie vorher" cmp -s "$HOME/.profile" "$probe/profil.vorher"

echo "8. Entfernen, wo nichts ist; die Entferner-Datei installiert nichts"
sh inst.sh --weg --jo </dev/null >/dev/null 2>&1
pruef "Code 1, wenn nichts installiert ist" test $? -eq 1
sh "$probe/woanders/share/oida/deinstalliern.sh" --jo </dev/null >/dev/null 2>&1
pruef "Code 1, wenn die Entferner-Datei installieren soll" test $? -eq 1
sh "$probe/woanders/share/oida/deinstalliern.sh" --weg --jo --ordner "$probe/woanders" </dev/null >/dev/null 2>&1
pruef "aus dem anderen Ordner entfernt" test ! -e "$probe/woanders/bin/oida"

echo
echo "$geprueft von $geprueft bestanden"
