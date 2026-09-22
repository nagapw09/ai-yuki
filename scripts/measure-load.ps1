$allProcesses = Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name
$ids = @($allProcesses | Where-Object Name -eq 'yuki-desktop.exe' | ForEach-Object ProcessId)
for ($depth=0; $depth -lt 6; $depth++) { $ids = @($ids + @($allProcesses | Where-Object { $ids -contains $_.ParentProcessId } | ForEach-Object ProcessId) | Sort-Object -Unique) }
$before = @(Get-Process -Id $ids -ErrorAction SilentlyContinue)
$cpuBefore = ($before | Measure-Object CPU -Sum).Sum
$watch = [Diagnostics.Stopwatch]::StartNew()
Start-Sleep -Seconds 8
$after = @(Get-Process -Id $ids -ErrorAction SilentlyContinue)
$seconds = $watch.Elapsed.TotalSeconds
[pscustomobject]@{ Processes=$after.Count; CPUPercent=[Math]::Round((($after | Measure-Object CPU -Sum).Sum-$cpuBefore)/$seconds/[Environment]::ProcessorCount*100,2); WorkingSetMB=[Math]::Round(($after | Measure-Object WorkingSet64 -Sum).Sum/1MB); PrivateMB=[Math]::Round(($after | Measure-Object PrivateMemorySize64 -Sum).Sum/1MB); SampleSeconds=[Math]::Round($seconds,1) } | ConvertTo-Json
