# Runs in the desktop session: executes each inbox\*.ps1 from the shared folder and writes outbox\<name>.out.
$share = '\\host.lan\Data'
while (-not (Test-Path "$share\inbox")) { Start-Sleep 5 }
Set-Content "$share\agent.ready" "$(whoami) session=$((Get-Process -Id $PID).SessionId)"
while ($true) {
    foreach ($job in Get-ChildItem "$share\inbox\*.ps1" -ErrorAction SilentlyContinue) {
        $out = "$share\outbox\$($job.BaseName).out"
        $local = Join-Path $env:TEMP $job.Name
        Move-Item $job.FullName $local -Force
        # Redirected to a file, not a pipe: a pipe would stay open for as long as any process the job started lives.
        $command = "powershell.exe -NoProfile -STA -ExecutionPolicy Bypass -File `"$local`" > `"$out`" 2>&1"
        $process = Start-Process cmd.exe -ArgumentList "/d /s /c `"$command`"" -WindowStyle Hidden -PassThru
        $process.WaitForExit()
        Set-Content "$out.done" 'done'
    }
    Start-Sleep 1
}
