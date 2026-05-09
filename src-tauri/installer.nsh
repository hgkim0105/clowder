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

!macro NSIS_HOOK_POSTUNINSTALL
  Push $R0
  Push $R1
  Push $R2
  StrCpy $R0 0
  clowder_notify_loop:
    ClearErrors
    EnumRegKey $R1 HKCU "Control Panel\NotifyIconSettings" $R0
    IfErrors clowder_notify_done
    StrCmp $R1 "" clowder_notify_done
    ReadRegStr $R2 HKCU "Control Panel\NotifyIconSettings\$R1" "ExecutablePath"
    ; StrCmp is case-insensitive, which is what we want for Windows paths.
    StrCmp $R2 "$INSTDIR\clowder.exe" 0 clowder_notify_next
      DeleteRegKey HKCU "Control Panel\NotifyIconSettings\$R1"
      ; Don't bump the index — DeleteRegKey shifts subsequent entries down.
      Goto clowder_notify_loop
    clowder_notify_next:
      IntOp $R0 $R0 + 1
      Goto clowder_notify_loop
  clowder_notify_done:
  Pop $R2
  Pop $R1
  Pop $R0
!macroend
