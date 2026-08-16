param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$SmokeArgs
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$SmoquePackage = "smoque@0.1.2"
$InstallRoot = $null
$PreviousLocation = Get-Location

function Invoke-Checked {
    param(
        [string]$Command,
        [string[]]$Arguments
    )
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Command failed with status $LASTEXITCODE"
    }
}

try {
    Set-Location $RepoRoot
    if ([string]::IsNullOrWhiteSpace($env:NPM_CONFIG_CACHE)) {
        $env:NPM_CONFIG_CACHE = Join-Path ([IO.Path]::GetTempPath()) "zhold-smoque-npm-cache"
    }

    $First = if ($SmokeArgs.Count -eq 0) { "" } else { $SmokeArgs[0] }
    if ($First -eq "types") {
        Invoke-Checked "node" @("scripts/check-smoke-types.mjs")
        return
    }
    if ($First -match "^(doctor|list|--help|-h|--version)$") {
        Invoke-Checked "npx.cmd" (@("--yes", $SmoquePackage) + $SmokeArgs)
        return
    }

    if ([string]::IsNullOrWhiteSpace($env:ZHOLD_SMOKE_BIN)) {
        $InstallRoot = Join-Path ([IO.Path]::GetTempPath()) (
            "zhold-smoke-install-" + [guid]::NewGuid().ToString("N")
        )
        New-Item -ItemType Directory -Path $InstallRoot | Out-Null
        Invoke-Checked "cargo" @(
            "install",
            "--path", (Join-Path $RepoRoot "crates/zhold-cli"),
            "--locked",
            "--root", $InstallRoot
        )
        $env:ZHOLD_SMOKE_BIN = Join-Path $InstallRoot "bin/zhold.exe"
    }

    if ($First -match "\.smoke\.mts$") {
        $RunArgs = @("run") + $SmokeArgs
    } else {
        Invoke-Checked "node" @("scripts/check-smoke-types.mjs")
        $RunArgs = @("run", "smoke/") + $SmokeArgs
    }
    if (-not [string]::IsNullOrWhiteSpace($env:CI)) {
        $RunArgs += "--ci"
    }
    Invoke-Checked "npx.cmd" (@("--yes", $SmoquePackage) + $RunArgs)
} finally {
    Set-Location $PreviousLocation
    if ($null -ne $InstallRoot -and (Test-Path $InstallRoot)) {
        Remove-Item -Recurse -Force $InstallRoot
    }
}
