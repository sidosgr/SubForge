!macro NSIS_HOOK_POSTINSTALL
  ExecWait '"$INSTDIR\SubForge.exe" --register-shell'
!macroend
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
  ExecWait '"$INSTDIR\SubForge.exe" --unregister-shell'
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    ExecWait '"$INSTDIR\SubForge.exe" --delete-user-data'
  ${EndIf}
!macroend
