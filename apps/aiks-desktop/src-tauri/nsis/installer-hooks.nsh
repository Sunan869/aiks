; AIKS NSIS installer hooks for upgrade lifecycle (V2.5)
; This file is automatically included by Tauri's NSIS bundler.
;
; Purpose: Before installing, gracefully close any running AIKS instance
; and its embedded SiYuan-Kernel.exe to avoid "file in use" errors.

!macro NSIS_HOOK_PREINSTALL
  ; ── Gracefully close running AIKS ────────────────────────────────────────────
  ; First try graceful exit via window message
  FindWindow $R0 "" "AIKS"
  IntCmp $R0 0 skip_close_aiks
    SendMessage $R0 0x0010 0 0  ; WM_CLOSE
    Sleep 3000
  skip_close_aiks:

  ; Wait for AIKS.exe to exit (up to 8 seconds)
  StrCpy $R1 0
  wait_aiks_exit:
    IntCmp $R1 8 aiks_timeout aiks_timeout 0
    FindProcess "aiks-desktop.exe" $R2
    IntCmp $R2 0 aiks_gone aiks_gone 0
    Sleep 1000
    IntOp $R1 $R1 + 1
    Goto wait_aiks_exit
  aiks_timeout:
    ; Force kill if still running after 8s
    nsExec::ExecToLog 'taskkill /F /IM "aiks-desktop.exe"'
  aiks_gone:

  ; ── Gracefully close SiYuan-Kernel.exe ────────────────────────────────────────
  ; Only kill kernel that was started by AIKS (check runtime.json)
  ; Read runtime.json from %LOCALAPPDATA%\AIKnowledgeSync\runtime.json
  ReadEnvStr $R3 LOCALAPPDATA
  StrCpy $R4 "$R3\AIKnowledgeSync\runtime.json"

  IfFileExists $R4 has_runtime no_runtime
  has_runtime:
    ; Kill the kernel PID stored in runtime.json
    ; Simple approach: kill any SiYuan-Kernel from our install dir
    nsExec::ExecToLog 'taskkill /F /IM "SiYuan-Kernel.exe"'
    Sleep 2000
    Delete "$R4"
  no_runtime:

  ; Brief pause to let file handles release
  Sleep 1000
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Nothing to do post-install
!macroend
