param([int]$YukiId, [switch]$Avatar)
$ErrorActionPreference='Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Collections.Generic;
public static class YukiDragProbe {
 [StructLayout(LayoutKind.Sequential)] public struct RECT {public int Left,Top,Right,Bottom;}
 [StructLayout(LayoutKind.Sequential)] public struct POINT {public int X,Y;}
 public delegate bool EnumProc(IntPtr h,IntPtr p);
 [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb,IntPtr p);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out RECT r);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern void mouse_event(uint flags,uint x,uint y,uint data,UIntPtr info);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr insert,int x,int y,int cx,int cy,uint flags);
 public static IntPtr Find(uint pid,bool avatar) { IntPtr result=IntPtr.Zero;EnumWindows((h,p)=>{uint id;GetWindowThreadProcessId(h,out id);RECT r;GetWindowRect(h,out r);if(id==pid && IsWindowVisible(h) && (avatar ? r.Right-r.Left>=400 && r.Right-r.Left<700 : r.Right-r.Left>=800)) result=h;return true;},IntPtr.Zero);return result; }
}
'@
$handle=[YukiDragProbe]::Find($YukiId,$Avatar.IsPresent)
if($handle -eq [IntPtr]::Zero){throw 'Yuki window not found'}
$before=New-Object YukiDragProbe+RECT
$cursor=New-Object YukiDragProbe+POINT
[void][YukiDragProbe]::GetWindowRect($handle,[ref]$before)
[void][YukiDragProbe]::GetCursorPos([ref]$cursor)
$startX=if($Avatar){$before.Left+($before.Right-$before.Left)/2}else{$before.Left+160}
$startY=if($Avatar){$before.Top+($before.Bottom-$before.Top)*.45}else{$before.Top+23}
try {
 [void][YukiDragProbe]::SetForegroundWindow($handle)
 [void][YukiDragProbe]::SetCursorPos($startX,$startY)
 Start-Sleep -Milliseconds 250
 [YukiDragProbe]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
 Start-Sleep -Milliseconds 200
 for($step=1;$step -le 10;$step++){[void][YukiDragProbe]::SetCursorPos($startX+$step*10,$startY-$step*3);Start-Sleep -Milliseconds 30}
 [YukiDragProbe]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
 Start-Sleep -Milliseconds 500
 $after=New-Object YukiDragProbe+RECT
 [void][YukiDragProbe]::GetWindowRect($handle,[ref]$after)
 [pscustomobject]@{avatar=$Avatar.IsPresent;left=$before.Left;top=$before.Top;width=$before.Right-$before.Left;height=$before.Bottom-$before.Top;deltaX=$after.Left-$before.Left;deltaY=$after.Top-$before.Top;passed=($after.Left -ne $before.Left -or $after.Top -ne $before.Top)} | ConvertTo-Json
} finally {
 [YukiDragProbe]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
 [void][YukiDragProbe]::SetWindowPos($handle,[IntPtr]::Zero,$before.Left,$before.Top,0,0,0x15)
 [void][YukiDragProbe]::SetCursorPos($cursor.X,$cursor.Y)
}
