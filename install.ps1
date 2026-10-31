# Installs bench51 (the bench51-gui desktop app and the bench51 terminal app)
# from the GitHub releases, on Windows.
#
#   irm https://raw.githubusercontent.com/krolyxon/bench51/master/install.ps1 | iex
#
# With options:
#
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/krolyxon/bench51/master/install.ps1))) -Version v0.2.0
#
# Options: -Version <tag> (default latest), -InstallDir <path> (default
# %LOCALAPPDATA%\Programs\bench51), -Archive <bench51-<target>.zip>,
# -NoShortcut, -Uninstall. BENCH51_VERSION and BENCH51_INSTALL_DIR set the
# defaults for -Version and -InstallDir when piping into iex.

param(
    [string]$Version = $(if ($env:BENCH51_VERSION) { $env:BENCH51_VERSION } else { 'latest' }),
    [string]$InstallDir = $(if ($env:BENCH51_INSTALL_DIR) { $env:BENCH51_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\bench51' }),
    [string]$Archive,
    [switch]$NoShortcut,
    [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'
# Invoke-WebRequest is much slower with the progress bar on.
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$Repo = 'krolyxon/bench51'
$Asset = 'bench51-x86_64-pc-windows-msvc.zip'
$Shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'bench51.lnk'

function Write-Step([string]$Message) {
    Write-Host "bench51: $Message"
}

# Adds or removes $Dir in the user's PATH.
function Edit-UserPath([string]$Dir, [bool]$Add) {
    $current = [Environment]::GetEnvironmentVariable('Path', 'User')
    $parts = @(($current -split ';') | Where-Object { $_ -and ($_.TrimEnd('\') -ne $Dir.TrimEnd('\')) })
    if ($Add) {
        $parts += $Dir
    }
    [Environment]::SetEnvironmentVariable('Path', ($parts -join ';'), 'User')
}

if ($Uninstall) {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $InstallDir
    Remove-Item -Force -ErrorAction SilentlyContinue $Shortcut
    Edit-UserPath $InstallDir $false
    Write-Step "removed bench51 from $InstallDir"
    return
}

$Temp = Join-Path ([IO.Path]::GetTempPath()) ("bench51-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $Temp | Out-Null
try {
    if (-not $Archive) {
        if ($Version -eq 'latest') {
            $Base = "https://github.com/$Repo/releases/latest/download"
        } else {
            if (-not $Version.StartsWith('v')) {
                $Version = "v$Version"
            }
            $Base = "https://github.com/$Repo/releases/download/$Version"
        }
        Write-Step "downloading $Asset ($Version)"
        $Archive = Join-Path $Temp $Asset
        $Sums = Join-Path $Temp 'SHA256SUMS'
        Invoke-WebRequest -UseBasicParsing -Uri "$Base/$Asset" -OutFile $Archive
        Invoke-WebRequest -UseBasicParsing -Uri "$Base/SHA256SUMS" -OutFile $Sums
        $line = Get-Content $Sums | Where-Object { $_ -match ('\s\*?' + [regex]::Escape($Asset) + '$') } | Select-Object -First 1
        if (-not $line) {
            throw "$Asset is not listed in SHA256SUMS"
        }
        $expected = ($line -split '\s+')[0].ToLowerInvariant()
        $actual = (Get-FileHash -Algorithm SHA256 -Path $Archive).Hash.ToLowerInvariant()
        if ($actual -ne $expected) {
            throw "checksum mismatch for $Asset"
        }
    }
    if (-not (Test-Path $Archive)) {
        throw "no such archive: $Archive"
    }

    $Extracted = Join-Path $Temp 'x'
    Expand-Archive -Path $Archive -DestinationPath $Extracted -Force
    $gui = Get-ChildItem -Path $Extracted -Recurse -Filter 'bench51-gui.exe' | Select-Object -First 1
    if (-not $gui) {
        throw 'the archive has no bench51-gui.exe'
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item -Path (Join-Path $gui.DirectoryName '*') -Destination $InstallDir -Recurse -Force
    Edit-UserPath $InstallDir $true
    $env:Path = "$env:Path;$InstallDir"
    Write-Step "installed bench51 and bench51-gui to $InstallDir"

    if (-not $NoShortcut) {
        $shell = New-Object -ComObject WScript.Shell
        $link = $shell.CreateShortcut($Shortcut)
        $link.TargetPath = Join-Path $InstallDir 'bench51-gui.exe'
        $link.WorkingDirectory = $env:USERPROFILE
        $link.Description = 'Intel 8051 assembler, simulator and debugger'
        $link.Save()
        Write-Step 'added bench51 to the Start menu'
    }

    & (Join-Path $InstallDir 'bench51.exe') --version
    Write-Step 'open a new terminal to use bench51 and bench51-gui from PATH'
} finally {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $Temp
}
