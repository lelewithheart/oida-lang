#!/bin/bash
# Baut die beiden Installer für Linux. Sie landen in installer/aus/:
#
#   oida-<version>-linux-<prozessor>.sh    packt sich selbst aus, geht auf jeder Distribution
#   oida_<version>_<architektur>.deb       für Ubuntu und Debian (holt clang gleich mit)
#
# Braucht Rust (rustup), clang und für das .deb dpkg-deb. Unter Windows ruft
# installer\bauen.ps1 -Linux dieses Skript im WSL auf.
#
# Der Compiler in den Installern wird für musl gebaut: Er hängt dann von keiner
# Bibliothek des Systems ab und läuft auf alten wie auf neuen Distributionen.
# (Die Programme, die er baut, sind gewöhnliche Linux-Programme.)
set -euo pipefail

hier=$(cd "$(dirname "$0")" && pwd)
wurzel=$(cd "$hier/../.." && pwd)
aus="$wurzel/installer/aus"
# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$wurzel/Cargo.toml" | head -n 1)
prozessor=$(uname -m)
case "$prozessor" in
    x86_64) architektur=amd64 ;;
    aarch64) architektur=arm64 ;;
    *)
        echo "Für den Prozessor $prozessor ist das nicht vorgesehen." >&2
        exit 1
        ;;
esac
ziel="$prozessor-unknown-linux-musl"

# Liegt das Projekt auf einem Windows-Laufwerk (WSL), kommt der Bauordner ins Linux:
# Dort ist es viel schneller, und Windows und Linux kommen sich in target/ nicht in die Quere.
if [ -z "${CARGO_TARGET_DIR:-}" ] && [[ "$wurzel" == /mnt/* ]]; then
    export CARGO_TARGET_DIR="$HOME/.cache/oida-bau/target"
fi
bauordner=${CARGO_TARGET_DIR:-$wurzel/target}

echo "Baue den Compiler für $ziel ..."
if command -v rustup >/dev/null 2>&1; then
    rustup target add "$ziel" >/dev/null
fi
CARGO_PROFILE_RELEASE_STRIP=symbols cargo build --release --target "$ziel" -p oida --manifest-path "$wurzel/Cargo.toml"
oida="$bauordner/$ziel/release/oida"

# --- Was installiert wird: bin/oida und share/oida/... -------------------------
stufe=$(mktemp -d)
trap 'rm -rf "$stufe"' EXIT
baum="$stufe/baum"
install -D -m 755 "$oida" "$baum/bin/oida"
install -D -m 644 "$wurzel/installer/LIESMICH.txt" "$baum/share/oida/LIESMICH.txt"
install -D -m 644 "$wurzel/woerterbuch.txt" "$baum/share/oida/woerterbuch-vorlog.txt"
# Beispiele: die Programme selbst, ohne die erwarteten Ausgaben der Tests. Das Spiel
# mit Fenster (spui) und das große mit Musik (roguewave) laufen unter Linux noch nicht.
for datei in "$wurzel"/beispiele/*.oida "$wurzel"/beispiele/turnier/*.oida; do
    install -D -m 644 "$datei" "$baum/share/oida/beispiele/${datei#"$wurzel/beispiele/"}"
done
# Die VS-Code-Erweiterung baut derzeit nur vscode\bauen.ps1 unter Windows; liegt sie da, kommt sie mit.
if [ -f "$aus/oida-vscode.vsix" ]; then
    install -D -m 644 "$aus/oida-vscode.vsix" "$baum/share/oida/oida-vscode.vsix"
else
    echo "Hinweis: installer/aus/oida-vscode.vsix fehlt; die Installer entstehen ohne VS-Code-Erweiterung."
fi
find "$baum" -type d -exec chmod 755 {} +
mkdir -p "$aus"

# --- 1. Die Datei, die sich selbst auspackt -----------------------------------
selbst="$aus/oida-$version-linux-$prozessor.sh"
tar -C "$baum" --owner=0 --group=0 --numeric-owner --sort=name -czf "$stufe/paket.tar.gz" bin share
sed "s/@VERSION@/$version/" "$hier/install.sh" >"$selbst"
# Das Paket muss direkt hinter der Zeile __OIDA_PAKET__ anfangen.
if [ "$(tail -n 1 "$selbst")" != "__OIDA_PAKET__" ]; then
    echo "install.sh muss mit der Zeile __OIDA_PAKET__ aufhören." >&2
    exit 1
fi
cat "$stufe/paket.tar.gz" >>"$selbst"
chmod 755 "$selbst"

# --- 2. Das .deb-Paket ---------------------------------------------------------
deb="$aus/oida_${version}_${architektur}.deb"
if command -v dpkg-deb >/dev/null 2>&1; then
    paket="$stufe/deb"
    mkdir -p "$paket/DEBIAN" "$paket/usr"
    cp -a "$baum/bin" "$baum/share" "$paket/usr/"
    groesse=$(du -sk "$paket/usr" | cut -f 1)
    cat >"$paket/DEBIAN/control" <<EOF
Package: oida
Version: $version
Section: devel
Priority: optional
Architecture: $architektur
Depends: clang
Installed-Size: $groesse
Maintainer: lelewithheart <lelewithheart@users.noreply.github.com>
Description: OIDA, die Programmiersprache im Dialekt
 Der Compiler oida macht aus einem .oida-Programm über LLVM ein
 fertiges Programm. Beispiele und eine kurze Anleitung liegen in
 /usr/share/oida.
EOF
    dpkg-deb --build --root-owner-group "$paket" "$deb" >/dev/null
else
    echo "Hinweis: dpkg-deb fehlt; das .deb-Paket wird ausgelassen."
    deb=""
fi

echo "Fertig:"
for datei in "$selbst" $deb; do
    echo "  $datei ($(du -h "$datei" | cut -f 1))"
done
