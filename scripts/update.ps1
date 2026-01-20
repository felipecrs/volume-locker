$DownloadUrl = $env:VL_DOWNLOAD_URL
$ExePath = $env:VL_EXE_PATH

$Host.UI.RawUI.WindowTitle = 'Volume Locker Update'
Write-Host "Updating Volume Locker..." -ForegroundColor Cyan

$TempDownload = "$ExePath.download"

Write-Host "Downloading '$DownloadUrl'..." -ForegroundColor Yellow
& curl.exe --fail --silent --show-error --location --progress-bar --output $TempDownload $DownloadUrl
if (-not $?) {
    Write-Host 'Update failed!' -ForegroundColor Red
    Write-Host 'Please try again or download manually from GitHub.' -ForegroundColor Red

    Remove-Item $TempDownload -Force

    Write-Host 'Press any key to close...'
    $null = $Host.UI.RawUI.ReadKey('NoEcho,IncludeKeyDown')
    exit 1
}

Write-Host 'Stopping Volume Locker...' -ForegroundColor Yellow
Get-Process | Where-Object { $_.Path -eq $ExePath } | Stop-Process -ErrorAction SilentlyContinue

Write-Host "Replacing '$ExePath'..." -ForegroundColor Yellow
Move-Item -Path $TempDownload -Destination $ExePath -Force

Write-Host 'Starting Volume Locker...' -ForegroundColor Yellow
Start-Process $ExePath -ErrorAction SilentlyContinue

Write-Host 'Done! This window will close in 5 seconds...' -ForegroundColor Green
Start-Sleep -Seconds 5
