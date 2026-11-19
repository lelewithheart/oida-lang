# Baut die VS-Code-Erweiterung fuer OIDA als .vsix-Datei.
#
#   bauen.ps1                 bauen  ->  vscode\oida-vscode.vsix
#   bauen.ps1 -Installieren   bauen und gleich in VS Code installieren
#   bauen.ps1 -OhneOrt -Ziel <datei.vsix>
#                             fuer die Installer: woanders hinlegen und den Ort der oida.exe
#                             auf diesem Rechner nicht hineinschreiben (dann sucht sie im PATH)
#
# Was passiert:
#   1. Der Compiler wird gebaut (cargo build --release).
#   2. Grammatik, Einrueckregeln und Schnipsel werden aus woerterbuch.txt erzeugt.
#   3. Alles kommt in die .vsix-Datei. Dazu der Ort der oida.exe, damit die
#      Erweiterung den Compiler auch findet, wenn ein anderer Ordner offen ist.
#
# Diese Datei absichtlich ohne Umlaute (Windows PowerShell liest sie sonst falsch).
param([switch]$Installieren, [switch]$OhneOrt, [string]$Ziel)

$ErrorActionPreference = 'Stop'
$hier   = Split-Path -Parent $MyInvocation.MyCommand.Path
$wurzel = Split-Path -Parent $hier

function Finde-Programm($name, $orte) {
    $imPfad = Get-Command $name -ErrorAction SilentlyContinue
    if ($imPfad) { return $imPfad.Source }
    foreach ($ort in $orte) {
        if (Test-Path $ort) { return $ort }
    }
    return $null
}

# --- 1. Compiler bauen ------------------------------------------------------
$cargo = Finde-Programm 'cargo.exe' @("$env:USERPROFILE\.cargo\bin\cargo.exe")
if (-not $cargo) { throw 'cargo nicht gefunden. Rust installieren (siehe README.md).' }
Write-Host 'Baue den Compiler ...'
& $cargo build --release --manifest-path (Join-Path $wurzel 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'Der Compiler laesst sich nicht bauen.' }
$oida = Join-Path $wurzel 'target\release\oida.exe'

# --- 2. Dateien aus dem Woerterbuch erzeugen --------------------------------
& $oida --woerterbuch (Join-Path $wurzel 'woerterbuch.txt') --editor-dateien $hier | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Die Dateien fuer den Editor lassen sich nicht erzeugen.' }

# --- 3. .vsix zusammenpacken (eine Zip-Datei mit festem Aufbau) -------------
$paket   = Get-Content (Join-Path $hier 'package.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$version = $paket.version
$vsix    = if ($Ziel) { $Ziel } else { Join-Path $hier 'oida-vscode.vsix' }
$utf8    = New-Object System.Text.UTF8Encoding $false

$manifest = @"
<?xml version="1.0" encoding="utf-8"?>
<PackageManifest Version="2.0.0" xmlns="http://schemas.microsoft.com/developer/vsx-schema/2011" xmlns:d="http://schemas.microsoft.com/developer/vsx-schema-design/2011">
  <Metadata>
    <Identity Language="en-US" Id="$($paket.name)" Version="$version" Publisher="$($paket.publisher)" />
    <DisplayName>$($paket.displayName)</DisplayName>
    <Description xml:space="preserve">Die Programmiersprache OIDA in VS Code</Description>
    <Tags>oida</Tags>
    <Categories>Programming Languages</Categories>
    <GalleryFlags>Public</GalleryFlags>
    <Properties>
      <Property Id="Microsoft.VisualStudio.Code.Engine" Value="$($paket.engines.vscode)" />
      <Property Id="Microsoft.VisualStudio.Code.ExtensionDependencies" Value="" />
      <Property Id="Microsoft.VisualStudio.Code.ExtensionPack" Value="" />
      <Property Id="Microsoft.VisualStudio.Code.ExtensionKind" Value="workspace" />
      <Property Id="Microsoft.VisualStudio.Code.LocalizedLanguages" Value="" />
    </Properties>
  </Metadata>
  <Installation>
    <InstallationTarget Id="Microsoft.VisualStudio.Code" />
  </Installation>
  <Dependencies />
  <Assets>
    <Asset Type="Microsoft.VisualStudio.Code.Manifest" Path="extension/package.json" Addressable="true" />
    <Asset Type="Microsoft.VisualStudio.Services.Content.Details" Path="extension/README.md" Addressable="true" />
  </Assets>
</PackageManifest>
"@

$inhaltsarten = @"
<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension=".json" ContentType="application/json" /><Default Extension=".js" ContentType="application/javascript" /><Default Extension=".md" ContentType="text/markdown" /><Default Extension=".txt" ContentType="text/plain" /><Default Extension=".vsixmanifest" ContentType="text/xml" /></Types>
"@

$dateien = @(
    'package.json',
    'extension.js',
    'README.md',
    'language-configuration.json',
    'syntaxes/oida.tmLanguage.json',
    'snippets/oida.json'
)

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
if (Test-Path $vsix) { Remove-Item -Force $vsix }
$zip = [System.IO.Compression.ZipFile]::Open($vsix, 'Create')
try {
    function Lege-Ab($name, [byte[]]$bytes) {
        # Namen in der Zip-Datei immer mit / schreiben, sonst liest VS Code sie nicht.
        $eintrag = $zip.CreateEntry($name)
        $strom = $eintrag.Open()
        $strom.Write($bytes, 0, $bytes.Length)
        $strom.Dispose()
    }
    Lege-Ab '[Content_Types].xml' $utf8.GetBytes($inhaltsarten)
    Lege-Ab 'extension.vsixmanifest' $utf8.GetBytes($manifest)
    foreach ($datei in $dateien) {
        $pfad = Join-Path $hier ($datei -replace '/', '\')
        if (-not (Test-Path $pfad)) { throw "Datei fehlt: $pfad" }
        Lege-Ab "extension/$datei" ([System.IO.File]::ReadAllBytes($pfad))
    }
    if (-not $OhneOrt) { Lege-Ab 'extension/compiler-ort.txt' $utf8.GetBytes($oida) }
} finally {
    $zip.Dispose()
}
Write-Host "Fertig: $vsix (Version $version)"

# --- 4. Auf Wunsch installieren ---------------------------------------------
if ($Installieren) {
    $code = Finde-Programm 'code.cmd' @(
        "$env:LOCALAPPDATA\Programs\Microsoft VS Code\bin\code.cmd",
        "$env:ProgramFiles\Microsoft VS Code\bin\code.cmd"
    )
    if (-not $code) { throw 'VS Code nicht gefunden. Installieren mit: winget install Microsoft.VisualStudioCode' }
    & $code --install-extension $vsix --force
    if ($LASTEXITCODE -ne 0) { throw 'Die Erweiterung laesst sich nicht installieren.' }
    Write-Host 'In VS Code installiert. Offene VS-Code-Fenster einmal neu laden (Strg+Shift+P, "Reload Window").'
}
