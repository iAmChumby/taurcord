$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent $PSScriptRoot
$iconsDir = Join-Path $root 'icons'
New-Item -ItemType Directory -Force -Path $iconsDir | Out-Null

function Add-RoundedRect([System.Drawing.Drawing2D.GraphicsPath]$p, [float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
    $d = 2 * $r
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
}

$S = 512
$master = New-Object System.Drawing.Bitmap($S, $S, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$g = [System.Drawing.Graphics]::FromImage($master)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias

$rect = New-Object System.Drawing.Rectangle(0, 0, $S, $S)
$grad = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    $rect,
    [System.Drawing.Color]::FromArgb(255, 0x58, 0x65, 0xF2),
    [System.Drawing.Color]::FromArgb(255, 0x3b, 0x44, 0xc6),
    90.0)

$bodyPath = New-Object System.Drawing.Drawing2D.GraphicsPath
Add-RoundedRect $bodyPath 0 0 $S $S ([int]($S * 0.22))
$g.FillPath($grad, $bodyPath)

$white = [System.Drawing.Brushes]::White

$bubblePath = New-Object System.Drawing.Drawing2D.GraphicsPath
Add-RoundedRect $bubblePath ($S * 0.18) ($S * 0.27) ($S * 0.64) ($S * 0.37) ($S * 0.115)
$g.FillPath($white, $bubblePath)

$tail = New-Object System.Drawing.PointF[] 3
$tail[0] = New-Object System.Drawing.PointF(($S * 0.235), ($S * 0.605))
$tail[1] = New-Object System.Drawing.PointF(($S * 0.385), ($S * 0.62))
$tail[2] = New-Object System.Drawing.PointF(($S * 0.20), ($S * 0.775))
$g.FillPolygon($white, $tail)

$dotBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 0x58, 0x65, 0xF2))
$dotR = $S * 0.042
$dotY = $S * 0.455
foreach ($fx in 0.34, 0.50, 0.66) {
    $cx = $S * $fx
    $g.FillEllipse($dotBrush, ($cx - $dotR), ($dotY - $dotR), (2 * $dotR), (2 * $dotR))
}

$g.Dispose()

$pngPath = Join-Path $iconsDir 'icon.png'
$master.Save($pngPath, [System.Drawing.Imaging.ImageFormat]::Png)

function Get-IcoImageBlob([System.Drawing.Bitmap]$src, [int]$size) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $gg = [System.Drawing.Graphics]::FromImage($bmp)
    $gg.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $gg.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $gg.DrawImage($src, 0, 0, $size, $size)
    $gg.Dispose()

    $rect = New-Object System.Drawing.Rectangle(0, 0, $size, $size)
    $data = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $stride = $data.Stride
        $bytes = New-Object byte[] ($stride * $size)
        [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $bytes, 0, $bytes.Length)
    } finally {
        $bmp.UnlockBits($data)
    }

    $xorLen = $size * 4 * $size
    $xor = New-Object byte[] $xorLen
    for ($y = 0; $y -lt $size; $y++) {
        $srcRow = (($size - 1 - $y) * $stride)
        $dstRow = $y * $size * 4
        [Array]::Copy($bytes, $srcRow, $xor, $dstRow, $size * 4)
    }

    $maskStride = [int][Math]::Ceiling($size / 32.0) * 4
    $mask = New-Object byte[] ($maskStride * $size)

    $header = New-Object byte[] 40
    $bw = [System.IO.BinaryWriter]::new([System.IO.MemoryStream]::new($header))
    $bw.Write([uint32]40)
    $bw.Write([int32]$size)
    $bw.Write([int32]($size * 2))
    $bw.Write([uint16]1)
    $bw.Write([uint16]32)
    $bw.Write([uint32]0)
    $bw.Write([uint32]($xorLen + $mask.Length))
    $bw.Write([int32]0)
    $bw.Write([int32]0)
    $bw.Write([uint32]0)
    $bw.Write([uint32]0)
    $bw.Flush()
    $bw.Dispose()

    $blob = New-Object byte[] ($header.Length + $xor.Length + $mask.Length)
    [Array]::Copy($header, 0, $blob, 0, $header.Length)
    [Array]::Copy($xor, 0, $blob, $header.Length, $xor.Length)
    [Array]::Copy($mask, 0, $blob, $header.Length + $xor.Length, $mask.Length)
    $bmp.Dispose()
    return ,$blob
}

$sizes = @(16, 24, 32, 48, 64, 128, 256)
$blobs = @{}
foreach ($sz in $sizes) { $blobs[$sz] = Get-IcoImageBlob $master $sz }

$icoStream = New-Object System.IO.MemoryStream
$iw = [System.IO.BinaryWriter]::new($icoStream)
$iw.Write([uint16]0)
$iw.Write([uint16]1)
$iw.Write([uint16]$sizes.Count)

$offset = 6 + (16 * $sizes.Count)
foreach ($sz in $sizes) {
    $b = if ($sz -ge 256) { 0 } else { $sz }
    $iw.Write([byte]$b)
    $iw.Write([byte]$b)
    $iw.Write([byte]0)
    $iw.Write([byte]0)
    $iw.Write([uint16]1)
    $iw.Write([uint16]32)
    $iw.Write([uint32]$blobs[$sz].Length)
    $iw.Write([uint32]$offset)
    $offset += $blobs[$sz].Length
}
foreach ($sz in $sizes) { $iw.Write($blobs[$sz]) }
$iw.Flush()

$icoPath = Join-Path $iconsDir 'icon.ico'
[System.IO.File]::WriteAllBytes($icoPath, $icoStream.ToArray())
$iw.Dispose()
$icoStream.Dispose()
$master.Dispose()

Write-Output "icon.png: $((Get-Item $pngPath).Length) bytes"
Write-Output "icon.ico: $((Get-Item $icoPath).Length) bytes"
