param(
    [string]$Python = "python",
    [string]$Output = "windows-verification"
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$Repo = Split-Path -Parent $PSScriptRoot
Set-Location $Repo
if (Test-Path $Output) { throw "Result directory already exists: $Output" }
if ($env:OS -ne "Windows_NT") { throw "Run this script on native Windows." }

function Run-Checked([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Command failed: $Program ($LASTEXITCODE)" }
}

# Python >=3.9 and stable Rust (including MSVC build prerequisites) are required.
$env:PYTHONUTF8 = "1"
Run-Checked $Python @("-m", "venv", ".venv-windows")
$VenvPython = Join-Path $Repo ".venv-windows\Scripts\python.exe"
$env:VIRTUAL_ENV = Join-Path $Repo ".venv-windows"
$env:PYO3_PYTHON = $VenvPython
Run-Checked $VenvPython @("-m", "pip", "install", "--upgrade", "pip", "maturin>=1,<2")
Run-Checked "cargo" @("fmt", "--check")
Run-Checked "cargo" @("clippy", "--all-targets", "--all-features", "--", "-D", "warnings")
Run-Checked "cargo" @("test")
Run-Checked $VenvPython @("-m", "maturin", "develop")
Run-Checked $VenvPython @("-m", "unittest", "discover", "-s", "tests", "-p", "test_*.py", "-v")
Run-Checked $VenvPython @("scripts/verify_sfc_platform.py", "--output", $Output)
Write-Host "Results: $Output\verification.json"
Write-Host "Open $Output\created.sfc in your CAD, inspect it, save as cad_saved.sfc, then reopen it."
Write-Host "This script verifies filesystem saving. CAD interoperability requires that additional manual check."
