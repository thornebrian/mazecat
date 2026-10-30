# One time, in an admin PowerShell: makes a code-signing certificate that only this
# computer trusts, for signing your own Maze Cat builds until the Microsoft Store one.
# The private key stays in your user's certificate store and can't be exported.
#Requires -RunAsAdministrator
$subject = 'CN=Maze Cat Local Signing'
$cert = Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert | Where-Object Subject -eq $subject | Select-Object -First 1
if (-not $cert) {
  $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $subject `
    -CertStoreLocation Cert:\CurrentUser\My -KeyExportPolicy NonExportable `
    -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 -NotAfter (Get-Date).AddYears(2)
}
$cer = Join-Path $env:TEMP 'mazecat-local.cer'
Export-Certificate -Cert $cert -FilePath $cer | Out-Null
Import-Certificate -FilePath $cer -CertStoreLocation Cert:\LocalMachine\Root | Out-Null
Import-Certificate -FilePath $cer -CertStoreLocation Cert:\LocalMachine\TrustedPublisher | Out-Null
Remove-Item $cer
Write-Host "Ready: $subject (thumbprint $($cert.Thumbprint)), trusted on this computer only."
