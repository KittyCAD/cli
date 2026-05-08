[CmdletBinding()]
param(
    [string]$BinaryPath = (Join-Path (Get-Location) "target/debug/zoo.exe"),
    [string]$Target = "x86_64-pc-windows-gnu",
    [string]$Name = "zoo",
    [string]$Version = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-LastExitCode {
    param([string]$CommandName)

    if ($LASTEXITCODE -ne 0) {
        throw "$CommandName failed with exit code $LASTEXITCODE"
    }
}

function Get-FreeTcpPort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    try {
        $listener.Start()
        return $listener.LocalEndpoint.Port
    }
    finally {
        $listener.Stop()
    }
}

function Get-PythonCommand {
    $python = Get-Command python -ErrorAction SilentlyContinue
    if ($python) {
        return @{
            FilePath = $python.Source
            PrefixArgs = @()
        }
    }

    $py = Get-Command py -ErrorAction SilentlyContinue
    if ($py) {
        return @{
            FilePath = $py.Source
            PrefixArgs = @("-3")
        }
    }

    throw "python is required to serve the local release artifact"
}

function Get-CargoPackageVersion {
    param(
        [string]$PackageName,
        [string]$ManifestPath
    )

    $metadataJson = & cargo metadata --manifest-path $ManifestPath --no-deps --format-version 1
    Assert-LastExitCode "cargo metadata"

    $metadata = $metadataJson | ConvertFrom-Json
    $package = $metadata.packages | Where-Object { $_.name -eq $PackageName } | Select-Object -First 1
    if (-not $package) {
        throw "could not find package '$PackageName' in cargo metadata"
    }

    return $package.version
}

function Wait-ForArtifact {
    param(
        [string]$Uri,
        [string]$ServerStdout,
        [string]$ServerStderr
    )

    for ($attempt = 0; $attempt -lt 60; $attempt++) {
        try {
            $response = Invoke-WebRequest -Uri $Uri -Method Head -TimeoutSec 2
            if ($response.StatusCode -eq 200) {
                return
            }
        }
        catch {
            Start-Sleep -Milliseconds 500
        }
    }

    $stdout = if (Test-Path $ServerStdout) { Get-Content $ServerStdout -Raw } else { "" }
    $stderr = if (Test-Path $ServerStderr) { Get-Content $ServerStderr -Raw } else { "" }
    throw "local artifact server did not serve $Uri`nstdout:`n$stdout`nstderr:`n$stderr"
}

function Get-PowerShellBlock {
    param([string]$Markdown)

    $match = [regex]::Match($Markdown, "(?s)```powershell\r?\n(?<script>.*?)\r?\n```")
    if (-not $match.Success) {
        throw "generated instructions did not contain a PowerShell code block"
    }

    return $match.Groups["script"].Value
}

