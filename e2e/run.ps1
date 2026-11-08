# Builds the app, then runs the tests in a VM through Docker in WSL. Arguments go to the test
# executable, so `run.ps1 tray` runs only the tests whose names contain "tray".
$ErrorActionPreference = 'Stop'
# From e2e/ so cargo uses .cargo/config.toml: a static C runtime, and run.sh as the test runner.
Set-Location $PSScriptRoot
cargo build --manifest-path ..\Cargo.toml --release --locked
if ($LASTEXITCODE) { exit $LASTEXITCODE }
cargo test --locked -- @args
exit $LASTEXITCODE
