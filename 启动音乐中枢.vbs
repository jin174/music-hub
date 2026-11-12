Set fso = CreateObject("Scripting.FileSystemObject")
dir = fso.GetParentFolderName(WScript.ScriptFullName)
exePath = fso.BuildPath(dir, "MusicHub.exe")
If fso.FileExists(exePath) Then
    CreateObject("WScript.Shell").Run """" & exePath & """", 1, False
Else
    hub = fso.BuildPath(dir, "hub.py")
    pyw = "pythonw"
    If fso.FileExists("D:\Program Files\python\pythonw.exe") Then pyw = "D:\Program Files\python\pythonw.exe"
    CreateObject("WScript.Shell").Run """" & pyw & """ """ & hub & """", 0, False
End If
