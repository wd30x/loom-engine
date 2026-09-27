#Requires -Version 7
# Smoke test on real hardware: GPU rendering, window exit paths, keyboard
# record/replay and minimized CPU use. Needs a GPU; the W group also needs a
# desktop session. Not part of verify.ps1.
#
# Usage: pwsh scripts/smoke.ps1 [-HeadlessOnly]
# Prints one PASS/FAIL line per check, then "smoke: N passed, M failed".
# Exits 1 if anything failed.

param([switch]$HeadlessOnly)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root
$clock = [Diagnostics.Stopwatch]::StartNew()

cargo build -p loom_engine
if ($LASTEXITCODE -ne 0) { Write-Host 'smoke: build failed'; exit 1 }
$exe = Join-Path $root 'target/debug/loom.exe'

$out = 'out/smoke'
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force $out | Out-Null

if (-not ('SmokeWin32' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class SmokeWin32 {
    public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int max);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd, int cmd);

    // The top-level window of `pid` titled `title` (visible only, unless
    // `anyVisibility`). Matching on the title matters: winit also creates a
    // hidden helper window.
    public static IntPtr FindWindow(uint pid, string title, bool anyVisibility = false) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((hwnd, _) => {
            uint owner;
            GetWindowThreadProcessId(hwnd, out owner);
            if (owner != pid || (!anyVisibility && !IsWindowVisible(hwnd))) return true;
            var text = new StringBuilder(256);
            GetWindowText(hwnd, text, text.Capacity);
            if (text.ToString() != title) return true;
            found = hwnd;
            return false;
        }, IntPtr.Zero);
        return found;
    }
}
'@
}

if (-not ('SmokeImage' -as [type])) {
    Add-Type -TypeDefinition @'
using System;

public static class SmokeImage {
    // Compares two images of `width` pixels per row, 4 bytes per pixel in
    // the order B, G, R, A (GDI+ Format32bppArgb), rows packed. Counts the
    // pixels that differ at all and the largest channel difference; returns
    // null, or a description of the first pixel (row by row) where some
    // channel differs by more than `tolerance`.
    public static string Compare(byte[] a, byte[] b, int width, int tolerance, out int differing, out int maxDiff) {
        differing = 0; maxDiff = 0;
        string error = null;
        for (int i = 0; i < a.Length; i += 4) {
            int worst = 0;
            for (int c = 0; c < 4; c++) worst = Math.Max(worst, Math.Abs(a[i + c] - b[i + c]));
            if (worst == 0) continue;
            differing++;
            maxDiff = Math.Max(maxDiff, worst);
            if (worst > tolerance && error == null) {
                int p = i / 4;
                error = string.Format("pixel ({0}, {1}) is RGBA {2} vs {3}, {4} apart (at most {5} allowed)",
                    p % width, p / width, Rgba(a, i), Rgba(b, i), worst, tolerance);
            }
        }
        return error;
    }

    static string Rgba(byte[] px, int i) {
        return string.Format("({0},{1},{2},{3})", px[i + 2], px[i + 1], px[i], px[i + 3]);
    }
}
'@
}

$WM_KEYDOWN = 0x0100; $WM_KEYUP = 0x0101; $WM_CLOSE = 0x0010
$SW_MINIMIZE = 6; $SW_RESTORE = 9
# Virtual-key code and scan code; winit reads the physical key from the scan code.
$keyCodes = @{ W = @(0x57, 0x11); A = @(0x41, 0x1E); D = @(0x44, 0x20); L = @(0x4C, 0x26); Q = @(0x51, 0x10); Space = @(0x20, 0x39); Esc = @(0x1B, 0x01) }
# opengl32.dll is not listed: like dxgi.dll it is a static import of loom.exe
# (wgpu's GL backend links it), so it is always loaded. It is only the GL
# loader; the actual GL driver DLL is caught by the vendor prefixes below.
$gpuDlls = @('vulkan-1.dll', 'd3d12.dll', 'd3d12core.dll', 'd3d11.dll')
$gpuDriverPrefixes = @('nv', 'amd', 'ig')

# Start-LoomWindow waits for loom_render's "surface format" info line; pin the
# default filter so an outer RUST_LOG cannot hide it.
$defaultLog = 'warn,loom=info,loom_render=info,loom_platform=info'
$env:RUST_LOG = $defaultLog

$script:passed = 0
$script:failed = 0

# Runs one check. $body throws to fail; its message becomes the reason.
# Measurements the body stores in $script:detail are appended to the line.
function Test-Check([string]$id, [string]$desc, [scriptblock]$body) {
    $script:detail = ''
    try {
        $null = & $body
        $script:passed++
        Write-Host "PASS $id $desc$(if ($script:detail) { " ($script:detail)" })"
    } catch {
        $script:failed++
        Write-Host "FAIL $id ${desc}: $($_.Exception.Message)$(if ($script:detail) { " ($script:detail)" })"
    }
}

function Assert-True([bool]$condition, [string]$message) {
    if (-not $condition) { throw $message }
}

# Runs loom to completion. Returns exit code and the parsed stdout JSON.
function Invoke-Loom([string[]]$loomArgs) {
    $stdout = & $exe @loomArgs 2>$null
    $code = $LASTEXITCODE
    $json = if ($stdout) { $stdout | ConvertFrom-Json } else { $null }
    [pscustomobject]@{ Code = $code; Json = $json }
}

function Get-PngSize([string]$path) {
    $bytes = [IO.File]::ReadAllBytes($path)
    $w = ([int]$bytes[16] -shl 24) -bor ([int]$bytes[17] -shl 16) -bor ([int]$bytes[18] -shl 8) -bor $bytes[19]
    $h = ([int]$bytes[20] -shl 24) -bor ([int]$bytes[21] -shl 16) -bor ([int]$bytes[22] -shl 8) -bor $bytes[23]
    "${w}x${h}"
}

function Get-ModuleNames($process) {
    $process.Refresh()
    @($process.Modules | ForEach-Object { $_.ModuleName.ToLowerInvariant() })
}

