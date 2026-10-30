param([string]$PdfTestOutput = '')
# Internal LiteOffice printing script. All document content is data, never PowerShell code.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
if ($env:LITEOFFICE_PRINT_STDIN -eq '1') {
    $raw = @([regex]::Split([Console]::In.ReadToEnd(), "\r\n|\n|\r") | Where-Object { $_.Length -gt 0 })
} else {
    $manifest = $env:LITEOFFICE_PRINT_INPUT
    if (-not $manifest -or -not [IO.File]::Exists($manifest)) { throw 'Missing LiteOffice print input' }
    $raw = [IO.File]::ReadAllLines($manifest, [Text.Encoding]::UTF8)
}
$script:blocks = New-Object 'System.Collections.Generic.List[object]'
$landscape = $false
$marginMm = 16.5
$imageScale = "percent"
$showPageNumbers = $false
$seenSettings = $false
$previousWasBreak = $true
foreach ($line in $raw) {
    $fields = $line.Split('|')
    if ($fields[0] -eq 'P' -and $fields.Length -eq 1) {
        if ($previousWasBreak) { throw 'Page break must separate non-empty planned pages' }
        $script:blocks.Add([pscustomobject]@{Type='P'})
        $previousWasBreak = $true
        continue
    }
    if ($fields[0] -eq 'S' -and $fields.Length -eq 5) {
        if ($seenSettings) { throw 'Duplicate print settings record' }
        $seenSettings = $true
        $landscape = ($fields[1] -eq 'L')
        if ($fields[1] -notin @('P','L')) { throw 'Invalid page orientation' }
        $marginMm = [double]::Parse($fields[2], [Globalization.CultureInfo]::InvariantCulture)
        if ([double]::IsNaN($marginMm) -or $marginMm -lt 5 -or $marginMm -gt 40) { throw 'Invalid page margins' }
        $imageScale = $fields[3]
        if ($imageScale -notin @('original','fit','percent')) { throw 'Invalid print image scale mode' }
        if ($fields[4] -notin @('0','1')) { throw 'Invalid page number setting' }
        $showPageNumbers = ($fields[4] -eq '1')
    } elseif ($fields[0] -eq 'T' -and $fields.Length -eq 2) {
        $value = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($fields[1]))
        foreach ($paragraph in [regex]::Split($value.Replace("`t", '    '), "\r\n|\n|\r")) {
            $script:blocks.Add([pscustomobject]@{Type='T'; Text=$paragraph})
        }
        $previousWasBreak = $false
    } elseif ($fields[0] -eq 'R' -and $fields.Length -ge 3) {
        $count = [int]::Parse($fields[1], [Globalization.CultureInfo]::InvariantCulture)
        if ($count -lt 1 -or $count -gt 128 -or $fields.Length -ne ($count + 2)) { throw 'Invalid table row' }
        $cells = New-Object 'System.Collections.Generic.List[string]'
        for ($i = 0; $i -lt $count; $i++) {
            $cells.Add([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($fields[$i + 2])))
        }
        $script:blocks.Add([pscustomobject]@{Type='R'; Cells=$cells})
        $previousWasBreak = $false
    } elseif ($fields[0] -eq 'I' -and $fields.Length -eq 3) {
        $pct = [double]::Parse($fields[1], [Globalization.CultureInfo]::InvariantCulture)
        $bytes = [Convert]::FromBase64String($fields[2])
        if ($bytes.Length -gt 12582912) { throw 'Oversized image in print manifest' }
        $script:blocks.Add([pscustomobject]@{Type='I'; Bytes=$bytes; Width=[math]::Max(15,[math]::Min(100,$pct))})
        $previousWasBreak = $false
    } elseif ($fields[0] -eq 'D' -and $fields.Length -eq 4) {
        $w = [double]::Parse($fields[1], [Globalization.CultureInfo]::InvariantCulture)
        $h = [double]::Parse($fields[2], [Globalization.CultureInfo]::InvariantCulture)
        if ([double]::IsNaN($w) -or [double]::IsNaN($h) -or $w -le 0 -or $h -le 0 -or $w -gt 842 -or $h -gt 842) { throw 'Invalid sized image geometry' }
        $bytes = [Convert]::FromBase64String($fields[3])
        if ($bytes.Length -eq 0 -or $bytes.Length -gt 12582912) { throw 'Invalid sized image data' }
        $script:blocks.Add([pscustomobject]@{Type='D'; Bytes=$bytes; WidthPt=$w; HeightPt=$h})
        $previousWasBreak = $false
    } else {
        throw 'Invalid LiteOffice print manifest record'
    }
}
if (-not $seenSettings) { throw 'Missing print settings record' }
if ($script:blocks.Count -eq 0 -or $previousWasBreak) { throw 'Empty or unterminated print job' }
$script:font = [Drawing.Font]::new('Segoe UI', [single]10, [Drawing.FontStyle]::Regular)
$script:index = 0
$script:pageNumber = 0
$script:pending = New-Object 'System.Collections.Generic.Queue[string]'
$document = New-Object Drawing.Printing.PrintDocument
if ($PdfTestOutput) {
    if (-not [IO.Path]::IsPathRooted($PdfTestOutput) -or [IO.Path]::GetExtension($PdfTestOutput) -ne '.pdf' -or [IO.File]::Exists($PdfTestOutput)) {
        throw 'PDF regression output must be a new absolute .pdf path'
    }
    $document.PrinterSettings.PrinterName = 'Microsoft Print to PDF'
    if (-not $document.PrinterSettings.IsValid) { throw 'Microsoft Print to PDF is unavailable for this regression test' }
    $document.PrinterSettings.PrintToFile = $true
    $document.PrinterSettings.PrintFileName = $PdfTestOutput
    $document.PrintController = New-Object Drawing.Printing.StandardPrintController
}
$document.DocumentName = 'LiteOffice document (approximate layout)'
$document.DefaultPageSettings.PaperSize = [Drawing.Printing.PaperSize]::new('A4',827,1169)
$document.DefaultPageSettings.Landscape = $landscape
# Printer units are hundredths of an inch, not millimetres.
$marginPrinter = [int][math]::Round($marginMm * 100.0 / 25.4)
$document.DefaultPageSettings.Margins = [Drawing.Printing.Margins]::new($marginPrinter,$marginPrinter,$marginPrinter,$marginPrinter)
$onBegin = [Drawing.Printing.PrintEventHandler] {
    param($sender,$e)
    $script:index = 0
    $script:pageNumber = 0
    $script:pending.Clear()
}
$document.add_BeginPrint($onBegin)
$onPage = [Drawing.Printing.PrintPageEventHandler] {
    param($sender,$e)
    $g = $e.Graphics
    # Some printer drivers retain their own MarginBounds after the print dialog.
    # Use the manifest margin against the physical page so PDF output matches the UI.
    $marginPrinter = [int][math]::Round($marginMm * 100.0 / 25.4)
    $bounds = [Drawing.Rectangle]::new($marginPrinter, $marginPrinter,
        [math]::Max(1, $e.PageBounds.Width - 2 * $marginPrinter),
        [math]::Max(1, $e.PageBounds.Height - 2 * $marginPrinter))
    $lineHeight = [math]::Max(1,[math]::Ceiling($script:font.GetHeight($g) * 1.24))
    $y = [single]$bounds.Top
    # Reserve a footer line before laying out content; the footer never overlaps a picture.
    $footerSpace = if ($showPageNumbers) { [single]30 } else { [single]0 }
    $maxY = [single]($bounds.Bottom - $footerSpace)
    $script:pageNumber++
    $width = [single]$bounds.Width
    $pageHasContent = $false
    while ($true) {
        if ($script:pending.Count -gt 0) {
            if ($y + $lineHeight -gt $maxY) { break }
            $part = $script:pending.Dequeue()
            $g.DrawString($part, $script:font, [Drawing.Brushes]::Black, [single]$bounds.Left, $y)
            $y += $lineHeight
            $pageHasContent = $true
            continue
        }
        if ($script:index -ge $script:blocks.Count) { break }
        $item = $script:blocks[$script:index]
        if ($item.Type -eq 'P') {
            $script:index++
            if ($pageHasContent) { break }
            continue
        }
        if ($item.Type -eq 'T') {
            $script:index++
            $remaining = [string]$item.Text
            if ($remaining.Length -eq 0) { $script:pending.Enqueue(''); continue }
            while ($remaining.Length -gt 0) {
                $lo=1; $hi=$remaining.Length; $fit=1
                while ($lo -le $hi) {
                    $mid = [int](($lo+$hi)/2)
                    if ($g.MeasureString($remaining.Substring(0,$mid),$script:font).Width -le $width) {
                        $fit=$mid; $lo=$mid+1
                    } else { $hi=$mid-1 }
                }
                if ($fit -lt $remaining.Length) {
                    $space = $remaining.LastIndexOf(' ', $fit-1, $fit)
                    if ($space -gt 0) { $fit=$space+1 }
                }
                $script:pending.Enqueue($remaining.Substring(0,$fit).TrimEnd())
                $remaining = $remaining.Substring($fit)
            }
            continue
        }
        if ($item.Type -eq 'R') {
            $cellCount = $item.Cells.Count
            $cellWidth = [single]($width / $cellCount)
            $rowHeight = [single]($lineHeight + 10)
            foreach ($cell in $item.Cells) {
                $measured = $g.MeasureString([string]$cell, $script:font, [int][math]::Max(1, $cellWidth - 8))
                $rowHeight = [single][math]::Max($rowHeight, $measured.Height + 10)
            }
            if ($y + $rowHeight -gt $maxY -and $y -gt $bounds.Top) { break }
            $pen = [Drawing.Pen]::new([Drawing.Color]::Black, [single]1)
            try {
                for ($cellIndex = 0; $cellIndex -lt $cellCount; $cellIndex++) {
                    $rect = [Drawing.RectangleF]::new([single]($bounds.Left + $cellIndex * $cellWidth), $y, $cellWidth, $rowHeight)
                    $g.DrawRectangle($pen, $rect.X, $rect.Y, $rect.Width, $rect.Height)
                    $g.DrawString([string]$item.Cells[$cellIndex], $script:font, [Drawing.Brushes]::Black,
                        [Drawing.RectangleF]::new($rect.X + 4, $rect.Y + 4, $rect.Width - 8, $rect.Height - 8))
                }
            } finally { $pen.Dispose() }
            $y += $rowHeight + 4
            $pageHasContent = $true
            $script:index++
            continue
        }
        if ($item.Type -in @('I', 'D')) {
            $stream = [IO.MemoryStream]::new([byte[]]$item.Bytes, $false)
            try {
                $pic = [Drawing.Image]::FromStream($stream)
                try {
                    # Graphics are measured in hundredths of an inch for PrintDocument.
                    # 'Original size' uses embedded DPI; 'fit' fills the printable width;
                    # 'percent' reproduces the user's Width% value.
                    $dpiX = [single]$pic.HorizontalResolution
                    if ($dpiX -lt 1 -or [single]::IsNaN($dpiX) -or [single]::IsInfinity($dpiX)) { $dpiX = 96 }
                    $nativeW = [single]($pic.Width * 100.0 / $dpiX)
                    if ($item.Type -eq 'D') {
                        $w = [single]($item.WidthPt * 100.0 / 72.0)
                    } elseif ($imageScale -eq 'original') {
                        $w = [single][math]::Min($width, $nativeW)
                    } elseif ($imageScale -eq 'fit') {
                        $w = [single]$width
                    } else {
                        $w = [single]($width * $item.Width / 100.0)
                    }
                    $w = [single][math]::Max(1.0, [math]::Min($width, $w))
                    $h = if ($item.Type -eq 'D') {
                        [single]($w * $item.HeightPt / $item.WidthPt)
                    } else { [single]($w * $pic.Height / $pic.Width) }
                    $availableHeight = [single]($maxY - $bounds.Top - 12)
                    if ($h -gt $availableHeight) { $w *= $availableHeight / $h; $h = $availableHeight }
                    if ($y + $h -gt $maxY -and $y -gt $bounds.Top) { break }
                    $g.DrawImage($pic, [single]$bounds.Left, $y, $w, $h)
                    $y += $h + 12
                    $pageHasContent = $true
                } finally { $pic.Dispose() }
            } finally { $stream.Dispose() }
            $script:index++
            continue
        }
        $script:index++
    }
    if ($showPageNumbers) {
        $footerText = "Page $($script:pageNumber)"
        $footerFont = [Drawing.Font]::new('Segoe UI', [single]8, [Drawing.FontStyle]::Regular)
        try {
            $footerBounds = [Drawing.RectangleF]::new([single]$bounds.Left, [single]($bounds.Bottom - 15), [single]$width, [single]20)
            $alignment = New-Object Drawing.StringFormat
            try {
                $alignment.Alignment = [Drawing.StringAlignment]::Far
                $g.DrawString($footerText, $footerFont, [Drawing.Brushes]::DimGray, $footerBounds, $alignment)
            } finally { $alignment.Dispose() }
        } finally { $footerFont.Dispose() }
    }
    $e.HasMorePages = ($script:index -lt $script:blocks.Count -or $script:pending.Count -gt 0)
}
$document.add_PrintPage($onPage)
if ($PdfTestOutput) {
    try {
        $document.Print()
        Write-Output 'LITEOFFICE_PRINT_RESULT:printed'
    } finally {
        $document.Dispose()
        $script:font.Dispose()
    }
    return
}
$preview = New-Object System.Windows.Forms.PrintPreviewDialog
$preview.Document = $document
$preview.Width = 1100
$preview.Height = 850
$dialog = New-Object System.Windows.Forms.PrintDialog
$dialog.UseEXDialog = $true
$dialog.Document = $document
try {
    # Closing preview without printing is valid. The system dialog then selects a printer.
    [void]$preview.ShowDialog()
    if ($dialog.ShowDialog() -eq [Windows.Forms.DialogResult]::OK) {
        $document.Print()
        Write-Output 'LITEOFFICE_PRINT_RESULT:printed'
    } else {
        Write-Output 'LITEOFFICE_PRINT_RESULT:cancelled'
    }
} finally {
    $preview.Dispose()
    $dialog.Dispose()
    $document.Dispose()
    $script:font.Dispose()
}
