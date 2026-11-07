$ErrorActionPreference = 'Stop'

cargo build --release --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Get-FileHash -LiteralPath .\Cargo.lock -Algorithm SHA256
Get-FileHash -LiteralPath .\target\release\rust-switcher.exe -Algorithm SHA256