function Test-GpuModule([string]$name) {
    ($gpuDlls -contains $name) -or @($gpuDriverPrefixes | Where-Object { $name.StartsWith($_) }).Count -gt 0
}

# Starts `loom run <args>` in the background with stdout/stderr in files.
# Window runs get --no-capture: the desktop mouse stays free, and moving it
# cannot leak into recordings.
function Start-Loom([string]$name, [string[]]$loomArgs) {
    $stdout = Join-Path $out "$name.stdout.txt"
    $stderr = Join-Path $out "$name.stderr.txt"
    if ($loomArgs -notcontains '--headless') { $loomArgs = @($loomArgs) + '--no-capture' }
    $process = Start-Process $exe -ArgumentList (@('run') + $loomArgs) -PassThru -NoNewWindow `
        -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    $null = $process.Handle   # keep a handle so ExitCode stays readable after exit
    [pscustomobject]@{ Process = $process; Stdout = $stdout; Stderr = $stderr; Window = [IntPtr]::Zero }
}

# The last log line (they start with `[`; continuation lines do not) of a
# run's stderr, to show where a stalled run got stuck.
function Get-LastLogLine($run) {
    $lines = @(Get-Content $run.Stderr -ErrorAction SilentlyContinue | Where-Object { $_ -match '^\[' })
    if ($lines.Count) { $lines[-1] } else { '(no log yet)' }
}

# Starts a windowed run and waits (up to 10 s) for its window, then (up to
# $rendererTimeoutS) for the renderer: the fixed sleeps of the checks count
# from there. Renderer creation sometimes stalls for minutes (G1.4); that is
# reported as such instead of surfacing as some later assertion.
function Start-LoomWindow([string]$name, [string[]]$loomArgs) {
    $rendererTimeoutS = 30
    $run = Start-Loom $name $loomArgs
    $deadline = (Get-Date).AddSeconds(10)
    while ($run.Window -eq [IntPtr]::Zero) {
        if ((Get-Date) -ge $deadline) {
            Stop-Loom $run
            throw 'loom window did not appear within 10 s'
        }
        if ($run.Process.HasExited) { throw "loom exited early with code $($run.Process.ExitCode)" }
        Start-Sleep -Milliseconds 100
        $run.Window = [SmokeWin32]::FindWindow([uint32]$run.Process.Id, 'loom')
    }
    $deadline = (Get-Date).AddSeconds($rendererTimeoutS)
    while (-not (Select-String -Path $run.Stderr -Pattern 'surface format' -Quiet -ErrorAction SilentlyContinue)) {
        if ($run.Process.HasExited) { throw "loom exited early with code $($run.Process.ExitCode); last log line: $(Get-LastLogLine $run)" }
        if ((Get-Date) -ge $deadline) {
            $last = Get-LastLogLine $run
            Stop-Loom $run
            throw "renderer init stalled: no `"surface format`" after $rendererTimeoutS s; last log line: $last"
        }
        Start-Sleep -Milliseconds 50
    }
    $run
}

# Kills the process if it is still running. A process stuck in the driver
# may not die at once: give up after 10 s instead of blocking smoke.
function Stop-Loom($run) {
    if ($run -and -not $run.Process.HasExited) {
        $run.Process.Kill()
        if (-not $run.Process.WaitForExit(10000)) {
            Write-Host "     note: process $($run.Process.Id) did not exit 10 s after Kill (driver stall?)"
        }
    }
}

# Waits (up to 10 s) for a run to exit. Returns exit code and parsed JSON.
function Wait-Loom($run) {
    if (-not $run.Process.WaitForExit(10000)) {
        $last = Get-LastLogLine $run
        Stop-Loom $run
        throw "loom did not exit within 10 s; last log line: $last"
    }
    $text = Get-Content $run.Stdout -Raw
    $json = if ($text) { $text | ConvertFrom-Json } else { $null }
    [pscustomobject]@{ Code = $run.Process.ExitCode; Json = $json; Stderr = $run.Stderr }
}

function Send-Key($run, [string]$key, [bool]$down) {
    $vk, $scan = $keyCodes[$key]
    $lParam = [int64](1 -bor ($scan -shl 16))
    if ($down) {
        [void][SmokeWin32]::PostMessage($run.Window, $WM_KEYDOWN, [IntPtr]$vk, [IntPtr]$lParam)
    } else {
        [void][SmokeWin32]::PostMessage($run.Window, $WM_KEYUP, [IntPtr]$vk, [IntPtr]($lParam -bor 0xC0000000L))
    }
}

function Send-Close($run) { [void][SmokeWin32]::PostMessage($run.Window, $WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) }
function Set-WindowState($run, [int]$cmd) { [void][SmokeWin32]::ShowWindow($run.Window, $cmd) }

function Assert-ExitOk($result) {
    if ($result.Code -ne 0) {
        $tail = if ($result.PSObject.Properties['Stderr'] -and (Test-Path $result.Stderr)) {
            (Get-Content $result.Stderr -Tail 3) -join ' | '
        }
        throw "exit code $($result.Code), stdout: $($result.Json | ConvertTo-Json -Compress), stderr tail: $tail"
    }
    Assert-True ($null -ne $result.Json -and $result.Json.status -eq 'ok') 'status is not ok'
}

# Headless replay of `script` for `ticks` ticks; returns the state file path.
function Invoke-HeadlessReplay([string]$script, [int64]$ticks, [string]$state) {
    $r = Invoke-Loom @('run', '--headless', '--frames', "$ticks", '--input', $script, '--dump-state', $state)
    Assert-ExitOk $r
    $state
}

# Headless run of `scene` for `ticks` ticks; returns the state file path.
function Invoke-HeadlessReplayScene([string]$scene, [int64]$ticks, [string]$state) {
    $r = Invoke-Loom @('run', '--headless', '--frames', "$ticks", '--scene', $scene, '--dump-state', $state)
    Assert-ExitOk $r
    $state
}

