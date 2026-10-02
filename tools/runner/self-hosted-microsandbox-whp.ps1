$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$MicrosandboxVersion = "0.7.6"
$MicrosandboxArchiveSha256 = "78fe36cf700d9c888e041373e2826bca7d553da2374b4d8172d12edae333ef4c"
$WorkspaceBytes = 1073741824
$Image = "quay.io/fedora/fedora@sha256:63773f454664cd77e239f8e0b13ae7f18effe9e3d6612a325b5646eb3bda11f1"

function Fail([string]$Message) {
  throw "Windows WHP setup: $Message"
}

function Get-Root {
  if ([string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) {
    Fail "RUNNER_TEMP is required"
  }
  return Join-Path $env:RUNNER_TEMP "octacity-self-hosted-microsandbox-whp"
}

function Assert-OwnedRoot([string]$Root) {
  $expected = [IO.Path]::GetFullPath((Join-Path $env:RUNNER_TEMP "octacity-self-hosted-microsandbox-whp"))
  $actual = [IO.Path]::GetFullPath($Root)
  if ($actual -ne $expected -or -not (Test-Path -LiteralPath (Join-Path $actual ".octacity-whp"))) {
    Fail "refusing to operate on an unowned staging root: $actual"
  }
}

function Add-Environment([string[]]$Values) {
  if ([string]::IsNullOrWhiteSpace($env:GITHUB_ENV)) {
    Fail "GITHUB_ENV is required"
  }
  Add-Content -LiteralPath $env:GITHUB_ENV -Value $Values -Encoding utf8
}

function Download-VerifiedArchive([string]$Destination) {
  $uri = "https://github.com/superradcompany/microsandbox/releases/download/v$MicrosandboxVersion/microsandbox-windows-x86_64.zip"
  for ($attempt = 1; $attempt -le 5; $attempt++) {
    try {
      Invoke-WebRequest -Uri $uri -OutFile $Destination -UseBasicParsing
      break
    } catch {
      if ($attempt -eq 5) { throw }
      Start-Sleep -Seconds ([math]::Pow(2, $attempt - 1))
    }
  }
  $actual = (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actual -ne $MicrosandboxArchiveSha256) {
    Fail "Microsandbox archive checksum mismatch"
  }
}

function Setup([string]$OctaVersion, [string]$OctaRevision) {
  if ([Environment]::Is64BitOperatingSystem -eq $false -or $env:PROCESSOR_ARCHITECTURE -notmatch "AMD64") {
    Fail "the qualified preview requires Windows x86_64"
  }
  $root = Get-Root
  if (Test-Path -LiteralPath $root) {
    Fail "staging root already exists: $root"
  }
  New-Item -ItemType Directory -Path $root, (Join-Path $root "runtime"), (Join-Path $root "work"), (Join-Path $root "state") | Out-Null
  Set-Content -LiteralPath (Join-Path $root ".octacity-whp") -Value "microsandbox-whp" -NoNewline
  $archive = Join-Path $root "microsandbox.zip"
  Download-VerifiedArchive $archive
  Expand-Archive -LiteralPath $archive -DestinationPath (Join-Path $root "runtime")
  $msb = Get-ChildItem -LiteralPath (Join-Path $root "runtime") -Filter "msb.exe" -File -Recurse | Select-Object -First 1
  $firmware = Get-ChildItem -LiteralPath (Join-Path $root "runtime") -Filter "libkrunfw*.dll" -File -Recurse | Select-Object -First 1
  if ($null -eq $msb -or $null -eq $firmware) {
    Fail "Microsandbox bundle is incomplete"
  }
  $reportedVersion = (& $msb.FullName --version | Out-String).Trim()
  if ($reportedVersion -ne "msb $MicrosandboxVersion") {
    Fail "unexpected Microsandbox version: $reportedVersion"
  }
  & $msb.FullName doctor
  if ($LASTEXITCODE -ne 0) { Fail "Microsandbox doctor rejected this WHP host" }

  $repository = (git rev-parse --show-toplevel).Trim()
  if ([string]::IsNullOrWhiteSpace($repository)) { Fail "run inside the OctaCity checkout" }
  $octaRoot = (& python "$repository/tools/stage_octa_release.py" `
    --version $OctaVersion `
    --revision $OctaRevision `
    --platform linux-amd64 `
    --output (Join-Path $root "octa-release") | Select-Object -Last 1).Trim()
  if (-not (Test-Path -LiteralPath $octaRoot -PathType Container)) {
    Fail "the pinned Linux guest Octa release was not staged"
  }
  Add-Environment @(
    "OCTACITY_CONTRACT_OCTA_RELEASE_ROOT=$octaRoot",
    "OCTACITY_CONTRACT_WORKSPACE_BYTES=$WorkspaceBytes",
    "OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT=$(Join-Path $root 'work')",
    "OCTACITY_CONTRACT_MICROSANDBOX_STATE_ROOT=$(Join-Path $root 'state')",
    "OCTACITY_CONTRACT_MICROSANDBOX_EXECUTABLE=$($msb.FullName)",
    "OCTACITY_CONTRACT_MICROSANDBOX_LIBKRUNFW=$($firmware.FullName)",
    "OCTACITY_CONTRACT_MICROSANDBOX_ENVIRONMENT_IDENTITY=microsandbox-$MicrosandboxVersion-linux-x86_64",
    "OCTACITY_CONTRACT_MICROSANDBOX_IMAGE=$Image",
    "OCTACITY_CONTRACT_MICROSANDBOX_ALLOWED_HOST=example.com",
    "OCTACITY_CONTRACT_MICROSANDBOX_DENIED_HOST=www.cloudflare.com"
  )
}

function Verify-Clean {
  $root = Get-Root
  Assert-OwnedRoot $root
  if ((Get-ChildItem -LiteralPath (Join-Path $root "work") -Force | Measure-Object).Count -ne 0) {
    Fail "Microsandbox workspaces remain after the contract"
  }
}

function Cleanup {
  $root = Get-Root
  if (-not (Test-Path -LiteralPath $root)) { return }
  Assert-OwnedRoot $root
  Remove-Item -LiteralPath $root -Recurse -Force
}

if ($args.Count -lt 1) { Fail "usage: self-hosted-microsandbox-whp.ps1 <setup|verify-clean|cleanup> [Octa version] [Octa revision]" }
switch ($args[0]) {
  "setup" {
    if ($args.Count -ne 3) { Fail "setup requires the Octa version and revision" }
    Setup $args[1] $args[2]
  }
  "verify-clean" { Verify-Clean }
  "cleanup" { Cleanup }
  default { Fail "unsupported command '$($args[0])'" }
}
