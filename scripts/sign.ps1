# Called by Tauri for each file it signs (see build-local.ps1).
param([Parameter(Mandatory)][string]$File)
$cert = Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert | Where-Object Subject -eq 'CN=Maze Cat Local Signing' | Select-Object -First 1
if (-not $cert) { throw 'No Maze Cat signing certificate. Run scripts\make-local-cert.ps1 as admin first.' }
$sig = Set-AuthenticodeSignature -FilePath $File -Certificate $cert -HashAlgorithm SHA256
if ($sig.Status -ne 'Valid') { throw "Signing $File failed: $($sig.StatusMessage)" }
