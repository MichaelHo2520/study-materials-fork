param(
    [Parameter(Mandatory=$true)][string]$Apk,
    [Parameter(Mandatory=$true)][string]$Output,
    [string]$Sdk = "$env:LOCALAPPDATA\Android\Sdk",
    [string]$JavaBin = 'C:\Program Files\Android\Android Studio\jbr\bin',
    [string]$SigningDir = "$env:LOCALAPPDATA\HGhubSigning"
)
$ErrorActionPreference = 'Stop'
# Windows PowerShell 5.1 ships ProtectedData in System.Security; PowerShell 7 has its own assembly.
try { Add-Type -AssemblyName System.Security.Cryptography.ProtectedData } catch { Add-Type -AssemblyName System.Security }
$scope = [System.Security.Cryptography.DataProtectionScope]::CurrentUser
$key = Join-Path $SigningDir 'personal.jks'
$secret = Join-Path $SigningDir 'password.dpapi'
$tools = Get-ChildItem -LiteralPath (Join-Path $Sdk 'build-tools') -Directory |
    Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
if (!$tools) { throw 'Android build-tools are required' }
function Run([string]$Executable, [string[]]$Arguments) {
    & $Executable @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Tool failed: $Executable ($LASTEXITCODE)" }
}
New-Item -ItemType Directory -Force -Path $SigningDir | Out-Null
$aligned = [System.IO.Path]::GetFullPath($Output) + '.aligned.apk'
try {
    if (!(Test-Path -LiteralPath $key)) {
        $password = [Convert]::ToBase64String([System.Security.Cryptography.RandomNumberGenerator]::GetBytes(32))
        $protected = [System.Security.Cryptography.ProtectedData]::Protect([Text.Encoding]::UTF8.GetBytes($password), $null, $scope)
        [IO.File]::WriteAllBytes($secret, $protected)
    } else {
        $password = [Text.Encoding]::UTF8.GetString([System.Security.Cryptography.ProtectedData]::Unprotect([IO.File]::ReadAllBytes($secret), $null, $scope))
    }
    $env:HGHUB_SIGN_PASS = $password
    if (!(Test-Path -LiteralPath $key)) {
        Run (Join-Path $JavaBin 'keytool.exe') @('-genkeypair','-keystore',$key,'-storetype','JKS','-alias','hghub-personal',
            '-storepass:env','HGHUB_SIGN_PASS','-keypass:env','HGHUB_SIGN_PASS','-keyalg','RSA','-keysize','3072',
            '-validity','10000','-dname','CN=HGhub Personal','-noprompt')
    }
    Run (Join-Path $tools.FullName 'zipalign.exe') @('-f','-P','16','4',[IO.Path]::GetFullPath($Apk),$aligned)
    $env:JAVA_HOME = Split-Path $JavaBin
    Run (Join-Path $tools.FullName 'apksigner.bat') @('sign','--ks',$key,'--ks-key-alias','hghub-personal',
        '--ks-pass','env:HGHUB_SIGN_PASS','--key-pass','env:HGHUB_SIGN_PASS','--v4-signing-enabled','false',
        '--out',[IO.Path]::GetFullPath($Output),$aligned)
    Run (Join-Path $tools.FullName 'apksigner.bat') @('verify','--verbose','--print-certs',[IO.Path]::GetFullPath($Output))
} finally {
    Remove-Item Env:\HGHUB_SIGN_PASS -ErrorAction SilentlyContinue
    $password = $null
    if (Test-Path -LiteralPath $aligned) { Remove-Item -LiteralPath $aligned }
}
