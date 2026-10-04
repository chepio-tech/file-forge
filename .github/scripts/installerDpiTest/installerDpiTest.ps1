param([string]$Makensis = "$env:LOCALAPPDATA\tauri\NSIS\makensis.exe")

$ErrorActionPreference = "Stop"
if (!(Test-Path $Makensis)) { throw "Tauri's NSIS compiler is missing: $Makensis. Run the Windows bundle build first." }
$repositoryRoot = (Resolve-Path "$PSScriptRoot\..\..\..").Path
$temporaryRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
$testDirectory = Join-Path $temporaryRoot "fileforge-installer-dpi"
New-Item -ItemType Directory -Force $testDirectory | Out-Null

# Core
Add-Type -AssemblyName System.Drawing
# An 8×8 black/white checker detects unfiltered downsampling: only filtered pixels contain gray.
$bitmap = [System.Drawing.Bitmap]::new(8, 8, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
try {
    for ($y = 0; $y -lt 8; $y++) {
        for ($x = 0; $x -lt 8; $x++) {
            $color = if (($x + $y) % 2) { [System.Drawing.Color]::White } else { [System.Drawing.Color]::Black }
            $bitmap.SetPixel($x, $y, $color)
        }
    }
    $bitmap.Save((Join-Path $testDirectory "checker.bmp"), [System.Drawing.Imaging.ImageFormat]::Bmp)
} finally { $bitmap.Dispose() }

foreach ($mode in @("stock", "smooth")) {
    $arguments = @("/V2", "/WX", "/DREPOSITORY_ROOT=$repositoryRoot", "/DTEST_DIRECTORY=$testDirectory", "/DTEST_MODE=$mode")
    if ($mode -eq "smooth") { $arguments += "/DSMOOTH" }
    & $Makensis @arguments "$PSScriptRoot\installerDpiTest.nsi"
    if ($LASTEXITCODE -ne 0) { throw "NSIS $mode compilation failed: $LASTEXITCODE" }
    $process = Start-Process (Join-Path $testDirectory "$mode.exe") -ArgumentList "/S" -PassThru
    if (!$process.WaitForExit(30000)) { $process.Kill(); throw "NSIS $mode test timed out" }
    if ($process.ExitCode -ne 0) { throw "NSIS $mode execution failed: $($process.ExitCode)" }
}

$stock = Get-Content (Join-Path $testDirectory "stock.txt")
$smooth = Get-Content (Join-Path $testDirectory "smooth.txt")
if ($stock.Count -ne 4 -or $smooth.Count -ne 4) { throw "Incomplete DPI reports" }
for ($index = 0; $index -lt 3; $index++) {
    $before = $stock[$index].Split(',') | ForEach-Object { [int]$_ }
    $after = $smooth[$index].Split(',') | ForEach-Object { [int]$_ }
    $size = 4 + 2 * $index
    if ($after[0] -ne $size -or $after[1] -ne $size -or $after[2] -ne $size) { throw "Filtered bitmap doesn't match $size-pixel control: $($smooth[$index])" }
    if ($before[3] -ne 0) { throw "Checker fixture should contain only black and white before filtering" }
    if ($size -lt 8 -and $after[3] -le 0) { throw "No antialiasing at $($size * 25)%: $($smooth[$index])" }
    if ($size -eq 8 -and $after[3] -ne 0) { throw "200% must preserve the source pixels" }
    Write-Output "PASS: $($size * 25)%: exact ${size}x${size} bitmap; $($after[3]) filtered edge pixels"
}
$gdiDelta = [int]$smooth[3].Split(',')[1]
if ($gdiDelta -gt 2) { throw "GDI resources leaked after 120 resizes: $gdiDelta" }
Write-Output "PASS: 120 repeated resizes retain no GDI resources (delta $gdiDelta)"
