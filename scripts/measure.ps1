# Measures what VoiceBro costs while it runs, per process (voicebro.exe, its codex.exe child
# and that child's helpers, the WebView2 processes it owns) and the CPU share of voicebro.exe
# over a sampling window.
#
# The headline is the private working set: what Task Manager shows as "Memory". Private
# bytes (the commit charge, which counts memory the process reserved but never touches) and
# the working set (which counts pages shared with other processes) are printed next to it
# for diagnosis; neither is the number to compare with Task Manager.
#
# Run it against a release build while the app is in the state to measure (idle in always
# mode, idle in wake-word mode, during a call); repeat three times and record the numbers in
# docs/MVP-PLAN.md. A debug build is refused unless -AllowDebug is given: it is several
# times larger and says nothing about what the user runs.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\measure.ps1 [-Seconds 10] [-Label "idle always"] [-AllowDebug]

param(
    [int]$Seconds = 10,
    [string]$Label = "",
    [switch]$AllowDebug
)

$ErrorActionPreference = "Stop"

function Mb($bytes) { [math]::Round($bytes / 1MB, 1) }

$app = Get-Process -Name voicebro -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $app) {
    Write-Output "voicebro.exe is not running"
    exit 1
}
$exe = $app.Path
if ($exe -match "\\debug\\" -and -not $AllowDebug) {
    Write-Output "refusing to measure a debug build ($exe); build the release (cargo build --release -p voicebro-desktop) or pass -AllowDebug"
    exit 2
}

# The build stamp: the first line of the log in the data folder (VOICEBRO_DATA_DIR of the
# process when this shell set it, else the folder next to the executable).
$dataDir = if ($env:VOICEBRO_DATA_DIR) { $env:VOICEBRO_DATA_DIR } else { Split-Path $exe }
$log = Get-ChildItem (Join-Path $dataDir "logs") -Filter "voicebro*.log" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
$build = if ($log) { (Get-Content $log.FullName -TotalCount 1) -replace "^.*\]\s*", "" } else { "unknown (no log in $dataDir)" }

# Children of voicebro.exe (codex.exe, the WebView2 browser process) and their children.
$all = Get-CimInstance Win32_Process
function Descendants($parentId) {
    $children = $all | Where-Object { $_.ParentProcessId -eq $parentId }
    foreach ($child in $children) {
        $child
        Descendants $child.ProcessId
    }
}
$tree = @($all | Where-Object { $_.ProcessId -eq $app.Id }) + @(Descendants $app.Id)

$cpuBefore = $app.TotalProcessorTime
$started = Get-Date
Start-Sleep -Seconds $Seconds
$app.Refresh()
$cpuAfter = $app.TotalProcessorTime
$elapsed = ((Get-Date) - $started).TotalSeconds
$cores = [Environment]::ProcessorCount
$cpuPercentOneCore = [math]::Round((($cpuAfter - $cpuBefore).TotalSeconds / $elapsed) * 100, 2)
$cpuPercentMachine = [math]::Round($cpuPercentOneCore / $cores, 2)

# The private working set per process id, from the "Working Set - Private" counter.
$perf = @{}
Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | ForEach-Object { $perf[[int]$_.IDProcess] = $_.WorkingSetPrivate }

$rows = foreach ($process in $tree) {
    $live = Get-Process -Id $process.ProcessId -ErrorAction SilentlyContinue
    if ($live) {
        [pscustomobject]@{
            Process            = $process.Name
            Pid                = $process.ProcessId
            PrivateWorkingSetMB = Mb $perf[[int]$process.ProcessId]
            PrivateBytesMB     = Mb $live.PrivateMemorySize64
            WorkingSetMB       = Mb $live.WorkingSet64
        }
    }
}

$stamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
Write-Output "VoiceBro measurement $stamp $Label"
Write-Output "build: $build ($exe)"
Write-Output "voicebro.exe CPU over $([math]::Round($elapsed, 1)) s: $cpuPercentOneCore % of one core ($cpuPercentMachine % of $cores cores)"
$rows | Format-Table -AutoSize | Out-String | Write-Output
function Sum($filter, $column) { [math]::Round((($rows | Where-Object { $_.Process -like $filter } | Measure-Object -Property $column -Sum).Sum), 1) }
foreach ($column in "PrivateWorkingSetMB", "PrivateBytesMB") {
    Write-Output ("{0}: voicebro {1}; codex {2}; webview2 {3}; total {4}" -f $column, (Sum "voicebro*" $column), (Sum "codex*" $column), (Sum "msedgewebview2*" $column), (Sum "*" $column))
}
