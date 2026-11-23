@echo off
setlocal DisableDelayedExpansion
set "PVPBOT_BATCH_FILE=%~f0"
"%~dp0..\..\run-batch.bat" %*
exit /b %errorlevel%

# PowerShell
param([string]$OutputDirectory = 'build/benchmarks/ping-sweep')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$repo = [IO.Path]::GetFullPath((Join-Path $scriptDirectory '../../..'))
$out = Join-Path $repo $OutputDirectory
$data = Get-Content -LiteralPath (Join-Path $out 'results.json') -Raw | ConvertFrom-Json
$bitmap = [Drawing.Bitmap]::new(1180, 700)
$g = [Drawing.Graphics]::FromImage($bitmap)
$g.Clear([Drawing.Color]::White)
$g.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.TextRenderingHint = [Drawing.Text.TextRenderingHint]::AntiAliasGridFit
$font = [Drawing.Font]::new('Segoe UI', 10)
$heading = [Drawing.Font]::new('Segoe UI', 18, [Drawing.FontStyle]::Bold)
$title = [Drawing.Font]::new('Segoe UI', 22, [Drawing.FontStyle]::Bold)
$ink = [Drawing.Brushes]::Black
$center = [Drawing.StringFormat]::new()
$center.Alignment = [Drawing.StringAlignment]::Center
$center.LineAlignment = [Drawing.StringAlignment]::Center
$g.DrawString('Ping and win rate: identical bots', $title, $ink, 30, 18)
$g.DrawString('RTT = upstream + downstream. Both directions RTT/2. Cells: row player win %.', $font, $ink, 32, 65)
for ($panel = 0; $panel -lt 2; $panel++) {
    $style = @('critical-plus', 'duel-rl')[$panel]
    $label = @('Critical+ vs Critical+', 'Duel-RL vs Duel-RL')[$panel]
    $originX = 88 + $panel * 570
    $originY = 180
    $g.DrawString($label, $heading, $ink, $originX, 101)
    $g.DrawString('Opponent RTT (ms)', $font, $ink, $originX + 145, 135)
    $g.DrawString('Own', $font, $ink, $originX - 62, 170)
    $g.DrawString('RTT', $font, $ink, $originX - 62, 188)
    $g.DrawString('(ms)', $font, $ink, $originX - 62, 206)
    for ($row = 0; $row -lt 15; $row++) {
        $a = ($row + 1) * 10
        $g.DrawString([string]$a, $font, $ink, [Drawing.RectangleF]::new($originX - 32, $originY + $row * 30, 30, 30), $center)
        $g.DrawString([string]$a, $font, $ink, [Drawing.RectangleF]::new($originX + $row * 30, $originY - 27, 30, 27), $center)
        for ($col = 0; $col -lt 15; $col++) {
            $b = ($col + 1) * 10
            $lo = [Math]::Min($a, $b)
            $hi = [Math]::Max($a, $b)
            $group = $data | Where-Object { $_.condition.style -eq $style -and $_.condition.family -eq 'rtt-grid' -and $_.condition.up_ms[0] * 2 -eq $lo -and $_.condition.up_ms[1] * 2 -eq $hi } | Select-Object -First 1
            $wins = if ($a -le $b) { $group.wins } else { $group.losses }
            $percent = 100 * $wins / $group.episodes.Count
            $strength = [Math]::Abs($percent - 50) / 50
            $rgb = if ($percent -ge 50) { @(35, 110, 180) } else { @(210, 65, 60) }
            $channels = $rgb | ForEach-Object { [int](250 + ($_ - 250) * $strength) }
            $brush = [Drawing.SolidBrush]::new([Drawing.Color]::FromArgb($channels[0], $channels[1], $channels[2]))
            $rect = [Drawing.RectangleF]::new($originX + $col * 30, $originY + $row * 30, 29, 29)
            $g.FillRectangle($brush, $rect)
            $textBrush = if ($strength -gt 0.65) { [Drawing.Brushes]::White } else { $ink }
            $g.DrawString($percent.ToString('F0'), $font, $textBrush, $rect, $center)
            $brush.Dispose()
        }
    }
}
$g.DrawString('Blue: row player wins more often. Red: row player wins less often. Values rounded to whole percentages.', $font, $ink, 32, 637)
$g.DrawString('256 matches/cell: 64 shared contexts x 2 seats x 2 input orders. 60 FPS. Rust simulation; Minecraft equivalence unverified.', $font, $ink, 32, 662)
$bitmap.Save((Join-Path $out 'ping-heatmap.png'), [Drawing.Imaging.ImageFormat]::Png)
$g.Dispose()
$bitmap.Dispose()
$font.Dispose()
$heading.Dispose()
$title.Dispose()
$center.Dispose()
Write-Output (Join-Path $out 'ping-heatmap.png')