# The events of `axis` in a recording, in file order: Tick and Value.
function Get-AxisEvents([string]$recording, [string]$axis) {
    [regex]::Matches((Get-Content $recording -Raw), "tick: (\d+), axis: `"$axis`", value: ([^)]+)\)") |
        ForEach-Object { [pscustomobject]@{ Tick = [int64]$_.Groups[1].Value; Value = [double]$_.Groups[2].Value } }
}

function Assert-SameBytes([string]$a, [string]$b) {
    $ha = (Get-FileHash $a).Hash; $hb = (Get-FileHash $b).Hash
    Assert-True ($ha -eq $hb) "$a ($ha) differs from $b ($hb)"
}

# The pixels of $bitmap as bytes B, G, R, A, rows packed (LockBits: GetPixel
# is far too slow for a whole screenshot).
function Get-BgraBytes($bitmap) {
    $rect = [Drawing.Rectangle]::new(0, 0, $bitmap.Width, $bitmap.Height)
    $data = $bitmap.LockBits($rect, [Drawing.Imaging.ImageLockMode]::ReadOnly,
        [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        Assert-True ($data.Stride -eq 4 * $bitmap.Width) "unexpected stride $($data.Stride)"
        $bytes = [byte[]]::new($data.Stride * $bitmap.Height)
        [Runtime.InteropServices.Marshal]::Copy($data.Scan0, $bytes, 0, $bytes.Length)
        , $bytes
    } finally {
        $bitmap.UnlockBits($data)
    }
}

# PNGs $a and $b have the same size and every channel (R, G, B, A) of every
# pixel differs by at most $tolerance. Returns how many pixels differ and by
# how much, for the check's detail; the error names the first pixel over.
function Assert-NearlySameImage([string]$a, [string]$b, [int]$tolerance) {
    Add-Type -AssemblyName System.Drawing
    $ia = [Drawing.Bitmap]::new((Join-Path $root $a))
    $ib = [Drawing.Bitmap]::new((Join-Path $root $b))
    try {
        Assert-True ($ia.Width -eq $ib.Width -and $ia.Height -eq $ib.Height) `
            "$a is $($ia.Width)x$($ia.Height), $b is $($ib.Width)x$($ib.Height)"
        $differing = 0; $maxDiff = 0
        $error1 = [SmokeImage]::Compare((Get-BgraBytes $ia), (Get-BgraBytes $ib), $ia.Width, $tolerance,
            [ref]$differing, [ref]$maxDiff)
        Assert-True ($null -eq $error1) "$a vs ${b}: $error1"
        $pixels = if ($differing -eq 1) { '1 pixel differs' } else { "$differing pixels differ" }
        "$pixels, by at most $maxDiff"
    } finally {
        $ia.Dispose()
        $ib.Dispose()
    }
}

# ---------------------------------------------------------------- H: headless

Test-Check 'H1' 'headless screenshots are byte-identical and 1280x720' {
    foreach ($n in 'a', 'b') {
        $r = Invoke-Loom @('run', '--headless', '--frames', '60', '--screenshot', "$out/h1_$n.png")
        Assert-ExitOk $r
        Assert-True ($r.Json.adapter.backend -eq 'Dx12') "backend is $($r.Json.adapter.backend), expected the default Dx12"
    }
    Assert-SameBytes "$out/h1_a.png" "$out/h1_b.png"
    $size = Get-PngSize "$out/h1_a.png"
    Assert-True ($size -eq '1280x720') "PNG is $size"
}

Test-Check 'H2' '--size 640x360 gives a 640x360 PNG' {
    $r = Invoke-Loom @('run', '--headless', '--frames', '60', '--size', '640x360', '--screenshot', "$out/h2.png")
    Assert-ExitOk $r
    $size = Get-PngSize "$out/h2.png"
    Assert-True ($size -eq '640x360') "PNG is $size"
}

Test-Check 'H3' 'headless run without screenshot loads no GPU driver' {
    $run = Start-Loom 'h3' @('--headless', '--frames', '4000000000')
    try {
        Start-Sleep -Milliseconds 1500
        Assert-True (-not $run.Process.HasExited) 'the long run exited early'
        $gpu = @(Get-ModuleNames $run.Process | Where-Object { Test-GpuModule $_ })
        Assert-True ($gpu.Count -eq 0) "loaded $($gpu -join ', ')"
    } finally {
        Stop-Loom $run
    }
}

Test-Check 'H4' 'WGPU_BACKEND=vulkan overrides the default backend and renders the image to within 1 per channel' {
    Assert-True (Test-Path "$out/h1_a.png") 'no screenshot from H1'
    $env:WGPU_BACKEND = 'vulkan'
    try {
        $r = Invoke-Loom @('run', '--headless', '--frames', '60', '--screenshot', "$out/h4.png")
    } finally {
        Remove-Item Env:WGPU_BACKEND -ErrorAction SilentlyContinue
    }
    Assert-ExitOk $r
    Assert-True ($r.Json.adapter.backend -eq 'Vulkan') "backend is $($r.Json.adapter.backend), expected Vulkan"
    # Backends may round differently; only the same machine and driver
    # promise identical bytes (docs/architecture.md section 4).
    $script:detail = Assert-NearlySameImage "$out/h1_a.png" "$out/h4.png" 1
}

