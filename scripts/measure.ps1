# Measures what VoiceBro costs while it runs: private bytes per process (voicebro.exe, its
# codex.exe child, the WebView2 processes it owns) and the CPU share of voicebro.exe over a
# sampling window. Run it while the app is in the state to measure (idle in always mode,
# idle in wake-word mode, during a call); repeat three times and record the numbers in
# docs/MVP-PLAN.md.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\measure.ps1 [-Seconds 10] [-Label "idle always"]

param(
    [int]$Seconds = 10,
    [string]$Label = ""
)

$ErrorActionPreference = "Stop"

function Mb($bytes) { [math]::Round($bytes / 1MB, 1) }

$app = Get-Process -Name voicebro -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $app) {
    Write-Output "voicebro.exe is not running"
    exit 1
}

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

$rows = foreach ($process in $tree) {
    $live = Get-Process -Id $process.ProcessId -ErrorAction SilentlyContinue
    if ($live) {
        [pscustomobject]@{
            Process      = $process.Name
            Pid          = $process.ProcessId
            PrivateMB    = Mb $live.PrivateMemorySize64
            WorkingSetMB = Mb $live.WorkingSet64
        }
    }
}

$stamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
Write-Output "VoiceBro measurement $stamp $Label"
Write-Output "voicebro.exe CPU over $([math]::Round($elapsed, 1)) s: $cpuPercentOneCore % of one core ($cpuPercentMachine % of $cores cores)"
$rows | Format-Table -AutoSize | Out-String | Write-Output
$total = ($rows | Measure-Object -Property PrivateMB -Sum).Sum
$webview = ($rows | Where-Object { $_.Process -like "msedgewebview2*" } | Measure-Object -Property PrivateMB -Sum).Sum
$codex = ($rows | Where-Object { $_.Process -like "codex*" } | Measure-Object -Property PrivateMB -Sum).Sum
$main = ($rows | Where-Object { $_.Process -like "voicebro*" } | Measure-Object -Property PrivateMB -Sum).Sum
Write-Output ("private MB: voicebro {0}; codex {1}; webview2 {2}; total {3}" -f $main, $codex, $webview, $total)
