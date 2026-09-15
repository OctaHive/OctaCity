[CmdletBinding()]
param(
  [string]$ServiceName = "OctaCityAgent",
  [ValidateRange(1, 86400)]
  [int]$StopTimeoutSeconds = 300
)

$ErrorActionPreference = "Stop"
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  throw "uninstall-service.ps1 must run from an elevated PowerShell session"
}
if ($ServiceName -notmatch '^[A-Za-z0-9][A-Za-z0-9_.-]{0,255}$') {
  throw "ServiceName must start with an ASCII letter or digit and contain at most 256 letters, digits, dots, underscores, or hyphens"
}

$service = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($service) {
  if ($service.Status -ne "Stopped") {
    Stop-Service -Name $ServiceName
    # This operator-supplied bound must cover the configured graceful cancel,
    # cleanup, and durable completion delivery budgets.
    $service.WaitForStatus("Stopped", [TimeSpan]::FromSeconds($StopTimeoutSeconds))
  }
  & sc.exe delete $ServiceName | Out-Null
  if ($LASTEXITCODE -ne 0) {
    throw "failed to delete service '$ServiceName' (sc.exe exit code $LASTEXITCODE)"
  }
}
if ([Diagnostics.EventLog]::SourceExists($ServiceName)) {
  Remove-EventLog -Source $ServiceName
}
