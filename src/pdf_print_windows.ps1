param([string]$PdfTestOutput = '')
# Only our trusted code is executed. The PDF itself never enters this process.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$manifest = $env:LITEOFFICE_PRINT_INPUT
$lines = [IO.File]::ReadAllLines($manifest, [Text.Encoding]::UTF8)
$header = $lines[0].Split('|')
if ($header.Count -ne 2 -or $header[0] -ne 'PDF') { throw 'Invalid PDF print manifest' }
$scale = $header[1]
if ($scale -ne 'fit' -and ($scale -notmatch '^\d{2,3}$' -or [int]$scale -lt 10 -or [int]$scale -gt 400)) { throw 'Invalid PDF scale' }
$script:pages = New-Object 'System.Collections.Generic.List[object]'
$culture = [Globalization.CultureInfo]::InvariantCulture
foreach ($line in $lines | Select-Object -Skip 1) {
    $p = $line.Split('|')
    if ($p.Count -ne 4 -or $p[0] -ne 'P' -or $p[1] -notmatch '^page-[0-9]{4}\.png$') { throw 'Invalid PDF page record' }
    $w = [double]::Parse($p[2], $culture)
    $h = [double]::Parse($p[3], $culture)
    if ($w -le 0 -or $h -le 0 -or $w -gt 105000 -or $h -gt 105000) { throw 'Invalid PDF page dimensions' }
    $script:pages.Add([pscustomobject]@{Path=[IO.Path]::Combine([IO.Path]::GetDirectoryName($manifest),$p[1]); Width=$w; Height=$h})
}
if ($script:pages.Count -eq 0) { throw 'No PDF pages selected' }
$document = New-Object Drawing.Printing.PrintDocument
$document.DocumentName = 'LiteOffice PDF'
$document.OriginAtMargins = $false
$script:page = 0
$document.add_BeginPrint({ $script:page = 0 })
$document.add_QueryPageSettings({
    param($sender,$e)
    $item = $script:pages[$script:page]
    $e.PageSettings.Landscape = ($item.Width -gt $item.Height)
})
$document.add_PrintPage({
    param($sender,$e)
    $item = $script:pages[$script:page]
    $image = [Drawing.Image]::FromFile($item.Path)
    try {
        $area = $e.PageSettings.PrintableArea
        # Some drivers retain portrait PrintableArea after per-page landscape
        # selection. Normalize its coordinates before centering/scaling.
        if (($e.PageSettings.Landscape -and $area.Width -lt $area.Height) -or
            (-not $e.PageSettings.Landscape -and $area.Width -gt $area.Height)) {
            $area = [Drawing.RectangleF]::new($area.Y,$area.X,$area.Height,$area.Width)
        }
        $factor = if ($scale -eq 'fit') { [math]::Min($area.Width/$item.Width, $area.Height/$item.Height) } else { [double]$scale/100.0 }
        $w = [single]($item.Width*$factor)
        $h = [single]($item.Height*$factor)
        $x = [single]($area.X+($area.Width-$w)/2.0-$e.PageSettings.HardMarginX)
        $y = [single]($area.Y+($area.Height-$h)/2.0-$e.PageSettings.HardMarginY)
        $e.Graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $e.Graphics.DrawImage($image, [Drawing.RectangleF]::new($x,$y,$w,$h))
    } finally { $image.Dispose() }
    $script:page++
    $e.HasMorePages = ($script:page -lt $script:pages.Count)
})
$dialog = New-Object Windows.Forms.PrintDialog
$dialog.UseEXDialog = $true
$dialog.Document = $document
try {
    if ($PdfTestOutput) {
        $document.PrinterSettings.PrinterName = 'Microsoft Print to PDF'
        if (-not $document.PrinterSettings.IsValid) { throw 'Microsoft Print to PDF unavailable' }
        $document.PrinterSettings.PrintToFile = $true
        $document.PrinterSettings.PrintFileName = $PdfTestOutput
        $document.PrintController = New-Object Drawing.Printing.StandardPrintController
    } elseif ($dialog.ShowDialog() -ne [Windows.Forms.DialogResult]::OK) {
        Write-Output 'LITEOFFICE_PRINT_RESULT:cancelled'
        return
    }
    if (-not $document.PrinterSettings.IsValid) { throw 'Selected printer unavailable' }
    $document.Print()
    Write-Output 'LITEOFFICE_PRINT_RESULT:printed'
} finally {
    $dialog.Dispose()
    $document.Dispose()
}
