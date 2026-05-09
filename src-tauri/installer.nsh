; Tauri NSIS installer hooks for clowder.
;
; Purpose: prune the dead "Other system tray icons" entry that Windows 11
; otherwise leaves behind under HKCU\Control Panel\NotifyIconSettings\<id>
; when the user uninstalls and then reinstalls to a different location. The
; default Tauri uninstaller does not touch this key, so the settings panel
; ends up showing two Clowder rows — one of them inert.
;
; A complementary first-run scan in the app (notify_icon_cleanup.rs) handles
; the case where someone skips the uninstaller and just deletes the folder.
;
; Path-format note: Windows stores ExecutablePath using a Known Folder GUID
; prefix for executables under any KF location (e.g.
;   {F1B32785-6FBA-4FCF-9D55-7B8E7F157091}\clowder\clowder.exe
; for an install under %LOCALAPPDATA%), not the literal "$INSTDIR\clowder.exe".
; So we can't directly compare against $INSTDIR. Instead we match any subkey
; whose ExecutablePath ends with "\clowder.exe" — the user is uninstalling
; clowder, so removing every clowder-flavored stale row is the right call.

!macro NSIS_HOOK_POSTUNINSTALL
  Push $R0      ; enumerator index
  Push $R1      ; subkey name
  Push $R2      ; ExecutablePath value
  Push $R3      ; suffix length / suffix start index
  Push $R4      ; extracted suffix
  StrCpy $R0 0
  clowder_notify_loop:
    ClearErrors
    EnumRegKey $R1 HKCU "Control Panel\NotifyIconSettings" $R0
    IfErrors clowder_notify_done
    StrCmp $R1 "" clowder_notify_done
    ReadRegStr $R2 HKCU "Control Panel\NotifyIconSettings\$R1" "ExecutablePath"
    StrCmp $R2 "" clowder_notify_next
    ; Compute the trailing 12 chars of $R2 ("\clowder.exe" = 12 chars).
    StrLen $R3 $R2
    IntCmp $R3 12 0 clowder_notify_next clowder_notify_check
    clowder_notify_check:
      IntOp $R3 $R3 - 12
      StrCpy $R4 $R2 12 $R3
      ; StrCmp is case-insensitive — fine for Windows paths.
      StrCmp $R4 "\clowder.exe" 0 clowder_notify_next
        DeleteRegKey HKCU "Control Panel\NotifyIconSettings\$R1"
        ; Don't bump the index — DeleteRegKey shifts subsequent entries down.
        Goto clowder_notify_loop
    clowder_notify_next:
      IntOp $R0 $R0 + 1
      Goto clowder_notify_loop
  clowder_notify_done:
  Pop $R4
  Pop $R3
  Pop $R2
  Pop $R1
  Pop $R0
!macroend
