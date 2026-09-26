$ErrorActionPreference = "Stop"

cargo build --release --locked
Get-FileHash .\Cargo.lock -Algorithm SHA256
Get-FileHash .\target\release\rust-switcher.exe -Algorithm SHA256
