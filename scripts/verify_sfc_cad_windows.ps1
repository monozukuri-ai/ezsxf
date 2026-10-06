param(
    [ValidateSet('10.03.6', '8.25a')][string]$Version = '10.03.6',
    [string]$InputDirectory = 'platform-verification',
    [string]$Output = 'cad-verification'
)
$ErrorActionPreference = 'Stop'
if (Test-Path $Output) { throw 'The result directory must be new.' }
$root = [IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory $root | Out-Null
$runtime = Join-Path $env:RUNNER_TEMP ('ezsxf-cad-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $runtime | Out-Null
$pins = @{
    '10.03.6' = @('jww10036.exe', '64c629ab8eabfd0d2a54228c5bdb2c0ff0ed91ce77de30509fb1de109684d2a6')
    '8.25a' = @('jww825a.exe', '47632125be65d95c0a3722521dfa892cab2ecadd043194494e2e6feb12c87d78')
}
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Collections.Generic;
using System.Text;
using System.Runtime.InteropServices;
public class CadUI {
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr h, EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern IntPtr GetMenu(IntPtr h);
    [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetSubMenu(IntPtr h, int p);
    [DllImport("user32.dll")] public static extern uint GetMenuItemID(IntPtr h, int p);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetMenuString(IntPtr h, uint p, StringBuilder s, int n, uint flags);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, string l);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, StringBuilder l);
    [DllImport("user32.dll")] public static extern int GetDlgCtrlID(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out Rect r);
    public struct Rect { public int Left, Top, Right, Bottom; }
    public static string Text(IntPtr h) { var b=new StringBuilder(2048); if(Class(h)=="Edit") SendMessage(h,0xD,(IntPtr)b.Capacity,b); else GetWindowText(h,b,b.Capacity); return b.ToString(); }
    public static string Class(IntPtr h) { var b=new StringBuilder(256); GetClassName(h,b,b.Capacity); return b.ToString(); }
    public static IntPtr[] Windows(int pid) {
        var result=new List<IntPtr>();
        EnumWindows((h,l)=>{ uint p; GetWindowThreadProcessId(h,out p); if(p==pid)result.Add(h); return true; },IntPtr.Zero);
        return result.ToArray();
    }
    public static IntPtr[] Children(IntPtr parent) {
        var result=new List<IntPtr>(); EnumChildWindows(parent,(h,l)=>{result.Add(h);return true;},IntPtr.Zero); return result.ToArray();
    }
}
'@
function Read-Menu([IntPtr]$menu, [string]$parent) {
    for ($i=0; $i -lt [CadUI]::GetMenuItemCount($menu); $i++) {
        $text = [Text.StringBuilder]::new(512)
        [void][CadUI]::GetMenuString($menu, $i, $text, 512, 0x400)
        $path = $parent + '/' + $text.ToString()
        $sub = [CadUI]::GetSubMenu($menu, $i)
        if ($sub -ne [IntPtr]::Zero) { Read-Menu $sub $path }
        else { [pscustomobject]@{text=$path; id=[CadUI]::GetMenuItemID($menu,$i)} }
    }
}
function Read-Windows($process) {
    foreach ($handle in [CadUI]::Windows($process.Id)) {
        if (-not [CadUI]::IsWindowVisible($handle)) { continue }
        $children = foreach ($child in [CadUI]::Children($handle)) {
            [pscustomobject]@{handle=$child.ToInt64(); class=[CadUI]::Class($child); text=[CadUI]::Text($child); id=[CadUI]::GetDlgCtrlID($child); visible=[CadUI]::IsWindowVisible($child)}
        }
        [pscustomobject]@{handle=$handle.ToInt64(); class=[CadUI]::Class($handle); text=[CadUI]::Text($handle); children=@($children); menus=@(Read-Menu ([CadUI]::GetMenu($handle)) '')}
    }
}
function Snapshot($process, [string]$name) {
    $rows = @(Read-Windows $process)
    $rows | ConvertTo-Json -Depth 12 | Set-Content -Encoding utf8 (Join-Path $root ($name + '.json'))
    foreach ($row in $rows) {
        $handle = [IntPtr]$row.handle
        $rect = [CadUI+Rect]::new()
        [void][CadUI]::GetWindowRect($handle, [ref]$rect)
        $w=$rect.Right-$rect.Left; $h=$rect.Bottom-$rect.Top
        if ($w -le 0 -or $h -le 0 -or $w -gt 4096 -or $h -gt 4096) { continue }
        $bitmap = [Drawing.Bitmap]::new($w,$h)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        $dc = $graphics.GetHdc()
        try { $painted=[CadUI]::PrintWindow($handle,$dc,0) } finally { $graphics.ReleaseHdc($dc) }
        $bitmap.Save((Join-Path $root ($name + '-' + $row.handle + '.png')))
        $graphics.Dispose(); $bitmap.Dispose()
        Write-Host ('snapshot ' + $name + ': ' + $row.text + '; PrintWindow=' + $painted)
    }
    return $rows
}
function Await($condition, [int]$seconds=30) {
    $deadline=[DateTime]::UtcNow.AddSeconds($seconds)
    do {
        $value=& $condition
        if ($value) { return $value }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'The expected CAD state was not observed before timeout.'
}
function Main-Window {
    return @(Read-Windows $script:process) | Where-Object { $_.menus.Count -gt 0 } | Select-Object -First 1
}
function Close-Cad {
    $main=Main-Window
    [void][CadUI]::PostMessage([IntPtr]$main.handle,0x10,[IntPtr]::Zero,[IntPtr]::Zero)
    if (-not $script:process.WaitForExit(15000)) { throw 'CAD did not exit normally.' }
    if ($script:process.ExitCode -ne 0) { throw 'CAD exited with an error.' }
    $script:process=$null
}
function Open-Cad([string]$path, [string]$name) {
    $script:process=Start-Process $script:exe -ArgumentList ('"'+$path+'"') -PassThru
    [void](Await { Main-Window })
    Start-Sleep -Seconds 2
    [void](Snapshot $script:process $name)
}
function Save-Cad([string]$stem, [string]$extension, [string]$previous='') {
    $main=Main-Window
    $pattern=if ($extension -eq 'sfc') {'SFC.*保存'} else {'名前を付けて保存'}
    $command=$main.menus | Where-Object { $_.text -match $pattern } | Select-Object -First 1
    if ($null -eq $command) { throw 'The CAD save menu was not found.' }
    [void][CadUI]::PostMessage([IntPtr]$main.handle,0x111,[IntPtr]$command.id,[IntPtr]::Zero)
    $selector=Await { @(Read-Windows $script:process) | Where-Object { @($_.children | Where-Object id -eq 2408).Count -eq 1 } | Select-Object -First 1 }
    [void](Snapshot $script:process ($stem+'-selector'))
    $button=$selector.children | Where-Object id -eq 2408
    [void][CadUI]::PostMessage([IntPtr]$button.handle,0xF5,[IntPtr]::Zero,[IntPtr]::Zero)
    $dialog=Await { @(Read-Windows $script:process) | Where-Object { $_.handle -ne $selector.handle -and $_.class -eq '#32770' -and @($_.children | Where-Object { $_.class -eq 'Edit' -and $_.visible }).Count -eq 1 } | Select-Object -First 1 }
    $edit=$dialog.children | Where-Object { $_.class -eq 'Edit' -and $_.visible }
    [void][CadUI]::SendMessage([IntPtr]$edit.handle,0xC,[IntPtr]::Zero,$stem)
    [void](Snapshot $script:process ($stem+'-filename'))
    $mtime=if ($previous) { (Get-Item $previous).LastWriteTimeUtc.Ticks } else { 0 }
    [void][CadUI]::PostMessage([IntPtr]$dialog.handle,0x111,[IntPtr]1,[IntPtr]::Zero)
    if ($previous) {
        $confirm=Await { @(Read-Windows $script:process) | Where-Object { $_.text -eq 'jw_win' -and $_.class -eq '#32770' } | Select-Object -First 1 }
        [void](Snapshot $script:process ($stem+'-confirmation'))
        if (@($confirm.children | Where-Object { $_.text -match '30002|SFIG_LOCATE' }).Count) { throw 'CAD reported 30002: SFIG_LOCATE.' }
        $yes=$confirm.children | Where-Object { $_.class -eq 'Button' -and $_.id -eq 6 } | Select-Object -First 1
        if ($null -eq $yes) { $yes=$confirm.children | Where-Object { $_.class -eq 'Button' -and $_.id -eq 1 } | Select-Object -First 1 }
        if ($null -eq $yes) { throw 'The overwrite confirmation button was not found.' }
        [void][CadUI]::PostMessage([IntPtr]$yes.handle,0xF5,[IntPtr]::Zero,[IntPtr]::Zero)
    }
    $path=Await {
        $rows=@(Read-Windows $script:process)
        $errors=@($rows | Where-Object { $_.class -eq '#32770' -and @($_.children | Where-Object { $_.text -match '30002|SFIG_LOCATE' }).Count })
        if ($errors.Count) { throw 'CAD reported 30002: SFIG_LOCATE.' }
        if (@($rows | Where-Object { $_.class -eq '#32770' }).Count) { return $null }
        $paths=@($script:app, $script:inputFolder, (Get-Location).Path, $script:root) | Select-Object -Unique
        foreach ($folder in $paths) {
            $candidate=Join-Path $folder ($stem+'.'+$extension)
            if ((Test-Path $candidate) -and (Get-Item $candidate).Length -gt 0 -and (Get-Item $candidate).LastWriteTimeUtc.Ticks -ne $mtime) { return $candidate }
        }
    }
    $before=(Get-FileHash $path -Algorithm SHA256).Hash
    Start-Sleep -Seconds 1
    if ($before -ne (Get-FileHash $path -Algorithm SHA256).Hash) { throw 'The saved file is still changing.' }
    [void](Snapshot $script:process ($stem+'-saved'))
    return $path
}
$report = @{version=$Version; platform=[Environment]::OSVersion.VersionString; native_windows=$true; mode='save, overwrite and reopen'; cases=@(); complete=$false}
$process = $null
try {
    $pin = $pins[$Version]
    $installer = Join-Path $runtime $pin[0]
    & curl.exe --fail --location --retry 3 --retry-all-errors --connect-timeout 20 --max-time 120 --output $installer ('https://www.jwcad.net/download/' + $pin[0])
    if ($LASTEXITCODE -ne 0) { throw 'Official installer download failed.' }
    $hash=(Get-FileHash $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $pin[1]) { throw 'The official installer hash changed.' }
    $app=Join-Path $runtime 'app'
    $setup=Start-Process $installer -ArgumentList @('/VERYSILENT','/SP-','/SUPPRESSMSGBOXES','/NORESTART','/NOICONS',('/DIR="'+$app+'"')) -PassThru -Wait
    if ($setup.ExitCode -ne 0) { throw ('Installer failed: '+$setup.ExitCode) }
    $exe=Join-Path $app 'Jw_win.exe'
    $report.installer_sha256=$hash
    $report.application_sha256=(Get-FileHash $exe -Algorithm SHA256).Hash.ToLowerInvariant()
    $report.sxf_library_sha256=(Get-FileHash (Join-Path $app 'common_lib.dll') -Algorithm SHA256).Hash.ToLowerInvariant()
    foreach ($case in @(@{id='basic';path=(Join-Path $InputDirectory 'created.sfc')}, @{id='compound';path='tests/fixtures/writer_all_features.sfc'}, @{id='attributes';path=(Join-Path $InputDirectory 'cad-attributes/attributes.sfc')})) {
        $input=[IO.Path]::GetFullPath($case.path)
        $inputFolder=Split-Path $input
        $before=(Get-FileHash $input -Algorithm SHA256).Hash
        $evidence=Join-Path $root $case.id
        New-Item -ItemType Directory $evidence | Out-Null
        $result=@{id=$case.id;input_sha256=$before;cad_save_completed=$false}
        try {
            Open-Cad $input ($case.id+'-input')
            $baseline=Save-Cad ($case.id+'_baseline') 'jww'
            Copy-Item $baseline (Join-Path $evidence 'baseline.jww')
            Close-Cad
            Open-Cad $input ($case.id+'-source-reopen')
            $saved=Save-Cad ($case.id+'_native') 'sfc'
            Copy-Item $saved (Join-Path $evidence 'first-save.sfc')
            Close-Cad
            Open-Cad $saved ($case.id+'-first-reopen')
            [void](Save-Cad ($case.id+'_native') 'sfc' $saved)
            Copy-Item $saved (Join-Path $evidence 'overwrite.sfc')
            Close-Cad
            Open-Cad $saved ($case.id+'-overwrite-reopen')
            $reopened=Save-Cad ($case.id+'_reopened') 'jww'
            Copy-Item $reopened (Join-Path $evidence 'reopened.jww')
            Close-Cad
            $result.cad_save_completed=$true
        } catch {
            $result.error=$_.Exception.Message
            if ($null -ne $process -and -not $process.HasExited) { [void](Snapshot $process ($case.id+'-failure')) }
            $partial=Join-Path $app ($case.id+'_native.sfc')
            if (Test-Path $partial) { Copy-Item $partial (Join-Path $evidence 'failed-output.sfc') }
        } finally {
            if ($null -ne $process -and -not $process.HasExited) { Stop-Process -Id $process.Id -Force }
            $process=$null
            $result.source_unchanged=((Get-FileHash $input -Algorithm SHA256).Hash -eq $before)
            $result.files_sha256=@{}
            foreach ($file in Get-ChildItem $evidence -File) { $result.files_sha256[$file.Name]=(Get-FileHash $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
            $report.cases+=$result
        }
    }
    $report.complete=$true
} finally {
    if ($null -ne $process -and -not $process.HasExited) { Stop-Process -Id $process.Id -Force }
    $report | ConvertTo-Json -Depth 12 | Set-Content -Encoding utf8 (Join-Path $root 'verification.json')
}