Test-Check 'H5' 'serve screenshots match loom run and reuse the renderer' {
    Assert-True (Test-Path "$out/h1_a.png") 'no screenshot from H1'
    $requests = @(
        '{"jsonrpc":"2.0","id":1,"method":"step","params":{"ticks":60}}'
        "{`"jsonrpc`":`"2.0`",`"id`":2,`"method`":`"screenshot`",`"params`":{`"path`":`"$out/h5_a.png`"}}"
        "{`"jsonrpc`":`"2.0`",`"id`":3,`"method`":`"screenshot`",`"params`":{`"path`":`"$out/h5_b.png`"}}"
        "{`"jsonrpc`":`"2.0`",`"id`":4,`"method`":`"screenshot`",`"params`":{`"path`":`"$out/h5_c.png`",`"size`":[640,360]}}"
        '{"jsonrpc":"2.0","id":5,"method":"quit"}'
    )
    $lines = @($requests | & $exe serve 2>$null)
    Assert-True ($LASTEXITCODE -eq 0) "loom serve exit code $LASTEXITCODE"
    Assert-True ($lines.Count -eq $requests.Count) "expected $($requests.Count) replies, got $($lines.Count): $($lines -join ' | ')"
    $replies = @($lines | ForEach-Object { $_ | ConvertFrom-Json })
    foreach ($reply in $replies) {
        Assert-True ($null -eq $reply.PSObject.Properties['error']) "error reply: $($reply | ConvertTo-Json -Compress -Depth 5)"
    }
    $ms = @($replies[1..3] | ForEach-Object { $_.result.render_ms })
    $script:detail = 'render_ms A {0}, B {1}, C {2} (640x360)' -f $ms[0], $ms[1], $ms[2]
    Assert-SameBytes "$out/h1_a.png" "$out/h5_a.png"
    Assert-SameBytes "$out/h1_a.png" "$out/h5_b.png"
    $size = Get-PngSize "$out/h5_c.png"
    Assert-True ($size -eq '640x360') "C is $size"
    Assert-True ($ms[1] -lt $ms[0] / 2) "B took $($ms[1]) ms, not under half of A's $($ms[0]) ms: the renderer was not reused?"
}

Test-Check 'H6' 'serve session without screenshot loads no GPU driver' {
    $psi = [Diagnostics.ProcessStartInfo]::new($exe, 'serve')
    $psi.UseShellExecute = $false
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.Environment['RUST_LOG'] = 'warn'   # keep smoke's own output clean
    $process = [Diagnostics.Process]::Start($psi)
    try {
        # Stdin stays open: the session must still be running when checked.
        $process.StandardInput.WriteLine('{"jsonrpc":"2.0","id":1,"method":"describe"}')
        $process.StandardInput.Flush()
        $read = $process.StandardOutput.ReadLineAsync()
        Assert-True ($read.Wait(10000)) 'no reply to describe within 10 s'
        Assert-True ($read.Result -like '{"jsonrpc":"2.0","id":1,"result":{"tick_hz":60,*') "unexpected reply: $($read.Result)"
        Assert-True (-not $process.HasExited) 'loom serve exited with stdin still open'
        $gpu = @(Get-ModuleNames $process | Where-Object { Test-GpuModule $_ })
        Assert-True ($gpu.Count -eq 0) "loaded $($gpu -join ', ')"
    } finally {
        if (-not $process.HasExited) {
            $process.Kill()
            $null = $process.WaitForExit(10000)
        }
    }
}

# Through scripts/mcp.ps1, as .mcp.json starts it, so this covers the script too.
Test-Check 'H7' 'mcp screenshot returns the PNG inline' {
    Assert-True (Test-Path "$out/h1_a.png") 'no screenshot from H1'
    $shot = Join-Path $root 'out/mcp/shot-1.png'
    if (Test-Path $shot) { Remove-Item $shot }
    $psi = [Diagnostics.ProcessStartInfo]::new('pwsh')
    foreach ($a in '-NoProfile', '-File', 'scripts/mcp.ps1') { $psi.ArgumentList.Add($a) }
    $psi.WorkingDirectory = $root
    $psi.UseShellExecute = $false
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.Environment['RUST_LOG'] = 'warn'   # keep smoke's own output clean
    $process = [Diagnostics.Process]::Start($psi)
    $send = { param($message) $process.StandardInput.WriteLine(($message | ConvertTo-Json -Compress -Depth 10)); $process.StandardInput.Flush() }
    # The reply with `id`, waiting at most `seconds` for each line.
    $receive = {
        param([int]$id, [int]$seconds)
        while ($true) {
            $read = $process.StandardOutput.ReadLineAsync()
            if (-not $read.Wait($seconds * 1000)) { throw "no reply to request $id within $seconds s" }
            if ($null -eq $read.Result) { throw "loom-mcp closed stdout before replying to request $id" }
            $reply = $read.Result | ConvertFrom-Json -Depth 20
            if ($reply.PSObject.Properties['id'] -and $reply.id -eq $id) { return $reply }
        }
    }
    try {
        & $send @{ jsonrpc = '2.0'; id = 1; method = 'initialize'; params = @{
                protocolVersion = '2025-11-25'; capabilities = @{}; clientInfo = @{ name = 'smoke'; version = '0' } } }
        # The first reply may wait for mcp.ps1's cargo build.
        $init = & $receive 1 180
        Assert-True ($init.result.serverInfo.name -eq 'loom-mcp') "unexpected initialize reply: $($init | ConvertTo-Json -Compress -Depth 10)"
        & $send @{ jsonrpc = '2.0'; method = 'notifications/initialized' }
        & $send @{ jsonrpc = '2.0'; id = 2; method = 'tools/call'; params = @{ name = 'step'; arguments = @{ ticks = 60 } } }
        $step = & $receive 2 60
        Assert-True (-not $step.result.isError -and $step.result.content[0].text -eq '{"tick":60,"events":[]}') "step: $($step | ConvertTo-Json -Compress -Depth 10)"
        & $send @{ jsonrpc = '2.0'; id = 3; method = 'tools/call'; params = @{ name = 'screenshot'; arguments = @{} } }
        $reply = & $receive 3 60
        $content = @($reply.result.content)
        Assert-True (-not $reply.result.isError -and $content.Count -eq 2) "screenshot: $($content[0].text)"
        $image = $content[1]
        Assert-True ($image.type -eq 'image' -and $image.mimeType -eq 'image/png') "second content is $($image.type) $($image.mimeType)"
        $bytes = [Convert]::FromBase64String($image.data)
        [IO.File]::WriteAllBytes((Join-Path $root "$out/h7_inline.png"), $bytes)
        $script:detail = '{0} PNG bytes inline; {1}' -f $bytes.Length, $content[0].text
        Assert-SameBytes "$out/h1_a.png" "$out/h7_inline.png"
        Assert-True (Test-Path $shot) 'out/mcp/shot-1.png was not written'
        Assert-SameBytes "$out/h1_a.png" $shot
        $process.StandardInput.Close()
        Assert-True ($process.WaitForExit(20000)) 'loom-mcp did not exit within 20 s after stdin closed'
        Assert-True ($process.ExitCode -eq 0) "mcp.ps1 exit code $($process.ExitCode)"
    } finally {
        if (-not $process.HasExited) {
            $process.Kill($true)
            $null = $process.WaitForExit(10000)
        }
    }
}

