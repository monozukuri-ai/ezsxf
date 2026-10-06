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
    [DllImport("user32.dll")] public static extern int GetDlgCtrlID(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out Rect r);
    public struct Rect { public int Left, Top, Right, Bottom; }
    public static string Text(IntPtr h) { var b=new StringBuilder(2048); GetWindowText(h,b,b.Capacity); return b.ToString(); }
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
        Write-Output ('snapshot ' + $name + ': ' + $row.text + '; PrintWindow=' + $painted)
    }
    return $rows
}
$report = @{version=$Version; platform=[Environment]::OSVersion.VersionString; native_windows=$true; mode='UI probe'; cases=@(); complete=$false}
$process = $null
try {
    $pin = $pins[$Version]
    $installer = Join-Path $runtime $pin[0]
    Invoke-WebRequest ('https://www.jwcad.net/download/' + $pin[0]) -OutFile $installer
    $hash=(Get-FileHash $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $pin[1]) { throw 'The official installer hash changed.' }
    $app=Join-Path $runtime 'app'
    $setup=Start-Process $installer -ArgumentList @('/VERYSILENT','/SP-','/SUPPRESSMSGBOXES','/NORESTART','/NOICONS',('/DIR="'+$app+'"')) -PassThru -Wait
    if ($setup.ExitCode -ne 0) { throw ('Installer failed: '+$setup.ExitCode) }
    $exe=Join-Path $app 'Jw_win.exe'
    $report.installer_sha256=$hash
    $report.application_sha256=(Get-FileHash $exe -Algorithm SHA256).Hash.ToLowerInvariant()
    $report.sxf_library_sha256=(Get-FileHash (Join-Path $app 'common_lib.dll') -Algorithm SHA256).Hash.ToLowerInvariant()
    foreach ($case in @(@{id='basic';path=(Join-Path $InputDirectory 'created.sfc')}, @{id='compound';path='tests/fixtures/writer_all_features.sfc'})) {
        $input=[IO.Path]::GetFullPath($case.path)
        $before=(Get-FileHash $input -Algorithm SHA256).Hash
        $process=Start-Process $exe -ArgumentList ('"'+$input+'"') -PassThru
        Start-Sleep -Seconds 6
        $rows=@(Snapshot $process ($case.id+'-input'))
        $main=$rows | Where-Object { $_.menus.Count -gt 0 } | Select-Object -First 1
        $command=$main.menus | Where-Object { $_.text -match '(SFC|SXF)' } | Select-Object -Last 1
        if ($null -ne $command) {
            [void][CadUI]::PostMessage([IntPtr]$main.handle,0x111,[IntPtr]$command.id,[IntPtr]::Zero)
            Start-Sleep -Seconds 3
            [void](Snapshot $process ($case.id+'-sfc-dialog'))
        }
        $report.cases+=@{id=$case.id;input_sha256=$before;source_unchanged=((Get-FileHash $input -Algorithm SHA256).Hash -eq $before);selected_command=$command}
        Stop-Process -Id $process.Id -Force
        $process=$null
    }
    $report.complete=$true
} finally {
    if ($null -ne $process -and -not $process.HasExited) { Stop-Process -Id $process.Id -Force }
    $report | ConvertTo-Json -Depth 12 | Set-Content -Encoding utf8 (Join-Path $root 'verification.json')
}
