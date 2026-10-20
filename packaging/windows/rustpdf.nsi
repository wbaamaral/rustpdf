; Instalador do RustPDF para Windows (NSIS 3).
; Gerado a partir de packaging/package.sh windows, que passa:
;   /DVERSION=x.y.z  /DSOURCE=<pasta do pacote portátil>  /DOUTFILE=<setup.exe>
;   /DICON=<rustpdf.ico> e /DLICENSE_FILE=<LICENSE.txt> (opcionais)
Unicode true
!include "MUI2.nsh"

!ifndef VERSION
  !error "Defina /DVERSION"
!endif
!ifndef SOURCE
  !error "Defina /DSOURCE"
!endif

Name "RustPDF ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\RustPDF"
InstallDirRegKey HKLM "Software\RustPDF" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

!ifdef ICON
  !define MUI_ICON "${ICON}"
  !define MUI_UNICON "${ICON}"
!endif
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\bin\rustpdf.exe"

!insertmacro MUI_PAGE_WELCOME
!ifdef LICENSE_FILE
  !insertmacro MUI_PAGE_LICENSE "${LICENSE_FILE}"
!endif
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "PortugueseBR"

!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustPDF"

Section "RustPDF (obrigatório)" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"
  File /r "${SOURCE}\*.*"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKLM "Software\RustPDF" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayName" "RustPDF"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "Publisher" "RustPDF"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\bin\rustpdf.exe"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Atalhos no menu Iniciar" SecStartMenu
  CreateDirectory "$SMPROGRAMS\RustPDF"
  CreateShortcut "$SMPROGRAMS\RustPDF\RustPDF.lnk" "$INSTDIR\bin\rustpdf.exe"
  CreateShortcut "$SMPROGRAMS\RustPDF\Desinstalar RustPDF.lnk" "$INSTDIR\Uninstall.exe"
SectionEnd

Section /o "Atalho na área de trabalho" SecDesktop
  CreateShortcut "$DESKTOP\RustPDF.lnk" "$INSTDIR\bin\rustpdf.exe"
SectionEnd

Section /o "Abrir arquivos .pdf com o RustPDF" SecAssoc
  WriteRegStr HKLM "Software\Classes\RustPDF.Document" "" "Documento PDF"
  WriteRegStr HKLM "Software\Classes\RustPDF.Document\DefaultIcon" "" "$INSTDIR\bin\rustpdf.exe,0"
  WriteRegStr HKLM "Software\Classes\RustPDF.Document\shell\open\command" "" '"$INSTDIR\bin\rustpdf.exe" "%1"'
  WriteRegStr HKLM "Software\Classes\.pdf\OpenWithProgids" "RustPDF.Document" ""
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\RustPDF\RustPDF.lnk"
  Delete "$SMPROGRAMS\RustPDF\Desinstalar RustPDF.lnk"
  RMDir "$SMPROGRAMS\RustPDF"
  Delete "$DESKTOP\RustPDF.lnk"
  DeleteRegKey HKLM "Software\Classes\RustPDF.Document"
  DeleteRegValue HKLM "Software\Classes\.pdf\OpenWithProgids" "RustPDF.Document"
  DeleteRegKey HKLM "${UNINSTALL_KEY}"
  DeleteRegKey HKLM "Software\RustPDF"
  RMDir /r "$INSTDIR"
SectionEnd