# The frame {x, y, w, h} of `tag` in assets/art/tiles.json.
function Get-TileFrame([string]$tag) {
    $sheetJson = Get-Content -Raw (Join-Path $root 'assets/art/tiles.json') | ConvertFrom-Json
    $from = ($sheetJson.meta.frameTags | Where-Object name -EQ $tag).from
    $sheetJson.frames[$from].frame
}

# Checks the 64x64 cell at screen (left, top) of the screenshot $png pixel
# by pixel against the sheet frame $top (from Get-TileFrame) scaled 4x:
# every sheet pixel must become one 4x4 block. Where $top is transparent,
# the frame $under shows instead (pass $null for an opaque $top). No
# tolerance: the sRGB round trip must be exact.
function Assert-TileCell([string]$png, [int]$left, [int]$top, $frame, $under, [string]$what) {
    Add-Type -AssemblyName System.Drawing
    $sheetImage = [Drawing.Bitmap]::new((Join-Path $root 'assets/art/tiles.png'))
    $shot = [Drawing.Bitmap]::new((Join-Path $root $png))
    try {
        $bad = 0; $first = ''
        for ($y = 0; $y -lt 64; $y++) {
            for ($x = 0; $x -lt 64; $x++) {
                $sx = [int][Math]::Floor($x / 4); $sy = [int][Math]::Floor($y / 4)
                $want = $sheetImage.GetPixel($frame.x + $sx, $frame.y + $sy)
                if ($null -ne $under -and $want.A -eq 0) {
                    $want = $sheetImage.GetPixel($under.x + $sx, $under.y + $sy)
                }
                $got = $shot.GetPixel($left + $x, $top + $y)
                if ($got.ToArgb() -ne $want.ToArgb()) {
                    if ($bad -eq 0) { $first = "($($left + $x),$($top + $y)) is $($got.Name), expected $($want.Name)" }
                    $bad++
                }
            }
        }
        Assert-True ($bad -eq 0) "${what}: $bad of 4096 pixels differ, first $first"
    } finally {
        $sheetImage.Dispose()
        $shot.Dispose()
    }
}

Test-Check 'H8' 'view renders the tile preview pixel-exact' {
    foreach ($n in 'a', 'b') {
        $r = Invoke-Loom @('view', '--scene', 'assets/scenes/tiles_preview.ron', '--screenshot', "$out/h8_$n.png")
        Assert-ExitOk $r
        Assert-True ($r.Json.mode -eq 'view') "mode is $($r.Json.mode)"
    }
    Assert-SameBytes "$out/h8_a.png" "$out/h8_b.png"
    $size = Get-PngSize "$out/h8_a.png"
    Assert-True ($size -eq '1280x720') "PNG is $size"
    $script:detail = "sha256 $((Get-FileHash "$out/h8_a.png").Hash)"
    # 64 screen pixels per cell: row r, column c is [64c, 64c+64) x [64r, 64r+64).
    Assert-TileCell "$out/h8_a.png" 0 0 (Get-TileFrame 'wall') $null 'wall cell (row 0, column 0)'
    Assert-TileCell "$out/h8_a.png" 192 128 (Get-TileFrame 'player') (Get-TileFrame 'floor') 'player cell (row 2, column 3)'
}

Test-Check 'H9' 'rogue run moves, fights and leaves a corpse' {
    # The CLI test's script (rogue_replay_fights_a_zombie_to_the_prototype_state): the k-th of
    # 13 presses on tick 2k-1, released on 2k: wait x5, move_e x4, move_n, wait x3.
    $keys = @('wait') * 5 + @('move_e') * 4 + @('move_n') + @('wait') * 3
    $events = @(
        for ($k = 1; $k -le $keys.Count; $k++) {
            "(tick: $(2 * $k - 1), action: `"$($keys[$k - 1])`", down: true)"
            "(tick: $(2 * $k), action: `"$($keys[$k - 1])`", down: false)"
        }
    ) -join ', '
    $script = "$out/h9_script.ron"
    [IO.File]::WriteAllText((Join-Path $root $script), "(version: 1, tick_hz: 60, events: [$events])`n")
    $r = Invoke-Loom @('run', '--headless', '--frames', '28', '--scene', 'assets/scenes/rogue.ron',
        '--input', $script, '--screenshot', "$out/h9.png", '--dump-state', "$out/h9_state.json")
    Assert-ExitOk $r
    Assert-True ($r.Json.game -eq 'rogue') "game is $($r.Json.game)"
    $state = Get-Content -Raw "$out/h9_state.json" | ConvertFrom-Json
    $cell = $state.game.player.cell -join ','
    Assert-True ($cell -eq '4,1' -and $state.game.turn -eq 13) "state has cell $cell, turn $($state.game.turn)"
    $script:detail = "sha256 $((Get-FileHash "$out/h9.png").Hash)"
    $floor = Get-TileFrame 'floor'
    Assert-TileCell "$out/h9.png" 256 64 (Get-TileFrame 'player') $floor 'player cell (column 4, row 1)'
    # The player stood on the corpse but has left: only the corpse over floor.
    Assert-TileCell "$out/h9.png" 256 128 (Get-TileFrame 'corpse') $floor 'corpse cell (column 4, row 2)'
    Assert-TileCell "$out/h9.png" 320 64 (Get-TileFrame 'zombie') $floor 'zombie_2 cell (column 5, row 1)'
    Assert-TileCell "$out/h9.png" 192 128 $floor $null 'start cell (column 3, row 2)'
}

