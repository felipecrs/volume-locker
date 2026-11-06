# Builds the app and the tests on Windows, then runs them in a VM through Docker in WSL.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
# The VM has no Visual C++ runtime installed.
$env:RUSTFLAGS = '-C target-feature=+crt-static'
$target = Join-Path $PSScriptRoot 'target'

cargo build --manifest-path "$root\Cargo.toml" --release --locked --target-dir $target
if ($LASTEXITCODE) { exit $LASTEXITCODE }
$tests = cargo test --manifest-path "$PSScriptRoot\Cargo.toml" --locked --target-dir $target --no-run --message-format=json-render-diagnostics
if ($LASTEXITCODE) { exit $LASTEXITCODE }
$testExe = ($tests | ConvertFrom-Json | Where-Object { $_.target.name -eq 'app' -and $_.executable }).executable

$paths = "$PSScriptRoot\run.sh", $testExe, "$target\release\VolumeLocker.exe" | ForEach-Object { (wsl.exe wslpath -u ($_ -replace '\\', '/')) -replace "`0", '' }
wsl.exe bash @paths
exit $LASTEXITCODE
