<#
.SYNOPSIS
    Renders public/icon.png (512x512) from the Android launcher glyph.

.DESCRIPTION
    The vector is apps/android/app/src/main/res/drawable/ic_launcher_foreground.xml:
    a #0072DA rounded rectangle with a white heartbeat line, on a 108-unit
    canvas. It is redrawn here with System.Drawing rather than converted,
    because the Tizen packager wants a PNG and a one-off image tool would be
    a dependency for a file that changes once a year. Run it after editing
    the Android glyph; the PNG is committed.
#>
[CmdletBinding()]
param([string]$Out = (Join-Path $PSScriptRoot '..\public\icon.png'))
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$size = 512
# The glyph's rounded rectangle spans x 28..80, y 32..76 in the 108-unit
# viewport. Scale it to fill the icon with a small margin and centre it.
$scale = 9.0
$offX = ($size - 52 * $scale) / 2 - 28 * $scale
$offY = ($size - 44 * $scale) / 2 - 32 * $scale
function P([double]$x, [double]$y) { [System.Drawing.PointF]::new([float]($x * $scale + $offX), [float]($y * $scale + $offY)) }

$bmp = [System.Drawing.Bitmap]::new($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$g = [System.Drawing.Graphics]::FromImage($bmp)
try {
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)

    $r = 10 * $scale
    $x0 = 28 * $scale + $offX; $y0 = 32 * $scale + $offY
    $w = 52 * $scale; $h = 44 * $scale
    $path = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $path.AddArc($x0, $y0, 2 * $r, 2 * $r, 180, 90)
    $path.AddArc($x0 + $w - 2 * $r, $y0, 2 * $r, 2 * $r, 270, 90)
    $path.AddArc($x0 + $w - 2 * $r, $y0 + $h - 2 * $r, 2 * $r, 2 * $r, 0, 90)
    $path.AddArc($x0, $y0 + $h - 2 * $r, 2 * $r, 2 * $r, 90, 90)
    $path.CloseFigure()
    $blue = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml('#0072DA'))
    $g.FillPath($blue, $path)

    # M33,55 h9 l4,-9 l6,18 l5,-14 l3,5 h15
    $points = @((P 33 55), (P 42 55), (P 46 46), (P 52 64), (P 57 50), (P 60 55), (P 75 55))
    $pen = [System.Drawing.Pen]::new([System.Drawing.Color]::White, [float](4 * $scale))
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
    $g.DrawLines($pen, [System.Drawing.PointF[]]$points)

    $full = [System.IO.Path]::GetFullPath($Out)
    $bmp.Save($full, [System.Drawing.Imaging.ImageFormat]::Png)
    "wrote $full ($((Get-Item $full).Length) bytes)"
} finally {
    $g.Dispose(); $bmp.Dispose()
}