# ---------------------------------------------------------------- W: window

if (-not $HeadlessOnly) {
    Test-Check 'W0' 'window run loads a GPU driver (control for H3)' {
        $run = Start-LoomWindow 'w0' @()
        try {
            Start-Sleep -Milliseconds 1500
            $modules = Get-ModuleNames $run.Process
            Assert-True ($modules -contains 'd3d12.dll') 'd3d12.dll is not loaded'
            Send-Close $run
            Assert-ExitOk (Wait-Loom $run)
        } finally {
            Stop-Loom $run
        }
    }

    Test-Check 'W1' 'Esc exits 0 and saves the screenshot' {
        $run = Start-LoomWindow 'w1' @('--screenshot', "$out/w1.png")
        try {
            Start-Sleep -Milliseconds 1000
            Send-Key $run 'Esc' $true
            $r = Wait-Loom $run
            Assert-ExitOk $r
            Assert-True ($r.Json.adapter.backend -eq 'Dx12') "backend is $($r.Json.adapter.backend), expected the default Dx12"
            Assert-True ($null -ne $r.Json.screenshot) 'screenshot is null'
            Assert-True (Test-Path "$out/w1.png") 'PNG missing'
        } finally {
            Stop-Loom $run
        }
    }

    Test-Check 'W2' 'closing the window exits 0 and saves the screenshot' {
        $run = Start-LoomWindow 'w2' @('--screenshot', "$out/w2.png")
        try {
            Start-Sleep -Milliseconds 1000
            Send-Close $run
            $r = Wait-Loom $run
            Assert-ExitOk $r
            Assert-True ($null -ne $r.Json.screenshot) 'screenshot is null'
            Assert-True (Test-Path "$out/w2.png") 'PNG missing'
        } finally {
            Stop-Loom $run
        }
    }

    Test-Check 'W3' 'closing while minimized skips the screenshot but dumps the state' {
        $run = Start-LoomWindow 'w3' @('--screenshot', "$out/w3.png", '--dump-state', "$out/w3.json")
        try {
            Start-Sleep -Milliseconds 1000
            Set-WindowState $run $SW_MINIMIZE
            Start-Sleep -Milliseconds 1000
            Send-Close $run
            $r = Wait-Loom $run
            Assert-ExitOk $r
            Assert-True ($null -eq $r.Json.screenshot) 'screenshot is not null'
            Assert-True (-not (Test-Path "$out/w3.png")) 'PNG was written'
            Assert-True (Test-Path "$out/w3.json") 'state file missing'
        } finally {
            Stop-Loom $run
        }
    }

    Test-Check 'W4' 'rendering resumes after minimize and restore' {
        $run = Start-LoomWindow 'w4' @('--screenshot', "$out/w4.png")
        try {
            Start-Sleep -Milliseconds 1000
            Set-WindowState $run $SW_MINIMIZE
            Start-Sleep -Milliseconds 1000
            Set-WindowState $run $SW_RESTORE
            Start-Sleep -Milliseconds 1000
            Send-Key $run 'Esc' $true
            $r = Wait-Loom $run
            Assert-ExitOk $r
            Assert-True ($null -ne $r.Json.screenshot) 'screenshot is null: no frame rendered after restore'
        } finally {
            Stop-Loom $run
        }
    }

    Test-Check 'W5' 'keyboard recording replays headless to identical state' {
        $run = Start-LoomWindow 'w5' @('--record', "$out/w5.ron", '--dump-state', "$out/w5.json")
        try {
            Start-Sleep -Milliseconds 700
            Send-Key $run 'D' $true; Start-Sleep -Milliseconds 1000
            Send-Key $run 'W' $true; Start-Sleep -Milliseconds 500
            Send-Key $run 'W' $false; Send-Key $run 'D' $false; Start-Sleep -Milliseconds 300
            foreach ($i in 1..3) {
                Send-Key $run 'A' $true; Send-Key $run 'A' $false
                Start-Sleep -Milliseconds 250
            }
            Start-Sleep -Milliseconds 300
            Send-Key $run 'Esc' $true
            Assert-ExitOk (Wait-Loom $run)
        } finally {
            Stop-Loom $run
        }
        $tick = (Get-Content "$out/w5.json" -Raw | ConvertFrom-Json).tick
        $replay = Invoke-HeadlessReplay "$out/w5.ron" $tick "$out/w5_replay.json"
        Assert-SameBytes "$out/w5.json" $replay

        # Each A tap: move_x set to -1.0, then back to 0.0 in a later tick.
        $moveX = @(Get-AxisEvents "$out/w5.ron" 'move_x')
        $taps = 0
        for ($i = 0; $i -lt $moveX.Count; $i++) {
            if ($moveX[$i].Value -ne -1.0) { continue }
            $taps++
            Assert-True ($i + 1 -lt $moveX.Count -and $moveX[$i + 1].Value -eq 0.0) "move_x -1.0 at tick $($moveX[$i].Tick) is not followed by 0.0"
            Assert-True ($moveX[$i + 1].Tick -gt $moveX[$i].Tick) "tap $taps set and reset move_x in tick $($moveX[$i].Tick)"
        }
        Assert-True ($taps -eq 3) "expected 3 taps of A (move_x -1.0 then 0.0), found $taps"
    }

    Test-Check 'W6' 'window replay matches headless replay' {
        Assert-True (Test-Path "$out/w5.ron") 'no recording from W5'
        $run = Start-LoomWindow 'w6' @('--input', "$out/w5.ron", '--dump-state', "$out/w6.json")
        try {
            Start-Sleep -Milliseconds 1500
            Send-Key $run 'Esc' $true
            Assert-ExitOk (Wait-Loom $run)
        } finally {
            Stop-Loom $run
        }
        $tick = (Get-Content "$out/w6.json" -Raw | ConvertFrom-Json).tick
        Assert-True ($tick -gt 0) 'window replay ran 0 ticks'
        $replay = Invoke-HeadlessReplay "$out/w5.ron" $tick "$out/w6_replay.json"
        Assert-SameBytes "$out/w6.json" $replay
    }

    # W7 runs with debug logs (renderer and exit steps, with timestamps); if
    # it fails, its stderr is kept outside $out, which the next smoke clears.
    Test-Check 'W7' 'minimized window stays idle while ticks follow the wall clock' {
        $run = $null
        try {
            # The filter only applies to this process: restore it once started.
            $env:RUST_LOG = 'warn,loom=debug,loom_render=debug,loom_platform=info'
            try {
                $run = Start-LoomWindow 'w7' @('--dump-state', "$out/w7.json")
            } finally {
                $env:RUST_LOG = $defaultLog
            }
            Start-Sleep -Milliseconds 1000
            Set-WindowState $run $SW_MINIMIZE
            Start-Sleep -Milliseconds 200   # let the minimize take effect
            $run.Process.Refresh()
            $cpu0 = $run.Process.TotalProcessorTime
            $wall = [Diagnostics.Stopwatch]::StartNew()
            Start-Sleep -Milliseconds 3000
            $run.Process.Refresh()
            $cpu = ($run.Process.TotalProcessorTime - $cpu0).TotalSeconds / $wall.Elapsed.TotalSeconds
            $closedAt = Get-Date
            Send-Close $run
            $r = Wait-Loom $run
            Assert-ExitOk $r
            $lifetime = ($run.Process.ExitTime - $run.Process.StartTime).TotalSeconds
            $ratio = $r.Json.ticks / ($lifetime * 60)
            # Wall time not covered by ticks, not counting startup up to the
            # ready renderer (renderer_init_ms), which varies and sometimes
            # stalls. Had ticks stopped while minimized, the 3 s minimized
            # span would add to it. Of the rest, "after close" is the exit.
            $initS = $r.Json.renderer_init_ms / 1000
            $untracked = $lifetime - $initS - $r.Json.ticks / 60
            $exitS = ($run.Process.ExitTime - $closedAt).TotalSeconds
            $script:detail = 'minimized CPU {0:P1} of one core; {1} ticks in {2:N2} s wall = {3:N3} x 60 Hz; renderer_init_ms {5}; exit {6:N2} s after close; {4:N2} s not covered by ticks' -f `
                $cpu, $r.Json.ticks, $lifetime, $ratio, $untracked, $r.Json.renderer_init_ms, $exitS
            Assert-True ($cpu -lt 0.05) ('minimized CPU is {0:P1} of one core' -f $cpu)
            Assert-True ($untracked -ge 0 -and $untracked -lt 1.0) ('{0:N2} s of wall time not covered by ticks' -f $untracked)
        } catch {
            Stop-Loom $run
            $kept = 'out/smoke-w7-{0:yyyyMMdd-HHmmss}.log' -f (Get-Date)
            Copy-Item (Join-Path $out 'w7.stderr.txt') $kept -ErrorAction SilentlyContinue
            throw "$($_.Exception.Message); stderr kept in $kept"
        } finally {
            Stop-Loom $run
        }
    }

    Test-Check 'W8' 'scene hot reload restarts on a good edit and ignores a broken one' {
        $scene = "$out/w8.scene.ron"
        $demo = [IO.File]::ReadAllText((Join-Path $root 'games/souls/src/demo.scene.ron'))
        $playerAt = '(name: "player", position: (0.0, 0.0, 0.0)'
        Assert-True ($demo.Contains($playerAt)) 'the demo scene no longer has the expected player line'
        $good = $demo.Replace($playerAt, '(name: "player", position: (3.0, 0.0, 0.0)')
        $broken = $good.TrimEnd().TrimEnd(')')
        $utf8 = [Text.UTF8Encoding]::new($false)
        [IO.File]::WriteAllText($scene, $demo, $utf8)
        [IO.File]::WriteAllText("$out/w8_good.scene.ron", $good, $utf8)

        $run = Start-LoomWindow 'w8' @('--scene', $scene, '--dump-state', "$out/w8.json", '--trace', "$out/w8.jsonl")
        try {
            # 2 s, not 1: the window appears before the renderer is ready, and
            # the pre-edit ticks are what the restart check below relies on.
            Start-Sleep -Milliseconds 2000
            [IO.File]::WriteAllText($scene, $good, $utf8)     # reload: restart from tick 0
            $goodWrittenAt = [DateTime]::Now
            Start-Sleep -Milliseconds 1500
            [IO.File]::WriteAllText($scene, $broken, $utf8)   # reload error: keep running
            Start-Sleep -Milliseconds 1500
            Send-Key $run 'Esc' $true
            $r = Wait-Loom $run
            Assert-ExitOk $r
        } finally {
            Stop-Loom $run
        }
        $lifetime = ($run.Process.ExitTime - $run.Process.StartTime).TotalSeconds
        # The reload happens after the good edit (within one check interval),
        # so the final run cannot have more ticks than the wall time since
        # that edit. Without a restart it would also count the ~1 s before it.
        $sinceEdit = ($run.Process.ExitTime - $goodWrittenAt).TotalSeconds
        $maxTicks = [math]::Floor($sinceEdit * 60) + 3
        $state = Get-Content "$out/w8.json" -Raw | ConvertFrom-Json
        $script:detail = 'reloads {0}, reload_errors {1}; final run {2} ticks, at most {3} after the edit; {4:N2} s wall' -f `
            $r.Json.reloads, $r.Json.reload_errors, $state.tick, $maxTicks, $lifetime
        Assert-True ($r.Json.reloads -eq 1) "reloads is $($r.Json.reloads)"
        Assert-True ($r.Json.reload_errors -eq 1) "reload_errors is $($r.Json.reload_errors)"
        foreach ($pos in @(@($state.nodes.player.position), @($state.game.player.position))) {
            Assert-True (($pos -join ',') -eq '3,0,0') "player position is [$($pos -join ', ')]"
        }
        Assert-True ($state.tick -le $maxTicks) "tick $($state.tick) is more than the $maxTicks possible since the edit: the reload did not restart from tick 0"
        $replay = Invoke-HeadlessReplayScene "$out/w8_good.scene.ron" $state.tick "$out/w8_replay.json"
        Assert-SameBytes "$out/w8.json" $replay
        # The trace starts over too: only the final run, from its tick 0.
        Assert-True ($r.Json.trace.lines -eq $state.tick + 1) "trace has $($r.Json.trace.lines) lines for $($state.tick) ticks"
        $h = Invoke-Loom @('run', '--headless', '--frames', "$($state.tick)", '--scene', "$out/w8_good.scene.ron", '--trace', "$out/w8_replay.jsonl")
        Assert-ExitOk $h
        Assert-SameBytes "$out/w8.jsonl" "$out/w8_replay.jsonl"
    }

    Test-Check 'W9' 'exiting before the first frame still exits 0 with a JSON report' {
        $results = @()
        foreach ($i in 1..6) {
            $viaEsc = $i -le 3
            $run = Start-Loom "w9_$i" @()
            try {
                # Busy-wait, no visibility check: send the moment the window
                # exists, before the renderer can draw a first frame.
                $deadline = [Diagnostics.Stopwatch]::StartNew()
                while ($run.Window -eq [IntPtr]::Zero) {
                    Assert-True ($deadline.Elapsed.TotalSeconds -lt 10) 'loom window did not appear within 10 s'
                    Assert-True (-not $run.Process.HasExited) "loom exited early with code $($run.Process.ExitCode)"
                    $run.Window = [SmokeWin32]::FindWindow([uint32]$run.Process.Id, 'loom', $true)
                }
                if ($viaEsc) { Send-Key $run 'Esc' $true } else { Send-Close $run }
                $r = Wait-Loom $run
                $how = if ($viaEsc) { 'Esc' } else { 'WM_CLOSE' }
                $results += "$how=$($r.Code)"
                Assert-ExitOk $r
            } finally {
                Stop-Loom $run
            }
        }
        $script:detail = $results -join ', '
    }

    Test-Check 'W10' 'holding a look key records its axis and replays headless to identical state' {
        $run = Start-LoomWindow 'w10' @('--record', "$out/w10.ron", '--dump-state', "$out/w10.json")
        try {
            Start-Sleep -Milliseconds 700
            Send-Key $run 'L' $true; Start-Sleep -Milliseconds 500
            Send-Key $run 'L' $false; Start-Sleep -Milliseconds 300
            Send-Key $run 'Esc' $true
            Assert-ExitOk (Wait-Loom $run)
        } finally {
            Stop-Loom $run
        }
        $look = @(Get-AxisEvents "$out/w10.ron" 'look_x')
        $script:detail = 'look_x: ' + (($look | ForEach-Object { "tick $($_.Tick) = $($_.Value)" }) -join ', ')
        Assert-True ($look.Count -eq 2) "expected 2 look_x events (0.04, then 0.0), found $($look.Count)"
        Assert-True ($look[0].Value -eq 0.04 -and $look[1].Value -eq 0.0) 'look_x is not 0.04 then 0.0'
        Assert-True ($look[1].Tick -gt $look[0].Tick) 'look_x was set and reset in the same tick'
        $state = Get-Content "$out/w10.json" -Raw | ConvertFrom-Json
        Assert-True ($state.game.camera.yaw -lt 0) "camera yaw is $($state.game.camera.yaw), expected < 0 after turning right"
        $tick = $state.tick
        $replay = Invoke-HeadlessReplay "$out/w10.ron" $tick "$out/w10_replay.json"
        Assert-SameBytes "$out/w10.json" $replay
    }

    Test-Check 'W11' 'window trace equals the trace of its recording replayed headless' {
        $run = Start-LoomWindow 'w11' @('--record', "$out/w11.ron", '--trace', "$out/w11.jsonl", '--dump-state', "$out/w11.json")
        try {
            Start-Sleep -Milliseconds 700
            Send-Key $run 'Q' $true; Start-Sleep -Milliseconds 100
            Send-Key $run 'Q' $false; Start-Sleep -Milliseconds 300
            Send-Key $run 'W' $true; Start-Sleep -Milliseconds 500
            Send-Key $run 'Space' $true; Start-Sleep -Milliseconds 100
            Send-Key $run 'Space' $false; Start-Sleep -Milliseconds 800
            Send-Key $run 'W' $false; Start-Sleep -Milliseconds 300
            Send-Key $run 'Esc' $true
            $r = Wait-Loom $run
            Assert-ExitOk $r
        } finally {
            Stop-Loom $run
        }
        $tick = (Get-Content "$out/w11.json" -Raw | ConvertFrom-Json).tick
        Assert-True ($r.Json.trace.lines -eq $tick + 1) "trace has $($r.Json.trace.lines) lines for $tick ticks"
        $h = Invoke-Loom @('run', '--headless', '--frames', "$tick", '--input', "$out/w11.ron", '--trace', "$out/w11_replay.jsonl")
        Assert-ExitOk $h
        Assert-SameBytes "$out/w11.jsonl" "$out/w11_replay.jsonl"
        $types = @(Get-Content "$out/w11.jsonl" | ForEach-Object { ($_ | ConvertFrom-Json).events } | ForEach-Object { $_.type })
        $script:detail = "$tick ticks, events: $($types -join ' ')"
        Assert-True ($types -contains 'lock') 'no lock event in the trace'
        Assert-True ($types -contains 'action_start') 'no action_start event in the trace'
    }
}

Write-Host ('elapsed {0:N1} s' -f $clock.Elapsed.TotalSeconds)
Write-Host "smoke: $script:passed passed, $script:failed failed"
if ($script:failed -gt 0) { exit 1 }