function Assert-PathEntryCount {
    param(
        [string]$PathValue,
        [string]$Directory,
        [int]$ExpectedCount
    )

    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        $PathValue = ""
    }

    $normalizedDirectory = $Directory.TrimEnd('\')
    $matches = @(
        $PathValue -split ';' |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
            Where-Object { $_.TrimEnd('\') -ieq $normalizedDirectory }
    )

    if ($matches.Count -ne $ExpectedCount) {
        throw "expected $Directory to appear in Path $ExpectedCount time(s), found $($matches.Count)"
    }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$renderScript = Join-Path $repoRoot "scripts/release-install-instructions.sh"
if (-not (Test-Path $renderScript)) {
    throw "missing release instructions renderer: $renderScript"
}

if (-not (Test-Path $BinaryPath)) {
    throw "missing binary to install: $BinaryPath"
}
$binaryPath = (Resolve-Path $BinaryPath).Path

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "v$(Get-CargoPackageVersion -PackageName $Name -ManifestPath (Join-Path $repoRoot "Cargo.toml"))"
}
elseif (-not $Version.StartsWith("v")) {
    $Version = "v$Version"
}

$testRoot = Join-Path ([System.IO.Path]::GetTempPath()) "zoo-install-directions-$([System.Guid]::NewGuid())"
$releaseRoot = Join-Path $testRoot "release-root"
$versionDir = Join-Path (Join-Path (Join-Path $releaseRoot "releases") "cli") $Version
$releaseBinary = Join-Path $versionDir "$Name-$Target"
$serverStdout = Join-Path $testRoot "http.out.log"
$serverStderr = Join-Path $testRoot "http.err.log"
$server = $null
$originalLocalAppData = $env:LOCALAPPDATA
$originalProcessPath = $env:Path
$originalUserPath = [Environment]::GetEnvironmentVariable("Path", [EnvironmentVariableTarget]::User)

try {
    New-Item -ItemType Directory -Force -Path $versionDir | Out-Null
    Copy-Item -Force -Path $binaryPath -Destination $releaseBinary
    $expectedHash = (Get-FileHash -Algorithm SHA256 $releaseBinary).Hash.ToLowerInvariant()

    $port = Get-FreeTcpPort
    $pythonCommand = Get-PythonCommand
    $serverArgs = @($pythonCommand.PrefixArgs) + @(
        "-m",
        "http.server",
        [string]$port,
        "--bind",
        "127.0.0.1"
    )
    $server = Start-Process `
        -FilePath $pythonCommand.FilePath `
        -ArgumentList $serverArgs `
        -WorkingDirectory $releaseRoot `
        -PassThru `
        -RedirectStandardOutput $serverStdout `
        -RedirectStandardError $serverStderr

    $baseUrl = "http://127.0.0.1:$port/releases/cli"
    $artifactUrl = "$baseUrl/$Version/$Name-$Target"
    Wait-ForArtifact -Uri $artifactUrl -ServerStdout $serverStdout -ServerStderr $serverStderr

    $bash = Get-Command bash -ErrorAction SilentlyContinue
    if (-not $bash) {
        throw "bash is required to render the release install instructions"
    }

    $instructions = & $($bash.Source) $renderScript $Target $Version $Name $expectedHash $baseUrl
    Assert-LastExitCode "release install instruction rendering"
    $instructionsPath = Join-Path $testRoot "README.md"
    Set-Content -Path $instructionsPath -Value $instructions -Encoding utf8

    if ($instructions -match "/usr/local/bin|chmod|sha256sum -c") {
        throw "Windows instructions still contain Unix install commands"
    }

    $installScript = Get-PowerShellBlock -Markdown ($instructions -join "`n")
    Set-Content -Path (Join-Path $testRoot "install.ps1") -Value $installScript -Encoding utf8

    $testLocalAppData = Join-Path $testRoot "local-app-data"
    New-Item -ItemType Directory -Force -Path $testLocalAppData | Out-Null

    $env:LOCALAPPDATA = $testLocalAppData
    $env:Path = [Environment]::GetEnvironmentVariable("Path", [EnvironmentVariableTarget]::Machine)
    [Environment]::SetEnvironmentVariable("Path", (Join-Path $testRoot "user-bin"), [EnvironmentVariableTarget]::User)

    Invoke-Expression $installScript
    Invoke-Expression $installScript

    $installDir = Join-Path $testLocalAppData "Zoo"
    $installedCli = Join-Path $installDir "zoo.exe"
    if (-not (Test-Path $installedCli)) {
        throw "install instructions did not create $installedCli"
    }

    $installedHash = (Get-FileHash -Algorithm SHA256 $installedCli).Hash.ToLowerInvariant()
    if ($installedHash -ne $expectedHash) {
        throw "installed binary hash mismatch"
    }

    $userPathAfterInstall = [Environment]::GetEnvironmentVariable("Path", [EnvironmentVariableTarget]::User)
    Assert-PathEntryCount -PathValue $userPathAfterInstall -Directory $installDir -ExpectedCount 1
    Assert-PathEntryCount -PathValue $env:Path -Directory $installDir -ExpectedCount 1

    & $installedCli -h | Out-Null
    Assert-LastExitCode "$installedCli -h"

    & $Name -h | Out-Null
    Assert-LastExitCode "$Name -h from Path"

    Write-Host "Windows install instructions installed $Name from $artifactUrl"
}
finally {
    $env:LOCALAPPDATA = $originalLocalAppData
    $env:Path = $originalProcessPath
    [Environment]::SetEnvironmentVariable("Path", $originalUserPath, [EnvironmentVariableTarget]::User)

    if ($server -and -not $server.HasExited) {
        Stop-Process -Id $server.Id -Force
    }
}
